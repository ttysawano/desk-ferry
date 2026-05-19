# Configuration

DeskFerry uses TOML configuration files.

Stage 1 supports:

- Windows server configuration
- Linux X11 client configuration
- TLS-only security mode declarations
- PSK file path declarations
- neighbor specifications in the form `<target_host>:<target_edge>`

An empty neighbor value means no transition for that edge.

Valid edges are:

- `left`
- `right`
- `up`
- `down`

The common crate validates structure and policy. It does not check live displays, PSK file permissions, X11 availability, or Windows monitor state in Stage 1.

