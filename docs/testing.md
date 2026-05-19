# Testing

Stage 1 tests are pure unit tests in `desk-ferry-common`.

Required coverage:

- config loading
- invalid config rejection
- neighbor parsing
- protocol JSON Lines encode/decode
- unknown message rejection
- protocol version mismatch rejection
- safe logger does not expose key input contents

Run:

```powershell
cargo test --workspace
```

