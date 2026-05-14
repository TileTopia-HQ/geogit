use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use geogit_core::dataset::DatasetMeta;
use geogit_encoding::feature::StoredFeature;
use geogit_encoding::legend::Legend;
use geogit_encoding::path::PathStructure;
use geogit_encoding::schema::{Column, DataType, Schema};
use geogit_encoding::value::ColumnValue;
use geogit_git::Repository;
use geogit_git::tree::TreeBuilder;
use geogit_wc::gpkg::GeoPackageWorkingCopy;
use geogit_wc::traits::WorkingCopy;

#[derive(Parser)]
#[command(
    name = "geogit",
    version,
    about = "Distributed version control for geospatial data",
    long_about = "GeoGit provides Git-like version control for geospatial and tabular datasets.\n\
                  Store, branch, diff, merge, push, and pull geodata — edit directly in QGIS."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new GeoGit repository
    Init {
        /// Directory to initialize (default: current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Import a dataset during init (e.g. GPKG:path/to/file.gpkg)
        #[arg(long)]
        import: Option<String>,
    },

    /// Import a dataset into the repository
    Import {
        /// Source in FORMAT:PATH format (e.g. GPKG:data.gpkg)
        source: String,

        /// Dataset name (default: derived from filename)
        #[arg(long)]
        name: Option<String>,
    },

    /// Show repository status
    Status,

    /// Commit changes from the working copy
    Commit {
        /// Commit message
        #[arg(short, long)]
        message: String,
    },

    /// Show commit history
    Log {
        /// Show one line per commit
        #[arg(long)]
        oneline: bool,
    },

    /// List, create, or delete branches
    Branch {
        /// Branch name to create
        name: Option<String>,

        /// Delete the named branch
        #[arg(short, long)]
        delete: bool,
    },

    /// Switch to a different branch
    Switch {
        /// Branch to switch to
        branch: String,

        /// Create the branch if it doesn't exist
        #[arg(short, long)]
        create: bool,
    },

    /// Merge a branch into the current branch
    Merge {
        /// Branch to merge
        branch: String,
    },

    /// List and inspect datasets
    Data {
        #[command(subcommand)]
        subcommand: DataCommand,
    },

    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum DataCommand {
    /// List all datasets in the repository
    Ls,
    /// Show dataset information
    Info { dataset: String },
    /// Show dataset schema
    Schema { dataset: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { dir, import } => cmd_init(&dir, import.as_deref()),
        Command::Import { source, name } => cmd_import(&source, name.as_deref()),
        Command::Status => cmd_status(),
        Command::Commit { message } => cmd_commit(&message),
        Command::Log { oneline } => cmd_log(oneline),
        Command::Branch { name, delete } => cmd_branch(name.as_deref(), delete),
        Command::Switch { branch, create } => cmd_switch(&branch, create),
        Command::Merge { branch } => cmd_merge(&branch),
        Command::Data { subcommand } => match subcommand {
            DataCommand::Ls => cmd_data_ls(),
            DataCommand::Info { dataset } => cmd_data_info(&dataset),
            DataCommand::Schema { dataset } => cmd_data_schema(&dataset),
        },
        Command::Version => {
            println!("geogit {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("not a geogit repository (or any parent up to mount point)");
        }
    }
}

fn cmd_init(dir: &Path, import: Option<&str>) -> Result<()> {
    let dir = if dir == Path::new(".") {
        std::env::current_dir()?
    } else {
        std::fs::create_dir_all(dir)?;
        dir.canonicalize()?
    };

    let _repo = Repository::init(&dir)?;
    println!("Initialized empty GeoGit repository in {}", dir.display());

    if let Some(source) = import {
        std::env::set_current_dir(&dir)?;
        cmd_import(source, None)?;
    }

    Ok(())
}

fn cmd_import(source: &str, name: Option<&str>) -> Result<()> {
    let (format, path) = source
        .split_once(':')
        .context("source must be in FORMAT:PATH format (e.g. GPKG:data.gpkg)")?;

    match format.to_uppercase().as_str() {
        "GPKG" => import_gpkg(Path::new(path), name),
        _ => bail!("unsupported format: {format}. Supported: GPKG"),
    }
}

fn import_gpkg(gpkg_path: &Path, dataset_name: Option<&str>) -> Result<()> {
    use rusqlite::Connection;

    let gpkg_path = if gpkg_path.is_relative() {
        std::env::current_dir()?.join(gpkg_path)
    } else {
        gpkg_path.to_path_buf()
    };

    let conn = Connection::open(&gpkg_path)
        .with_context(|| format!("failed to open GeoPackage: {}", gpkg_path.display()))?;

    // Find feature tables
    let mut stmt = conn.prepare(
        "SELECT table_name, identifier FROM gpkg_contents WHERE data_type = 'features'",
    )?;
    let tables: Vec<(String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if tables.is_empty() {
        bail!("no feature tables found in GeoPackage");
    }

    let repo_root = find_repo_root()?;
    let repo = Repository::open(&repo_root)?;

    let bar = indicatif::ProgressBar::new(0);
    bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=>-"),
    );

    for (table_name, identifier) in &tables {
        let ds_name = dataset_name.unwrap_or(table_name);
        bar.set_message(format!("Importing {ds_name}"));

        // Read schema
        let schema = read_gpkg_schema(&conn, table_name)?;
        let meta = DatasetMeta {
            title: if identifier.is_empty() {
                table_name.clone()
            } else {
                identifier.clone()
            },
            description: String::new(),
            schema: schema.clone(),
            path_structure: PathStructure::default(),
        };

        // Read features
        let col_names: Vec<String> = schema.0.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let select_sql = format!("SELECT {} FROM \"{}\"", col_names.join(", "), table_name);

        let pk_indices: Vec<usize> = schema
            .0
            .iter()
            .enumerate()
            .filter(|(_, c)| c.primary_key_index.is_some())
            .map(|(i, _)| i)
            .collect();

        let mut feat_stmt = conn.prepare(&select_sql)?;
        let mut features = Vec::new();
        let mut wc_features = Vec::new();
        let legend = Legend::new(schema.column_ids());
        let legend_hash = legend.hash();

        let mut rows = feat_stmt.query([])?;
        while let Some(row) = rows.next()? {
            let mut values = HashMap::new();
            let mut pk = Vec::new();

            for (i, col) in schema.0.iter().enumerate() {
                let val = read_sqlite_value(row, i)?;
                if pk_indices.contains(&i) {
                    pk.push(val.clone());
                }
                values.insert(col.name.clone(), val);
            }

            // Stored feature: only non-PK values in schema order
            let stored_vals: Vec<ColumnValue> = schema
                .0
                .iter()
                .filter(|c| c.primary_key_index.is_none())
                .map(|c| values.get(&c.name).cloned().unwrap_or(ColumnValue::Null))
                .collect();

            let stored = StoredFeature {
                legend_hash: legend_hash.clone(),
                values: stored_vals,
            };

            features.push((pk.clone(), stored));
            wc_features.push((pk, values));
        }
        drop(rows);

        bar.set_length(features.len() as u64);

        // Write dataset tree structure
        let builder = TreeBuilder::new(&repo);
        builder.import_dataset(ds_name, &meta, &features)?;

        // Create working copy GeoPackage
        let wc_path = repo_root.join(format!(
            "{}.gpkg",
            repo_root
                .file_name()
                .unwrap()
                .to_string_lossy()
        ));
        let mut wc = GeoPackageWorkingCopy::open(&wc_path)?;
        wc.checkout(ds_name, &meta, &wc_features)?;

        bar.set_position(features.len() as u64);
        bar.finish_with_message(format!(
            "Imported {ds_name} ({} features)",
            features.len()
        ));
    }

    println!(
        "\nUse `geogit commit -m \"Initial import\"` to create the first commit."
    );

    Ok(())
}

#[allow(clippy::collapsible_if)]
fn read_gpkg_schema(conn: &rusqlite::Connection, table_name: &str) -> Result<Schema> {
    // Check for geometry columns
    let geom_cols: HashMap<String, (String, i64)> = {
        let mut map = HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT column_name, geometry_type_name, COALESCE(srs_id, 4326)
             FROM gpkg_geometry_columns WHERE table_name = ?1",
        ) {
            if let Ok(rows) = stmt.query_map([table_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, String>(1)?, row.get::<_, i64>(2)?),
                ))
            }) {
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
            }
        }
        map
    };

    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let mut columns = Vec::new();

    let rows = stmt.query_map([], |row| {
        let name: String = row.get(1)?;
        let type_name: String = row.get(2)?;
        let pk: i32 = row.get(5)?;
        Ok((name, type_name, pk))
    })?;

    for row in rows {
        let (name, type_name, pk) = row?;

        let (data_type, geom_type, geom_crs, size) =
            if let Some((gt, srs_id)) = geom_cols.get(&name) {
                (
                    DataType::Geometry,
                    Some(gt.clone()),
                    Some(format!("EPSG:{srs_id}")),
                    None,
                )
            } else {
                let upper = type_name.to_uppercase();
                let dt = if upper.contains("INT") {
                    DataType::Integer
                } else if upper.contains("REAL")
                    || upper.contains("FLOAT")
                    || upper.contains("DOUBLE")
                {
                    DataType::Float
                } else if upper.contains("BLOB") {
                    DataType::Blob
                } else if upper.contains("BOOL") {
                    DataType::Boolean
                } else {
                    DataType::Text
                };

                let size = match dt {
                    DataType::Integer => Some(64u32),
                    DataType::Float if upper.contains("REAL") => Some(32),
                    DataType::Float => Some(64),
                    _ => None,
                };

                (dt, None, None, size)
            };

        columns.push(Column {
            id: uuid::Uuid::new_v4(),
            name,
            data_type,
            primary_key_index: if pk > 0 { Some((pk - 1) as u32) } else { None },
            geometry_type: geom_type,
            geometry_crs: geom_crs,
            size,
            length: None,
            precision: None,
            scale: None,
            timezone: None,
        });
    }

    Ok(Schema(columns))
}

fn read_sqlite_value(row: &rusqlite::Row, idx: usize) -> Result<ColumnValue> {
    use rusqlite::types::ValueRef;

    match row.get_ref(idx)? {
        ValueRef::Null => Ok(ColumnValue::Null),
        ValueRef::Integer(v) => Ok(ColumnValue::Integer(v)),
        ValueRef::Real(v) => Ok(ColumnValue::Float(v)),
        ValueRef::Text(v) => {
            let s = String::from_utf8_lossy(v).to_string();
            Ok(ColumnValue::Text(s))
        }
        ValueRef::Blob(v) => Ok(ColumnValue::Blob(v.to_vec())),
    }
}

fn cmd_status() -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    let branch = repo.current_branch()?.unwrap_or_else(|| "HEAD detached".into());
    println!("On branch {branch}");

    let wc_name = format!("{}.gpkg", root.file_name().unwrap().to_string_lossy());
    let wc_path = root.join(&wc_name);

    if wc_path.exists() {
        let wc = GeoPackageWorkingCopy::open(&wc_path)?;
        let datasets = wc.list_datasets()?;

        let mut total_changes = 0;
        for ds in &datasets {
            let changes = wc.status(ds)?;
            if !changes.is_empty() {
                let inserts = changes.iter().filter(|d| d.is_insert()).count();
                let updates = changes.iter().filter(|d| d.is_update()).count();
                let deletes = changes.iter().filter(|d| d.is_delete()).count();
                println!("  {ds}:");
                if inserts > 0 {
                    println!("    {inserts} inserts");
                }
                if updates > 0 {
                    println!("    {updates} updates");
                }
                if deletes > 0 {
                    println!("    {deletes} deletes");
                }
                total_changes += changes.len();
            }
        }

        if total_changes == 0 {
            println!("Nothing to commit, working copy clean");
        }
    } else {
        println!("No working copy found");
    }

    Ok(())
}

fn cmd_commit(message: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let result = repo.commit(message)?;
    println!("{result}");
    Ok(())
}

fn cmd_log(oneline: bool) -> Result<()> {
    let root = find_repo_root()?;

    let mut args = vec!["log".to_string(), "--no-pager".to_string()];
    if oneline {
        args.push("--oneline".into());
    }

    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(&root)
        .output()
        .context("failed to run git log")?;

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn cmd_branch(name: Option<&str>, delete: bool) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    if let Some(name) = name {
        if delete {
            repo.delete_branch(name)?;
            println!("Deleted branch {name}");
        } else {
            repo.create_branch(name, "HEAD")?;
            println!("Created branch {name}");
        }
    } else {
        let branches = repo.branches()?;
        let current = repo.current_branch()?;
        for b in branches {
            let marker = if Some(&b.name) == current.as_ref() {
                "* "
            } else {
                "  "
            };
            println!("{marker}{}", b.name);
        }
    }
    Ok(())
}

fn cmd_switch(branch: &str, create: bool) -> Result<()> {
    let root = find_repo_root()?;

    let mut args = vec!["switch".to_string()];
    if create {
        args.push("-c".into());
    }
    args.push(branch.into());

    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(&root)
        .output()
        .context("failed to run git switch")?;

    if !output.status.success() {
        bail!(
            "switch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!("Switched to branch '{branch}'");
    Ok(())
}

fn cmd_merge(branch: &str) -> Result<()> {
    let root = find_repo_root()?;

    let output = std::process::Command::new("git")
        .args(["merge", branch])
        .current_dir(&root)
        .output()
        .context("failed to run git merge")?;

    if !output.status.success() {
        bail!(
            "merge failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn cmd_data_ls() -> Result<()> {
    let root = find_repo_root()?;

    fn find_datasets(dir: &Path, prefix: &str, results: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name == "target" {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    if name == ".table-dataset" {
                        results.push(prefix.trim_end_matches('/').to_string());
                    } else {
                        let new_prefix = if prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{prefix}/{name}")
                        };
                        find_datasets(&path, &new_prefix, results);
                    }
                }
            }
        }
    }

    let mut datasets = Vec::new();
    find_datasets(&root, "", &mut datasets);

    if datasets.is_empty() {
        println!("No datasets found. Use `geogit import` to add data.");
    } else {
        for ds in &datasets {
            println!("  {ds}");
        }
        println!("\n{} dataset(s)", datasets.len());
    }
    Ok(())
}

fn cmd_data_info(dataset: &str) -> Result<()> {
    let root = find_repo_root()?;
    let meta_dir = root.join(dataset).join(".table-dataset/meta");

    if !meta_dir.exists() {
        bail!("dataset '{dataset}' not found");
    }

    let title = std::fs::read_to_string(meta_dir.join("title")).unwrap_or_default();
    let desc = std::fs::read_to_string(meta_dir.join("description")).unwrap_or_default();
    let schema_json = std::fs::read_to_string(meta_dir.join("schema.json"))?;
    let schema: Schema = serde_json::from_str(&schema_json)?;

    // Count features
    let feature_dir = root.join(dataset).join(".table-dataset/feature");
    let count = count_files(&feature_dir);

    println!("Dataset: {dataset}");
    println!("Title: {title}");
    if !desc.is_empty() {
        println!("Description: {desc}");
    }
    println!("Columns: {}", schema.0.len());
    println!("Features: {count}");
    println!(
        "Primary key: {}",
        schema
            .primary_key_columns()
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    for col in &schema.0 {
        if col.data_type == DataType::Geometry {
            println!(
                "Geometry: {} ({}) [{}]",
                col.name,
                col.geometry_type.as_deref().unwrap_or("GEOMETRY"),
                col.geometry_crs.as_deref().unwrap_or("unknown CRS"),
            );
        }
    }
    Ok(())
}

fn cmd_data_schema(dataset: &str) -> Result<()> {
    let root = find_repo_root()?;
    let schema_path = root.join(dataset).join(".table-dataset/meta/schema.json");

    if !schema_path.exists() {
        bail!("dataset '{dataset}' not found");
    }

    let schema_json = std::fs::read_to_string(&schema_path)?;
    let schema: Schema = serde_json::from_str(&schema_json)?;

    println!("{:<4} {:<20} {:<15} Info", "#", "Name", "Type");
    println!("{}", "-".repeat(60));

    for (i, col) in schema.0.iter().enumerate() {
        let type_str = format!("{:?}", col.data_type).to_lowercase();
        let mut info = Vec::new();

        if col.primary_key_index.is_some() {
            info.push("PK".to_string());
        }
        if let Some(ref gt) = col.geometry_type {
            info.push(gt.clone());
        }
        if let Some(ref crs) = col.geometry_crs {
            info.push(crs.clone());
        }
        if let Some(size) = col.size {
            info.push(format!("{size}-bit"));
        }
        if let Some(len) = col.length {
            info.push(format!("max {len} chars"));
        }

        println!(
            "{:<4} {:<20} {:<15} {}",
            i + 1,
            col.name,
            type_str,
            info.join(", ")
        );
    }
    Ok(())
}

fn count_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path);
            } else {
                count += 1;
            }
        }
    }
    count
}
