# DeskFerry Protocol

Stage 1 defines protocol data types and JSON Lines encode/decode only. It does not implement TCP, TLS, authentication, input capture, input suppression, or input injection.

Every message carries `protocol_version`. The current Stage 1 version is `1`.

## Messages

- `hello`
- `mouse_move`
- `mouse_warp`
- `mouse_button`
- `key`
- `boundary_request`
- `active_host_changed`
- `release_all`

Messages are serialized as one JSON object per line. Unknown message types and version mismatches are rejected by the decoder.

## Logging Rule

Key events may be counted or classified generically, but key codes, characters, text input, and concrete keystroke sequences must not be emitted by normal logs.

