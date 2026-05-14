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
```

## How It Works

GeoGit stores every feature row as a [MessagePack](https://msgpack.org/)-encoded blob inside a standard Git repository. The storage format is compatible with [Kart](https://kartproject.org/):

```
myproject/
├── .git/                       # Standard Git repository
├── parcels/
│   └── .table-dataset/
│       ├── meta/
│       │   ├── title
│       │   ├── description
│       │   ├── schema.json     # Column definitions
│       │   ├── legend/         # Schema evolution history
│       │   └── crs/            # Coordinate reference systems
│       └── feature/
│           ├── A/A/A/B/kU0=    # Feature with PK=77
│           └── ...             # One blob per row
└── myproject.gpkg              # Working copy (editable in QGIS)
```

**Key properties:**
- **Git deduplication** — unchanged features share blobs across commits (zero cost)
- **Efficient diffs** — compare blob OIDs, skip unchanged subtrees: O(changed) not O(total)
- **Standard remotes** — push/pull to GitHub, GitLab, or any Git host
- **Edit anywhere** — working copy is a GeoPackage, editable in any GIS software
- **Schema evolution** — legends enable reading old features after schema changes

## Supported Formats

| Format | Import | Export |
|--------|--------|--------|
| GeoPackage (.gpkg) | ✅ | ✅ |
| PostGIS | 🔜 | 🔜 |
| Shapefile (.shp) | 🔜 | 🔜 |
| GeoJSON | 🔜 | 🔜 |
| GeoParquet | 🔜 | 🔜 |

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
| `geogit-git` | Git object storage via [gitoxide](https://github.com/GitoxideLabs/gitoxide) |
| `geogit-wc` | Working copy adapters (GeoPackage, PostGIS) |
| `geogit` (CLI) | Command-line interface via [clap](https://github.com/clap-rs/clap) |

## License

MIT OR Apache-2.0
