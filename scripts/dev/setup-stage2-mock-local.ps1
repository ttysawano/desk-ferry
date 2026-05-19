param(
    [string]$LocalDir = ".local",
    [int]$Port = 24800
)

$ErrorActionPreference = "Stop"

function Resolve-OpenSsl {
    $command = Get-Command "openssl" -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) {
        return $command.Source
    }

    $candidatePaths = @()
    if ($env:ProgramFiles) {
        $candidatePaths += (Join-Path $env:ProgramFiles "Git\usr\bin\openssl.exe")
        $candidatePaths += (Join-Path $env:ProgramFiles "Git\mingw64\bin\openssl.exe")
    }
    if (${env:ProgramFiles(x86)}) {
        $candidatePaths += (Join-Path ${env:ProgramFiles(x86)} "Git\usr\bin\openssl.exe")
        $candidatePaths += (Join-Path ${env:ProgramFiles(x86)} "Git\mingw64\bin\openssl.exe")
    }

    foreach ($candidatePath in $candidatePaths) {
        if (Test-Path -LiteralPath $candidatePath -PathType Leaf) {
            return $candidatePath
        }
    }

    $checkedPaths = if ($candidatePaths.Count -gt 0) {
        " Checked Git for Windows candidates: $($candidatePaths -join ', ')"
    } else {
        ""
    }

    throw "openssl.exe was not found on PATH or in Git for Windows install locations.$checkedPaths Install OpenSSL, install Git for Windows, or add openssl.exe to PATH."
}

function Write-TextNoNewline {
    param(
        [string]$Path,
        [string]$Text
    )

    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Text, $utf8NoBom)
}

function Convert-ToTomlPath {
    param([string]$Path)

    return $Path.Replace("\", "/")
}

$openssl = Resolve-OpenSsl

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

New-Item -ItemType Directory -Force $LocalDir | Out-Null

$pskPath = Join-Path $LocalDir "stage2-test.psk"
$keyPath = Join-Path $LocalDir "stage2-server.key"
$certPath = Join-Path $LocalDir "stage2-server.crt"
$pskTomlPath = Convert-ToTomlPath $pskPath
$keyTomlPath = Convert-ToTomlPath $keyPath
$certTomlPath = Convert-ToTomlPath $certPath
$serverLocal = "examples/server-windows.local.toml"
$clientLocal = "examples/client-linux-x11.local.toml"

$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
try {
    $rng.GetBytes($bytes)
}
finally {
    $rng.Dispose()
}
Write-TextNoNewline $pskPath ([Convert]::ToBase64String($bytes))

& $openssl req -x509 -newkey rsa:3072 -nodes `
    -keyout $keyPath `
    -out $certPath `
    -days 7 `
    -subj "/CN=localhost" | Out-Null

$cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
    (Resolve-Path $certPath)
)
$sha256 = [System.Security.Cryptography.SHA256]::Create()
try {
    $fingerprint = [BitConverter]::ToString($sha256.ComputeHash($cert.RawData)).Replace("-", "").ToLowerInvariant()
}
finally {
    $sha256.Dispose()
}

Copy-Item "examples/server-windows.toml" $serverLocal -Force
Copy-Item "examples/client-linux-x11.toml" $clientLocal -Force

$serverText = Get-Content $serverLocal -Raw
$serverText = $serverText `
    -replace 'bind_address = "0\.0\.0\.0"', 'bind_address = "127.0.0.1"' `
    -replace 'port = 24800', "port = $Port" `
    -replace 'psk_file = "\./secret\.psk"', "psk_file = `"$pskTomlPath`"" `
    -replace '# cert_file = "\./local/server\.crt"', "cert_file = `"$certTomlPath`"" `
    -replace '# key_file = "\./local/server\.key"', "key_file = `"$keyTomlPath`""
Write-TextNoNewline $serverLocal $serverText

$clientText = Get-Content $clientLocal -Raw
$clientText = $clientText `
    -replace 'host = "192\.168\.1\.10"', 'host = "localhost"' `
    -replace 'port = 24800', "port = $Port" `
    -replace 'psk_file = "\./secret\.psk"', "psk_file = `"$pskTomlPath`"" `
    -replace '# server_fingerprint = "<sha256-server-cert-fingerprint>"', "server_fingerprint = `"$fingerprint`""
Write-TextNoNewline $clientLocal $clientText

Write-Host "Created local Stage 2 mock files:"
Write-Host "  $serverLocal"
Write-Host "  $clientLocal"
Write-Host "  $pskPath"
Write-Host "  $certPath"
Write-Host "  $keyPath"
Write-Host ""
Write-Host "Run mock-server:"
Write-Host "  cargo run -p desk-ferry-mock-server -- $serverLocal"
Write-Host ""
Write-Host "Run mock-client:"
Write-Host "  cargo run -p desk-ferry-mock-client -- $clientLocal"
