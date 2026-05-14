# GeoGit

**Distributed version control for geospatial data.**

GeoGit brings Git-like branching, diffing, merging, and collaboration to geodata. Import GeoPackages, edit in QGIS, commit changes, push/pull between machines — all with row-level tracking.

## Quick Start

```bash
# Initialize a repo and import a GeoPackage
geogit init myproject --import GPKG:parcels.gpkg
cd myproject

# Check status
geogit status

# Commit
geogit commit -m "Initial import"

# Branch and edit
geogit branch feature/update-parcels
geogit switch feature/update-parcels
# ... edit the working copy GeoPackage in QGIS ...
geogit commit -m "Update parcel boundaries"

# Merge
geogit switch main
geogit merge feature/update-parcels

# Inspect data
geogit data ls
geogit data info parcels
geogit data schema parcels

# Export
geogit export parcels output.gpkg
geogit export parcels output.geojson
geogit export parcels output.csv
```

## File & Document Version Control

Version arbitrary files alongside geospatial datasets:

```bash
# Add files to a file dataset
geogit files add report.pdf spec.docx

# Add to a custom dataset
geogit files add --dataset documents report.pdf

# List tracked files
geogit files ls

# Remove a file
geogit files rm report.pdf

# Commit as usual
geogit commit -m "Add project documents"
```

## Dataset Metadata & Licensing

Attach ISO 19115 XML metadata and license information to any dataset:

```bash
# Set XML metadata (ISO 19115, FGDC, or any XML)
geogit metadata set parcels metadata.xml
geogit metadata show parcels

# Set license (text or XML)
geogit license set parcels LICENSE.txt
geogit license show parcels

# Works with both table and file datasets
geogit metadata set documents meta.xml
```

## How It Works

GeoGit stores every feature row as a [MessagePack](https://msgpack.org/)-encoded blob inside a standard Git repository. The storage format is compatible with [Kart](https://kartproject.org/):

```
myproject/
├── .git/                       # Standard Git repository
├── .gitignore                  # Excludes working copy (.gpkg)
├── parcels/
│   └── .table-dataset/
│       ├── meta/
│       │   ├── title
│       │   ├── description
│       │   ├── schema.json     # Column definitions
│       │   ├── metadata.xml    # ISO 19115 metadata (optional)
│       │   ├── license         # License text (optional)
│       │   ├── legend/         # Schema evolution history
│       │   └── crs/            # Coordinate reference systems
│       └── feature/
│           ├── A/A/A/B/kU0=    # Feature with PK=77
│           └── ...             # One blob per row
├── documents/
│   └── .file-dataset/
│       ├── meta/
│       │   ├── title
│       │   └── metadata.xml    # Optional metadata
│       └── files/
│           ├── report.pdf      # Versioned files
│           └── spec.docx
└── myproject.gpkg              # Working copy (editable in QGIS)
```

**Key properties:**
- **Git deduplication** — unchanged features share blobs across commits (zero cost)
- **Efficient diffs** — compare blob OIDs, skip unchanged subtrees: O(changed) not O(total)
- **Standard remotes** — push/pull to GitHub, GitLab, or any Git host
- **Edit anywhere** — working copy is a GeoPackage, editable in any GIS software
- **Schema evolution** — legends enable reading old features after schema changes
- **File tracking** — version documents and files alongside geodata

## Supported Formats

| Format | Import | Export |
|--------|--------|--------|
| GeoPackage (.gpkg) | ✅ | ✅ |
| Shapefile (.shp) | ✅ | — |
| GeoJSON | — | ✅ |
| CSV | — | ✅ |
| PostGIS | ✅ | — |
| Files (any) | ✅ | ✅ |

## Commands

| Command | Description |
|---------|-------------|
| `geogit init [dir]` | Initialize a new repository |
| `geogit clone <url>` | Clone a remote repository |
| `geogit import GPKG:file.gpkg` | Import a GeoPackage dataset |
| `geogit status` | Show working copy changes |
| `geogit diff [base] [target]` | Feature-level diffs |
| `geogit commit -m "msg"` | Commit changes |
| `geogit log [--oneline] [-n N]` | Show commit history |
| `geogit show [commit]` | Display commit details |
| `geogit branch [name] [-d]` | List/create/delete branches |
| `geogit switch <branch> [-c]` | Switch branches |
| `geogit merge <branch>` | Merge a branch |
| `geogit push [remote] [branch]` | Push to a remote |
| `geogit pull [remote] [branch]` | Pull from a remote |
| `geogit remote add\|remove\|ls` | Manage remotes |
| `geogit reset [target]` | Reset to a commit |
| `geogit restore <datasets>` | Restore datasets from a commit |
| `geogit checkout [datasets]` | Checkout tree to working copy |
| `geogit export <ds> <path>` | Export to GPKG/GeoJSON/CSV |
| `geogit data ls\|info\|schema` | Inspect datasets |
| `geogit files add\|ls\|rm` | Manage versioned files |
| `geogit metadata set\|show` | Dataset XML metadata |
| `geogit license set\|show` | Dataset license management |
| `geogit conflicts [ls\|abort]` | View/manage merge conflicts |
| `geogit resolve [paths]` | Resolve conflicts |

## Building

```bash
cargo build --release
# Binary at target/release/geogit
```

## Architecture

Five Rust crates in a workspace:

| Crate | Purpose |
|-------|---------|
| `geogit-encoding` | MessagePack feature encoding, geometry, paths, schemas |
| `geogit-core` | Dataset model, diff engine, three-way merge |
| `geogit-git` | Git object storage via shell git |
| `geogit-wc` | Working copy adapters (GeoPackage, PostGIS) |
| `geogit` (CLI) | Command-line interface via [clap](https://github.com/clap-rs/clap) |

## License

AGPL-3.0-or-later
