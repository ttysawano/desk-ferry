# DeskFerry Protocol

Stage 2 defines protocol data types, JSON Lines encode/decode, and the
authentication messages used after TLS is established. It does not implement
input capture, input suppression, or input injection.

Every message carries `protocol_version`. The current protocol version is `1`.

## Messages

- `auth_challenge`
- `auth_response`
- `auth_result`
- `hello`
- `mouse_move`
- `mouse_warp`
- `mouse_button`
- `key`
- `boundary_request`
- `active_host_changed`
- `release_all`

Messages are serialized as one JSON object per line. Unknown message types and version mismatches are rejected by the decoder.

## Authentication Flow

Authentication messages are exchanged only after the TCP connection has been
wrapped in TLS.

1. Server sends `auth_challenge`.
2. Client sends `auth_response`.
3. Server verifies the response and sends `auth_result`.
4. Client sends `hello` only when `auth_result.success` is `true`.
5. Input protocol messages are accepted only after authentication succeeds.

`auth_challenge` contains a fresh hex nonce.

`auth_response` contains the client host name and
`HMAC-SHA256(psk, nonce || host_name || protocol_version)`, encoded as hex.

`auth_result` contains a boolean result and a generic reason string. Normal logs
must not include PSK material or HMAC values.

## Protocol Dump

`desk-ferry-common` provides a safe protocol dump summary helper for JSON Lines.
It classifies message types without printing key codes, text input, concrete
keystrokes, PSKs, or HMAC values.

## Logging Rule

Key events may be counted or classified generically, but key codes, characters, text input, and concrete keystroke sequences must not be emitted by normal logs.
