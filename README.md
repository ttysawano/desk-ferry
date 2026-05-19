# DeskFerry

DeskFerry is a Rust workspace for a TCP/IP input sharing application scoped to a Windows 10/11 host and Linux X11 client.

This repository is currently at Stage 2. It contains repository structure,
shared protocol/configuration types, JSON Lines helpers, configuration loading
and validation, safe logging helpers, TLS transport helpers, server certificate
fingerprint verification, PSK challenge-response authentication, mock
server/client crates, active host state management, and unit tests.

## Supported Scope

- Server / host: Windows 10/11
- Client: Linux on X11
- Initial layout: one Windows host and one Linux X11 client
- Configuration format: TOML
- Protocol framing: JSON Lines
- Secure transport: TLS plus server certificate fingerprint pinning
- Authentication: PSK challenge-response over TLS

## Stage 2 Capabilities

- `auth_challenge`, `auth_response`, and `auth_result` protocol messages
- TLS helper functions for mock server/client transport
- SHA-256 server certificate fingerprint verification
- HMAC-SHA256 PSK challenge-response authentication
- Authentication gates that reject input events before authentication succeeds
- Mock active host changes and disconnect fail-safe release behavior
- Safe protocol dump summaries that do not print key input details or secrets

## Out of Scope

- Linux host
- Windows client
- macOS
- Wayland
- GUI configuration tools
- Clipboard sharing
- Plaintext TCP as a normal runtime path
- Custom cryptography
- OS input capture, suppression, or injection
- Windows API implementation
- X11 API implementation

## Workspace

```text
crates/
  desk-ferry-common/
  desk-ferry-server-windows/
  desk-ferry-client-x11/
  desk-ferry-mock-server/
  desk-ferry-mock-client/
```

`desk-ferry-common` contains shared Stage 1 and Stage 2 logic. The mock crates
exercise secure transport and authentication only. The Windows and Linux X11
application crates are still placeholders for later stages.

## Mock Transport

The mock binaries require local PSK and certificate files that are not committed
to this repository. Copy the examples to ignored local files, then set
`security.psk_file`, `security.cert_file`, `security.key_file`, and
`security.server_fingerprint` in those local files.

```powershell
Copy-Item examples/server-windows.toml examples/server-windows.local.toml
Copy-Item examples/client-linux-x11.toml examples/client-linux-x11.local.toml
```

The detailed PSK, certificate, and fingerprint generation steps are in
`docs/testing.md`. On Windows with OpenSSL installed, the helper script can set
up local ignored test files:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/dev/setup-stage2-mock-local.ps1
```

Then run:

```powershell
cargo run -p desk-ferry-mock-server -- examples/server-windows.local.toml
cargo run -p desk-ferry-mock-client -- examples/client-linux-x11.local.toml
```

The mock transport does not capture, suppress, or inject real input.

## Tests

When Rust is installed:

```powershell
cargo test --workspace
```
