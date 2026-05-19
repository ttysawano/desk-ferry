# Testing

Stage 1 and Stage 2 tests are pure unit tests in `desk-ferry-common`.

Required coverage:

- config loading
- invalid config rejection
- neighbor parsing
- protocol JSON Lines encode/decode
- unknown message rejection
- protocol version mismatch rejection
- safe logger does not expose key input contents

Stage 2 coverage:

- correct PSK authentication succeeds
- wrong PSK authentication fails
- unallowed host authentication fails
- input events are rejected before authentication
- nonce values change across challenges
- HMAC verification failure is rejected
- server certificate fingerprints can be verified
- fingerprint mismatches are rejected
- plaintext normal mode is disabled by configuration policy
- `auth_challenge`, `auth_response`, and `auth_result` encode/decode
- post-authentication protocol messages encode/decode
- authenticated state and active host transitions are tracked
- disconnect returns active host to the server and produces `release_all`
- safe logs and protocol dump summaries do not expose PSKs, HMACs, key codes, or concrete keystrokes

Run:

```powershell
cargo test --workspace
```

## Manual Mock Transport Check

The manual mock check verifies only the Stage 2 transport path:

```text
mock-client -> TCP -> TLS -> certificate fingerprint check
  -> PSK challenge-response
  -> hello / mock protocol messages
```

It does not use Windows APIs, X11 APIs, input capture, input suppression, or
input injection.

### Local Files

Create local copies of the example configuration files and keep all generated
secrets out of Git:

```powershell
Copy-Item examples/server-windows.toml examples/server-windows.local.toml
Copy-Item examples/client-linux-x11.toml examples/client-linux-x11.local.toml
```

The repository `.gitignore` must ignore these local artifacts:

- `*.local.toml`
- `*.psk`
- `*.pem`
- `*.key`
- `*.crt`

### PSK

Generate a local-only PSK. Put it outside the repository or in a local ignored
file such as `.local/stage2-test.psk`.

PowerShell example:

```powershell
New-Item -ItemType Directory -Force .local
$bytes = New-Object byte[] 32
[System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
[Convert]::ToBase64String($bytes) | Set-Content -NoNewline .local/stage2-test.psk
```

Use the same `psk_file` path in both local TOML files.

### Self-Signed Certificate

For a localhost-only mock check, generate a self-signed certificate and private
key. This requires OpenSSL to be available in the terminal.

```powershell
openssl req -x509 -newkey rsa:3072 -nodes `
  -keyout .local/stage2-server.key `
  -out .local/stage2-server.crt `
  -days 7 `
  -subj "/CN=localhost"
```

Compute the SHA-256 certificate fingerprint from the DER certificate bytes:

```powershell
$cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
  (Resolve-Path .local/stage2-server.crt)
)
$fingerprint = [Convert]::ToHexString(
  [System.Security.Cryptography.SHA256]::HashData($cert.RawData)
).ToLowerInvariant()
$fingerprint
```

### Local Config

In `examples/server-windows.local.toml`, use:

```toml
[security]
mode = "tls"
psk_file = ".local/stage2-test.psk"
allow_plaintext = false
cert_file = ".local/stage2-server.crt"
key_file = ".local/stage2-server.key"
```

For a single-machine check, set:

```toml
[network]
bind_address = "127.0.0.1"
port = 24800
```

In `examples/client-linux-x11.local.toml`, use the same PSK path and paste the
fingerprint:

```toml
[server]
host = "localhost"
port = 24800

[security]
mode = "tls"
psk_file = ".local/stage2-test.psk"
allow_plaintext = false
server_fingerprint = "<sha256-server-cert-fingerprint>"
```

`cert_file` and `key_file` are used only by `desk-ferry-mock-server`.
`server_fingerprint` is used only by `desk-ferry-mock-client`.

### Run

Open two terminals.

Terminal 1:

```powershell
cargo run -p desk-ferry-mock-server -- examples/server-windows.local.toml
```

Terminal 2:

```powershell
cargo run -p desk-ferry-mock-client -- examples/client-linux-x11.local.toml
```

The mock transport only exercises TLS, fingerprint pinning, PSK authentication,
JSON Lines protocol exchange, active host state, and fail-safe release behavior.
It does not perform Windows input capture, Windows input suppression, or X11
input injection.

### Helper Script

On Windows, the helper script below creates `.local/`, generates a local PSK and
self-signed certificate, copies the example TOML files to ignored
`*.local.toml` files, and writes the local paths and fingerprint:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/dev/setup-stage2-mock-local.ps1
```

It requires OpenSSL. The generated `.local/*` files and `*.local.toml` files are
for local testing only and must not be committed.
