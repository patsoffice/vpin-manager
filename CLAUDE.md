# VPin Manager

A virtual pinball resource library manager built in Rust. Browse, search, download, import, and organize resources from the [Virtual Pinball Spreadsheet](https://virtualpinballspreadsheet.github.io/) database.

## Build & Test

```bash
cargo build                                                     # Build entire workspace
cargo test                                                      # Run all tests
cargo test -p vpin-manager-core                                 # Test a single crate
cargo clippy --all-features --all-targets                       # Check code quality
cargo clippy --all-features --all-targets --allow-dirty --fix   # Auto-fix clippy warnings
cargo fmt                                                       # Format code
cargo run -p vpin-manager-cli -- sync                           # Run CLI: fetch/refresh VPS database
cargo run -p vpin-manager-cli -- search "Medieval Madness"      # Run CLI: search for a game
cargo run -p vpin-manager-web                                   # Run the web frontend
```

- All tests must pass before committing
- `cargo clippy` must pass with no warnings
- `cargo fmt` must pass with no formatting changes

## Architecture

Workspace with three crates:

- **vpin-manager-core** -- library crate with all business logic (VPS DB models/fetch/search, library management, file organization)
- **vpin-manager-cli** -- binary crate, thin CLI using clap with subcommands (`sync`, `search`, `show`, `import`, `library`, `config`)
- **vpin-manager-web** -- binary crate, HTMX web UI (stub, planned for later)

Key design decisions:

- **VPS database** fetched from remote JSON, cached locally, loaded in-memory as `Vec<Game>`
- **SQLite** (rusqlite, bundled) for tracking installed resources, settings, download history
- **Pure Rust dependencies** preferred -- rustls for TLS, zip/sevenz_rust2/unrar for archives

## Key Data Sources

- VPS database: `https://virtualpinballspreadsheet.github.io/vps-db/db/vpsdb.json` (~6.7MB, ~2,500 games)
- Freshness check: `https://virtualpinballspreadsheet.github.io/vps-db/lastUpdated.json` (Unix ms timestamp)
- Fetch `lastUpdated.json` first to avoid re-downloading the full DB unnecessarily

## Commit Style

- Prefix: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`
- Summary line under 80 chars with counts where relevant
- Body: each logical change on its own `-` bullet
- Summarize what was added/changed and why, not just file names
