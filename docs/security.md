# Security Notes

Stage 1 intentionally avoids transport implementation. The repository must not contain real PSKs, private keys, certificates, access tokens, or production secrets.

Required constraints:

- Do not use plaintext TCP as the normal runtime path.
- Do not implement custom cryptography.
- Do not log key input content, text input, specific key codes, or concrete keystroke sequences.
- Keep OS-dependent APIs out of `desk-ferry-common`.

`examples/secret.psk.example` contains only a dummy placeholder.

