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

Generate local-only test credentials outside the repository, create a PSK file
outside the repository, and set these config keys:

```toml
[security]
mode = "tls"
psk_file = "C:/path/outside/repo/secret.psk"
allow_plaintext = false
cert_file = "C:/path/outside/repo/server.crt"
key_file = "C:/path/outside/repo/server.key"
server_fingerprint = "<sha256-server-cert-fingerprint>"
```

`cert_file` and `key_file` are used by `desk-ferry-mock-server`.
`server_fingerprint` is used by `desk-ferry-mock-client`.

Then run:

```powershell
cargo run -p desk-ferry-mock-server -- examples/server-windows.toml
cargo run -p desk-ferry-mock-client -- examples/client-linux-x11.toml
```

The mock transport only exercises TLS, fingerprint pinning, PSK authentication,
JSON Lines protocol exchange, active host state, and fail-safe release behavior.
It does not perform Windows input capture, Windows input suppression, or X11
input injection.
