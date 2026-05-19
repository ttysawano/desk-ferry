# DeskFerry

DeskFerry is a Rust workspace for a TCP/IP input sharing application scoped to a Windows 10/11 host and Linux X11 client.

This repository is currently at Stage 1. It contains only repository structure, shared protocol/configuration types, JSON Lines helpers, configuration loading and validation, safe logging helpers, and unit tests.

## Supported Scope

- Server / host: Windows 10/11
- Client: Linux on X11
- Initial layout: one Windows host and one Linux X11 client
- Configuration format: TOML
- Protocol framing: JSON Lines

## Out of Scope

- Linux host
- Windows client
- macOS
- Wayland
- GUI configuration tools
- Clipboard sharing
- Plaintext TCP as a normal runtime path
- Custom cryptography
- OS input capture, suppression, or injection in the common crate

## Workspace

```text
crates/
  desk-ferry-common/
  desk-ferry-server-windows/
  desk-ferry-client-x11/
  desk-ferry-mock-server/
  desk-ferry-mock-client/
```

Only `desk-ferry-common` contains real Stage 1 logic. The other crates are placeholders for later stages.

## Tests

When Rust is installed:

```powershell
cargo test --workspace
```

