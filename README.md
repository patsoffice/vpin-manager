# vpin-manager

A virtual pinball resource library manager. Browse, search, download, import, and organize resources from the [Virtual Pinball Spreadsheet](https://virtualpinballspreadsheet.github.io/) database.

## Features

- **Browse & Search** -- Query the VPS database (~2,500 games) by name, manufacturer, year, type, and table format
- **Library Management** -- Track installed resources, check for updates against the VPS database
- **Import** -- Scan existing directories for virtual pinball files, fuzzy-match them to VPS entries, and register them in your library
- **File Organization** -- Move and organize files into platform-specific folder structures with built-in presets for VPX and VPX-standalone, plus custom profiles
- **Archive Extraction** -- Extract ZIP, 7z, and RAR archives natively
- **Authenticated Downloads** -- Store credentials for VPUniverse and VPForums to automate downloads (planned)
- **Web UI** -- HTMX-based local web interface for browsing and managing your library (planned)

## Resource Types

The VPS database tracks 13 resource types per game:

| Type | Description |
| ---- | ----------- |
| Tables | Virtual pinball table files (.vpx, .fp, etc.) |
| Backglasses | B2S backglass artwork and animations |
| ROMs | Game ROM files for VPinMAME |
| Wheel Art | Cabinet wheel/marquee artwork |
| Toppers | Topper video/LED content |
| PuP Packs | PinUP Popper customization packs |
| Alt Sound | Alternative sound packs |
| Alt Color | Alternative DMD color schemes |
| POV | Point-of-view configuration files |
| Media Packs | Complete media packages |
| Rules | Game rule documentation |
| Sound | Sound/audio files |
| Tutorials | Guides and instruction resources |

## Building

Requires Rust 1.85+.

```sh
cargo build --release
```

All dependencies are pure Rust (TLS via rustls, SQLite compiled from source via bundled feature). No system libraries required.

## Testing

```sh
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p vpin-manager-core

# Run the full VPS database integration test (downloads ~6.7MB from the network)
cargo test -p vpin-manager-core -- --ignored --nocapture
```

The integration test (`parse_real_vpsdb`) is marked `#[ignore]` by default since it fetches the full VPS database on every run. Use `--ignored` to include it.

## Usage

### Sync the VPS database

```sh
# Fetch the database (skips download if already up to date)
vpin-manager sync

# Force a full re-download
vpin-manager sync --force
```

### Search for games

```sh
# Search by name
vpin-manager search "Medieval Madness"

# Filter by manufacturer and table format
vpin-manager search --manufacturer "Data East" --format VPX

# Filter by author (searches across all resource types)
vpin-manager search --author "JPSalas"

# Filter by year
vpin-manager search --year 1992

# Combine filters
vpin-manager search "hook" --manufacturer "Data East" --format VPX

# Sort by last updated, show more results
vpin-manager search --sort updated --limit 50
```

### Show game details

```sh
# By game ID
vpin-manager show 1IlVLynt

# By name (if unique match)
vpin-manager show "Hook"
```

Shows all resources grouped by type with versions, authors, features, and download URLs.

### Configuration

```sh
# Show current configuration
vpin-manager config

# Show config file path
vpin-manager config --path

# Create default config file
vpin-manager config --init
```

### Coming soon

```sh
# Import existing files into your library (planned)
vpin-manager import ~/VPinball/Tables

# List installed resources (planned)
vpin-manager library

# Check for updates (planned)
vpin-manager library status
```

## Export Profiles

vpin-manager ships with built-in folder structure presets for different platforms:

**VPX (standard install)**:

```text
Tables/              # .vpx + .directb2s files
VPinMAME/roms/       # ROM archives
VPinMAME/altcolor/   # Alt color files
VPinMAME/altsound/   # Alt sound packs
PinUPSystem/PUPVideos/  # PuP packs
```

**VPX-standalone**:

```text
tables/              # .vpx + .directb2s files
roms/                # ROM archives
altcolor/            # Alt color files
altsound/            # Alt sound packs
pupvideos/           # PuP packs
```

Custom profiles can be created and saved in the configuration file.

## Data Sources

- Game and resource metadata: [VPS Database](https://virtualpinballspreadsheet.github.io/vps-db/db/vpsdb.json)
- Database freshness: [lastUpdated.json](https://virtualpinballspreadsheet.github.io/vps-db/lastUpdated.json)

## License

MIT
