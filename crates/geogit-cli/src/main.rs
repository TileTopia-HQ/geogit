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

    /// Clone a remote repository
    Clone {
        /// Remote URL (git SSH or HTTPS)
        url: String,
        /// Destination directory (default: derived from URL)
        dest: Option<PathBuf>,
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
        /// Number of commits to show
        #[arg(short = 'n', long)]
        max_count: Option<usize>,
    },

    /// Show details of a commit
    Show {
        /// Commit to show (default: HEAD)
        #[arg(default_value = "HEAD")]
        commit: String,
    },

    /// Show differences between versions
    Diff {
        /// Base commit or branch (default: HEAD)
        #[arg(default_value = "HEAD")]
        base: String,
        /// Target commit or branch (default: working copy)
        target: Option<String>,
        /// Show only a summary
        #[arg(long)]
        stat: bool,
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
        /// Abort an in-progress merge
        #[arg(long)]
        abort: bool,
    },

    /// Push commits to a remote
    Push {
        /// Remote name (default: origin)
        #[arg(default_value = "origin")]
        remote: String,
        /// Branch to push
        branch: Option<String>,
    },

    /// Pull commits from a remote
    Pull {
        /// Remote name (default: origin)
        #[arg(default_value = "origin")]
        remote: String,
        /// Branch to pull
        branch: Option<String>,
    },

    /// Manage remotes
    Remote {
        #[command(subcommand)]
        subcommand: RemoteCommand,
    },

    /// Reset the working copy to a clean state
    Reset {
        /// Target commit (default: HEAD)
        #[arg(default_value = "HEAD")]
        target: String,
    },

    /// Restore specific datasets from a commit
    Restore {
        /// Datasets to restore
        datasets: Vec<String>,
        /// Source commit (default: HEAD)
        #[arg(long, default_value = "HEAD")]
        source: String,
    },

    /// Checkout dataset(s) to a working copy GeoPackage
    Checkout {
        /// Datasets to checkout (default: all)
        datasets: Vec<String>,
    },

    /// Manage merge conflicts
    Conflicts {
        #[command(subcommand)]
        subcommand: Option<ConflictsCommand>,
    },

    /// Resolve merge conflicts
    Resolve {
        /// Paths to mark as resolved
        paths: Vec<String>,
        /// Accept theirs for all conflicts
        #[arg(long)]
        theirs: bool,
        /// Accept ours for all conflicts
        #[arg(long)]
        ours: bool,
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
enum RemoteCommand {
    /// Add a new remote
    Add { name: String, url: String },
    /// Remove a remote
    Remove { name: String },
    /// List all remotes
    #[command(name = "ls")]
    List,
}

#[derive(Subcommand)]
enum ConflictsCommand {
    /// List current conflicts
    #[command(name = "ls")]
    List,
    /// Abort the merge
    Abort,
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
        Command::Clone { url, dest } => cmd_clone(&url, dest.as_deref()),
        Command::Import { source, name } => cmd_import(&source, name.as_deref()),
        Command::Status => cmd_status(),
        Command::Commit { message } => cmd_commit(&message),
        Command::Log { oneline, max_count } => cmd_log(oneline, max_count),
        Command::Show { commit } => cmd_show(&commit),
        Command::Diff { base, target, stat } => cmd_diff(&base, target.as_deref(), stat),
        Command::Branch { name, delete } => cmd_branch(name.as_deref(), delete),
        Command::Switch { branch, create } => cmd_switch(&branch, create),
        Command::Merge { branch, abort } => cmd_merge(&branch, abort),
        Command::Push { remote, branch } => cmd_push(&remote, branch.as_deref()),
        Command::Pull { remote, branch } => cmd_pull(&remote, branch.as_deref()),
        Command::Remote { subcommand } => match subcommand {
            RemoteCommand::Add { name, url } => cmd_remote_add(&name, &url),
            RemoteCommand::Remove { name } => cmd_remote_remove(&name),
            RemoteCommand::List => cmd_remote_list(),
        },
        Command::Reset { target } => cmd_reset(&target),
        Command::Restore { datasets, source } => cmd_restore(&datasets, &source),
        Command::Checkout { datasets } => cmd_checkout(&datasets),
        Command::Conflicts { subcommand } => match subcommand {
            Some(ConflictsCommand::List) | None => cmd_conflicts_list(),
            Some(ConflictsCommand::Abort) => cmd_conflicts_abort(),
        },
        Command::Resolve {
            paths,
            theirs,
            ours,
        } => cmd_resolve(&paths, theirs, ours),
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

// --- Helpers ---

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

fn wc_path(root: &Path) -> PathBuf {
    root.join(format!(
        "{}.gpkg",
        root.file_name().unwrap().to_string_lossy()
    ))
}

fn find_datasets(dir: &Path, prefix: &str, results: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name.ends_with(".gpkg") {
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

// --- Command Implementations ---

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

fn cmd_clone(url: &str, dest: Option<&Path>) -> Result<()> {
    let dest = match dest {
        Some(d) => d.to_path_buf(),
        None => {
            let name = url
                .rsplit('/')
                .next()
                .unwrap_or("repo")
                .trim_end_matches(".git");
            PathBuf::from(name)
        }
    };

    println!("Cloning into '{}'...", dest.display());
    let _repo = Repository::clone_repo(url, &dest)?;
    println!("Done.");
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

    let mut stmt = conn
        .prepare("SELECT table_name, identifier FROM gpkg_contents WHERE data_type = 'features'")?;
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

        let builder = TreeBuilder::new(&repo);
        builder.import_dataset(ds_name, &meta, &features)?;

        let wc_gpkg_path = wc_path(&repo_root);
        let mut wc = GeoPackageWorkingCopy::open(&wc_gpkg_path)?;
        wc.checkout(ds_name, &meta, &wc_features)?;

        bar.set_position(features.len() as u64);
        bar.finish_with_message(format!("Imported {ds_name} ({} features)", features.len()));
    }

    println!("\nUse `geogit commit -m \"Initial import\"` to create the first commit.");
    Ok(())
}

#[allow(clippy::collapsible_if)]
fn read_gpkg_schema(conn: &rusqlite::Connection, table_name: &str) -> Result<Schema> {
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

        let (data_type, geom_type, geom_crs, size) = if let Some((gt, srs_id)) =
            geom_cols.get(&name)
        {
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
            } else if upper.contains("REAL") || upper.contains("FLOAT") || upper.contains("DOUBLE")
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

    let branch = repo
        .current_branch()?
        .unwrap_or_else(|| "HEAD detached".into());
    println!("On branch {branch}");

    if root.join(".git/MERGE_HEAD").exists() {
        println!("  (merge in progress — use 'geogit resolve' then 'geogit commit')");
    }

    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        let wc = GeoPackageWorkingCopy::open(&wc_gpkg)?;
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
        println!("No working copy. Use `geogit checkout` to create one.");
    }

    Ok(())
}

fn cmd_commit(message: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    // Sync working copy changes to tree before committing
    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        sync_wc_to_tree(&root, &wc_gpkg)?;
    }

    let result = repo.commit(message)?;
    println!("{result}");
    Ok(())
}

/// Sync working copy changes back to the .table-dataset tree.
fn sync_wc_to_tree(root: &Path, wc_gpkg: &Path) -> Result<()> {
    let wc = GeoPackageWorkingCopy::open(wc_gpkg)?;
    let datasets = wc.list_datasets()?;

    for ds in &datasets {
        let changes = wc.status(ds)?;
        if changes.is_empty() {
            continue;
        }

        let schema_path = root.join(ds).join(".table-dataset/meta/schema.json");
        if !schema_path.exists() {
            continue;
        }
        let schema_json = std::fs::read_to_string(&schema_path)?;
        let schema: Schema = serde_json::from_str(&schema_json)?;

        let legend = Legend::new(schema.column_ids());
        let legend_hash = legend.hash();
        let ps_path = root
            .join(ds)
            .join(".table-dataset/meta/path-structure.json");
        let ps: PathStructure = if ps_path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&ps_path)?)?
        } else {
            PathStructure::default()
        };

        let feature_dir = root.join(ds).join(".table-dataset/feature");

        for delta in &changes {
            match delta {
                geogit_core::diff::FeatureDelta::Insert { pk, new }
                | geogit_core::diff::FeatureDelta::Update { pk, new, .. } => {
                    let stored_vals: Vec<ColumnValue> = schema
                        .0
                        .iter()
                        .filter(|c| c.primary_key_index.is_none())
                        .map(|c| new.get(&c.name).cloned().unwrap_or(ColumnValue::Null))
                        .collect();

                    let stored = StoredFeature {
                        legend_hash: legend_hash.clone(),
                        values: stored_vals,
                    };

                    let rel_path = ps.feature_path(pk);
                    let full_path = feature_dir.join(&rel_path);
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&full_path, stored.to_msgpack())?;
                }
                geogit_core::diff::FeatureDelta::Delete { pk, .. } => {
                    let rel_path = ps.feature_path(pk);
                    let full_path = feature_dir.join(&rel_path);
                    let _ = std::fs::remove_file(&full_path);
                }
            }
        }

        wc.clear_tracking(ds)?;
    }

    Ok(())
}

fn cmd_log(oneline: bool, max_count: Option<usize>) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let entries = repo.log(max_count, oneline)?;

    if entries.is_empty() {
        println!("No commits yet.");
        return Ok(());
    }

    for entry in &entries {
        if oneline {
            println!("{} {}", entry.short_hash, entry.subject);
        } else {
            println!("\x1b[33mcommit {}\x1b[0m", entry.hash);
            println!("Author: {} <{}>", entry.author_name, entry.author_email);
            println!("Date:   {}", entry.date);
            println!();
            println!("    {}", entry.subject);
            if !entry.body.is_empty() {
                for line in entry.body.lines() {
                    println!("    {line}");
                }
            }
            println!();
        }
    }
    Ok(())
}

fn cmd_show(commit: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let output = repo.show_commit(commit)?;
    print!("{output}");
    Ok(())
}

fn cmd_diff(base: &str, target: Option<&str>, stat: bool) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    if let Some(target) = target {
        // Diff between two commits
        let entries = repo.diff_tree(base, target)?;
        if entries.is_empty() {
            println!("No differences.");
        } else {
            print_file_diff(&entries, stat);
        }
    } else {
        // Diff working copy vs HEAD (feature-level)
        let wc_gpkg = wc_path(&root);
        if wc_gpkg.exists() {
            let wc = GeoPackageWorkingCopy::open(&wc_gpkg)?;
            let datasets = wc.list_datasets()?;
            let mut any = false;
            for ds in &datasets {
                let changes = wc.status(ds)?;
                if changes.is_empty() {
                    continue;
                }
                any = true;
                println!("--- {ds} ---");
                if stat {
                    println!(
                        "  {} inserts, {} updates, {} deletes",
                        changes.iter().filter(|d| d.is_insert()).count(),
                        changes.iter().filter(|d| d.is_update()).count(),
                        changes.iter().filter(|d| d.is_delete()).count(),
                    );
                } else {
                    for delta in &changes {
                        print_delta(delta);
                    }
                }
            }
            if !any {
                println!("Nothing to diff, working copy clean.");
            }
        } else {
            // Fall back to git-level diff
            let entries = repo.diff_working()?;
            if entries.is_empty() {
                println!("No differences.");
            } else {
                print_file_diff(&entries, stat);
            }
        }
    }
    Ok(())
}

fn print_file_diff(entries: &[geogit_git::DiffEntry], stat: bool) {
    if stat {
        let mut ds_changes: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for entry in entries {
            let ds = entry
                .path
                .split("/.table-dataset/")
                .next()
                .unwrap_or(&entry.path);
            let counts = ds_changes.entry(ds.to_string()).or_default();
            match entry.status {
                geogit_git::DiffStatus::Added => counts.0 += 1,
                geogit_git::DiffStatus::Modified => counts.1 += 1,
                geogit_git::DiffStatus::Deleted => counts.2 += 1,
            }
        }
        for (ds, (a, m, d)) in &ds_changes {
            println!("{ds}: {a} added, {m} modified, {d} deleted");
        }
    } else {
        for entry in entries {
            let ch = match entry.status {
                geogit_git::DiffStatus::Added => "+",
                geogit_git::DiffStatus::Deleted => "-",
                geogit_git::DiffStatus::Modified => "~",
            };
            println!("{ch} {}", entry.path);
        }
    }
}

fn print_delta(delta: &geogit_core::diff::FeatureDelta) {
    let pk_str = format_pk(delta.pk());
    match delta {
        geogit_core::diff::FeatureDelta::Insert { new, .. } => {
            println!("  \x1b[32m+ {pk_str}\x1b[0m");
            for (k, v) in new {
                println!("      {k}: {}", format_value(v));
            }
        }
        geogit_core::diff::FeatureDelta::Delete { .. } => {
            println!("  \x1b[31m- {pk_str}\x1b[0m");
        }
        geogit_core::diff::FeatureDelta::Update {
            new,
            old,
            changed_columns,
            ..
        } => {
            println!("  \x1b[33m~ {pk_str}\x1b[0m");
            for col in changed_columns {
                let old_v = old.get(col).map(format_value).unwrap_or_default();
                let new_v = new.get(col).map(format_value).unwrap_or_default();
                println!("      {col}: {old_v} → {new_v}");
            }
        }
    }
}

fn format_pk(pk: &[ColumnValue]) -> String {
    pk.iter().map(format_value).collect::<Vec<_>>().join(",")
}

fn format_value(v: &ColumnValue) -> String {
    match v {
        ColumnValue::Null => "NULL".to_string(),
        ColumnValue::Bool(b) => b.to_string(),
        ColumnValue::Integer(i) => i.to_string(),
        ColumnValue::Float(f) => f.to_string(),
        ColumnValue::Text(s) => {
            if s.len() > 50 {
                format!("\"{}...\"", &s[..47])
            } else {
                format!("\"{s}\"")
            }
        }
        ColumnValue::Blob(b) => format!("<{} bytes>", b.len()),
    }
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
    let repo = Repository::open(&root)?;
    repo.switch_branch(branch, create)?;
    println!("Switched to branch '{branch}'");

    // Refresh working copy
    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        refresh_working_copy(&root, &wc_gpkg)?;
    }
    Ok(())
}

fn cmd_merge(branch: &str, abort: bool) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    if abort {
        repo.merge_abort()?;
        println!("Merge aborted.");
        return Ok(());
    }

    let result = repo.merge(branch)?;
    if result.success {
        println!("{}", result.message.trim());
        let wc_gpkg = wc_path(&root);
        if wc_gpkg.exists() {
            refresh_working_copy(&root, &wc_gpkg)?;
        }
    } else {
        println!("{}", result.message.trim());
        if !result.conflicts.is_empty() {
            println!("\nConflicts:");
            for c in &result.conflicts {
                println!("  {c}");
            }
            println!("\nResolve conflicts then run `geogit resolve <paths>` and `geogit commit`.");
        }
    }
    Ok(())
}

fn cmd_push(remote: &str, branch: Option<&str>) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let output = repo.push(remote, branch)?;
    if output.is_empty() {
        println!("Everything up-to-date");
    } else {
        println!("{output}");
    }
    Ok(())
}

fn cmd_pull(remote: &str, branch: Option<&str>) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let output = repo.pull(remote, branch)?;
    println!("{output}");

    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        refresh_working_copy(&root, &wc_gpkg)?;
    }
    Ok(())
}

fn cmd_remote_add(name: &str, url: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    repo.remote_add(name, url)?;
    println!("Added remote '{name}' → {url}");
    Ok(())
}

fn cmd_remote_remove(name: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    repo.remote_remove(name)?;
    println!("Removed remote '{name}'");
    Ok(())
}

fn cmd_remote_list() -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let remotes = repo.remotes()?;
    if remotes.is_empty() {
        println!("No remotes configured.");
    } else {
        for r in &remotes {
            println!("  {} → {}", r.name, r.url);
        }
    }
    Ok(())
}

fn cmd_reset(target: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    repo.reset_hard(target)?;
    println!("Reset to {target}");

    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        refresh_working_copy(&root, &wc_gpkg)?;
    }
    Ok(())
}

fn cmd_restore(datasets: &[String], source: &str) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    for ds in datasets {
        let ds_path = format!("{ds}/.table-dataset");
        repo.checkout_path(source, &ds_path)?;
        println!("Restored {ds} from {source}");
    }

    let wc_gpkg = wc_path(&root);
    if wc_gpkg.exists() {
        refresh_working_copy(&root, &wc_gpkg)?;
    }
    Ok(())
}

fn cmd_checkout(datasets: &[String]) -> Result<()> {
    let root = find_repo_root()?;
    let wc_gpkg = wc_path(&root);

    let ds_list = if datasets.is_empty() {
        let mut found = Vec::new();
        find_datasets(&root, "", &mut found);
        found
    } else {
        datasets.to_vec()
    };

    if ds_list.is_empty() {
        bail!("No datasets found. Import data first with `geogit import`.");
    }

    let mut wc = GeoPackageWorkingCopy::open(&wc_gpkg)?;

    for ds in &ds_list {
        let meta_dir = root.join(ds).join(".table-dataset/meta");
        if !meta_dir.exists() {
            println!("Warning: dataset '{ds}' not found, skipping.");
            continue;
        }

        let schema_json = std::fs::read_to_string(meta_dir.join("schema.json"))?;
        let schema: Schema = serde_json::from_str(&schema_json)?;
        let title = std::fs::read_to_string(meta_dir.join("title")).unwrap_or_default();
        let description = std::fs::read_to_string(meta_dir.join("description")).unwrap_or_default();
        let ps: PathStructure = if meta_dir.join("path-structure.json").exists() {
            serde_json::from_str(&std::fs::read_to_string(
                meta_dir.join("path-structure.json"),
            )?)?
        } else {
            PathStructure::default()
        };

        let meta = DatasetMeta {
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            schema: schema.clone(),
            path_structure: ps,
        };

        let feature_dir = root.join(ds).join(".table-dataset/feature");
        let features = load_features_from_tree(&feature_dir, &meta)?;
        wc.checkout(ds, &meta, &features)?;
        println!("Checked out {ds} ({} features)", features.len());
    }

    println!("\nWorking copy: {}", wc_gpkg.display());
    Ok(())
}

/// A feature row: primary key values + column name-value map.
type FeatureRow = (Vec<ColumnValue>, HashMap<String, ColumnValue>);

fn load_features_from_tree(feature_dir: &Path, meta: &DatasetMeta) -> Result<Vec<FeatureRow>> {
    let mut features = Vec::new();

    let legend_dir = feature_dir.parent().unwrap().join("meta/legend");
    let legends = load_legends(&legend_dir)?;

    walk_feature_files(feature_dir, &legends, &meta.schema, &mut features)?;
    Ok(features)
}

fn load_legends(legend_dir: &Path) -> Result<HashMap<String, Legend>> {
    let mut legends = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(legend_dir) {
        for entry in entries.flatten() {
            let hash = entry.file_name().to_string_lossy().to_string();
            let data = std::fs::read(entry.path())?;
            let legend = Legend::from_msgpack(&data)?;
            legends.insert(hash, legend);
        }
    }
    Ok(legends)
}

fn walk_feature_files(
    dir: &Path,
    legends: &HashMap<String, Legend>,
    schema: &Schema,
    out: &mut Vec<FeatureRow>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_feature_files(&path, legends, schema, out)?;
        } else {
            let data = std::fs::read(&path)?;
            let feature = StoredFeature::from_msgpack(&data)?;

            if let Some(legend) = legends.get(&feature.legend_hash) {
                let col_names = legend.column_names(schema);
                let non_pk_cols: Vec<&Column> = schema
                    .0
                    .iter()
                    .filter(|c| c.primary_key_index.is_none())
                    .collect();

                let mut values = HashMap::new();
                for (i, val) in feature.values.iter().enumerate() {
                    if i < col_names.len() {
                        values.insert(col_names[i].clone(), val.clone());
                    } else if i < non_pk_cols.len() {
                        values.insert(non_pk_cols[i].name.clone(), val.clone());
                    }
                }

                let pk: Vec<ColumnValue> = schema
                    .0
                    .iter()
                    .filter(|c| c.primary_key_index.is_some())
                    .map(|c| values.get(&c.name).cloned().unwrap_or(ColumnValue::Null))
                    .collect();

                out.push((pk, values));
            }
        }
    }
    Ok(())
}

fn cmd_conflicts_list() -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    let conflicts = repo.list_conflicts()?;

    if conflicts.is_empty() {
        println!("No conflicts.");
    } else {
        println!("{} conflicted file(s):", conflicts.len());
        for c in &conflicts {
            println!("  {c}");
        }
    }
    Ok(())
}

fn cmd_conflicts_abort() -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;
    repo.merge_abort()?;
    println!("Merge aborted.");
    Ok(())
}

fn cmd_resolve(paths: &[String], theirs: bool, ours: bool) -> Result<()> {
    let root = find_repo_root()?;
    let repo = Repository::open(&root)?;

    if theirs || ours {
        let conflicts = if paths.is_empty() {
            repo.list_conflicts()?
        } else {
            paths.to_vec()
        };

        let strategy = if theirs { "--theirs" } else { "--ours" };
        for path in &conflicts {
            let output = std::process::Command::new("git")
                .args(["checkout", strategy, "--", path])
                .current_dir(&root)
                .output()
                .context("failed to checkout conflict")?;
            if !output.status.success() {
                eprintln!(
                    "Warning: could not resolve {path}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        let refs: Vec<&str> = conflicts.iter().map(|s| s.as_str()).collect();
        repo.resolve_conflicts(&refs)?;
        println!("Resolved {} conflict(s) with {strategy}", conflicts.len());
    } else if paths.is_empty() {
        bail!("specify paths to resolve, or use --theirs/--ours for all");
    } else {
        let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        repo.resolve_conflicts(&refs)?;
        println!("Marked {} path(s) as resolved", paths.len());
    }
    Ok(())
}

fn cmd_data_ls() -> Result<()> {
    let root = find_repo_root()?;

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

    let feature_dir = root.join(dataset).join(".table-dataset/feature");
    let count = count_files(&feature_dir);

    println!("Dataset: {dataset}");
    println!("Title: {}", title.trim());
    if !desc.trim().is_empty() {
        println!("Description: {}", desc.trim());
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

/// Refresh the working copy GeoPackage to match the current tree.
fn refresh_working_copy(root: &Path, wc_gpkg: &Path) -> Result<()> {
    let _ = std::fs::remove_file(wc_gpkg);

    let mut datasets = Vec::new();
    find_datasets(root, "", &mut datasets);

    if datasets.is_empty() {
        return Ok(());
    }

    let mut wc = GeoPackageWorkingCopy::open(wc_gpkg)?;
    for ds in &datasets {
        let meta_dir = root.join(ds).join(".table-dataset/meta");
        if !meta_dir.exists() {
            continue;
        }

        let schema_json = std::fs::read_to_string(meta_dir.join("schema.json"))?;
        let schema: Schema = serde_json::from_str(&schema_json)?;
        let title = std::fs::read_to_string(meta_dir.join("title")).unwrap_or_default();
        let description = std::fs::read_to_string(meta_dir.join("description")).unwrap_or_default();
        let ps: PathStructure = if meta_dir.join("path-structure.json").exists() {
            serde_json::from_str(&std::fs::read_to_string(
                meta_dir.join("path-structure.json"),
            )?)?
        } else {
            PathStructure::default()
        };

        let meta = DatasetMeta {
            title: title.trim().to_string(),
            description: description.trim().to_string(),
            schema: schema.clone(),
            path_structure: ps,
        };

        let feature_dir = root.join(ds).join(".table-dataset/feature");
        let features = load_features_from_tree(&feature_dir, &meta)?;
        wc.checkout(ds, &meta, &features)?;
    }

    Ok(())
}
