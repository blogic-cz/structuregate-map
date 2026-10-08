//! fbt command line.

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use fbt::db;
use fbt::diff;
use fbt::hash;
use fbt::pipeline::{Engine, Refresh};
use fbt::scan::ScanOptions;
use fbt::targets;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "fbt", version, about = "Fast source tree map and change detection")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan the tree and store it as a new snapshot.
    Snapshot(ScanArgs),
    /// Report what changed since the last snapshot, without storing anything.
    Status(ScanArgs),
    /// Report what changed and store the result as the new snapshot.
    Update(ScanArgs),
    /// Report the targets that must rebuild, and store the new snapshot.
    Dirty(DirtyArgs),
    /// Compare two stored snapshots.
    Diff(DiffArgs),
    /// List stored snapshots.
    Snapshots(DbArgs),
    /// Delete old snapshots, keeping the newest few per root.
    Prune(PruneArgs),
    /// Manage the build target graph.
    #[command(subcommand)]
    Targets(TargetCmd),
    /// Print the raw NTFS journal records since the last snapshot. Diagnostic.
    #[cfg(windows)]
    Journal(JournalArgs),
}

#[cfg(windows)]
#[derive(Args, Clone)]
struct JournalArgs {
    #[arg(default_value = ".")]
    root: PathBuf,
    #[command(flatten)]
    db: DbArgs,
    /// Only print records whose name contains this text.
    #[arg(long)]
    filter: Option<String>,
    #[arg(long, default_value_t = 40)]
    limit: usize,
}

#[derive(Args, Clone)]
struct DbArgs {
    /// Database file. Defaults to <root>/.fbt/map.db.
    #[arg(long, global = true)]
    db: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct ScanArgs {
    /// Directory to map.
    #[arg(default_value = ".")]
    root: PathBuf,
    #[command(flatten)]
    db: DbArgs,
    /// Do not read .gitignore.
    #[arg(long)]
    no_gitignore: bool,
    /// Do not skip the built-in list of output directories.
    #[arg(long)]
    no_default_skip: bool,
    /// Extra directory name to skip. Repeatable.
    #[arg(long = "skip", value_name = "NAME")]
    skip: Vec<String>,
    /// Read every file, even when size and mtime are unchanged.
    #[arg(long)]
    rehash_all: bool,
    /// Use the NTFS journal instead of walking the tree. Much cheaper on a very
    /// large tree, but it can lag behind a file another process holds open, so a
    /// full walk is still forced every MAX_USN_CHAIN runs. Needs administrator
    /// rights.
    #[arg(long)]
    usn: bool,
    /// Include directories in the change list.
    #[arg(long)]
    dirs: bool,
    /// Print JSON instead of text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Clone)]
struct DirtyArgs {
    #[command(flatten)]
    scan: ScanArgs,
    /// Do not store a new snapshot, so the same change is reported again.
    #[arg(long)]
    dry_run: bool,
    /// List changed paths that no target claims.
    #[arg(long)]
    show_unclaimed: bool,
}

#[derive(Args, Clone)]
struct DiffArgs {
    old: i64,
    new: i64,
    #[command(flatten)]
    db: DbArgs,
    #[arg(long)]
    dirs: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Clone)]
struct PruneArgs {
    #[command(flatten)]
    db: DbArgs,
    /// Snapshots to keep per root.
    #[arg(long, default_value_t = 5)]
    keep: i64,
}

#[derive(Subcommand)]
enum TargetCmd {
    /// Replace the target graph with a JSON file.
    Import {
        file: PathBuf,
        #[command(flatten)]
        db: DbArgs,
    },
    /// Print the stored target graph.
    List {
        #[command(flatten)]
        db: DbArgs,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Snapshot(a) => cmd_refresh(&a, true, true),
        Command::Status(a) => cmd_refresh(&a, false, false),
        Command::Update(a) => cmd_refresh(&a, true, false),
        Command::Dirty(a) => cmd_dirty(&a),
        Command::Diff(a) => cmd_diff(&a),
        Command::Snapshots(a) => cmd_snapshots(&a),
        Command::Prune(a) => cmd_prune(&a),
        Command::Targets(c) => cmd_targets(c),
        #[cfg(windows)]
        Command::Journal(a) => cmd_journal(&a),
    }
}

#[cfg(windows)]
fn cmd_journal(a: &JournalArgs) -> Result<()> {
    use fbt::usn;

    let conn = db::open(&db_path(&a.db, &a.root))?;
    let root = std::fs::canonicalize(&a.root)?;
    let root_label = root
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");

    let volume = usn::volume_of(&root)?;
    let vol = usn::open_volume(&volume)?;
    let info = usn::query_journal(&vol)?;

    let stored = db::latest_snapshot(&conn, &root_label)?.and_then(|s| s.usn);
    println!(
        "volume {volume}  journal {}  first {}  next {}  stored {}",
        info.journal_id,
        info.first_usn,
        info.next_usn,
        stored.map(|u| u.to_string()).unwrap_or_else(|| "-".into())
    );

    let start = stored.unwrap_or(info.first_usn);
    let (changes, next) = usn::read_changes(&vol, &info, start)?;
    println!("{} record(s) between {start} and {next}", changes.len());

    let mut shown = 0usize;
    for c in &changes {
        if let Some(f) = &a.filter
            && !c.name.to_lowercase().contains(&f.to_lowercase()) {
                continue;
            }
        if shown >= a.limit {
            break;
        }
        shown += 1;
        println!(
            "  file {:>20}  parent {:>20}  {}  [{}]",
            c.file_ref,
            c.parent_ref,
            c.name,
            usn::reason_names(c.reason).join(",")
        );
    }
    Ok(())
}

fn db_path(db: &DbArgs, root: &Path) -> PathBuf {
    db.db
        .clone()
        .unwrap_or_else(|| root.join(".fbt").join("map.db"))
}

fn engine(a: &ScanArgs) -> Result<Engine> {
    if !a.root.is_dir() {
        bail!("{} is not a directory", a.root.display());
    }
    let opts = ScanOptions {
        use_gitignore: !a.no_gitignore,
        extra_skip: a.skip.clone(),
        no_default_skip: a.no_default_skip,
        skip_paths: Vec::new(),
        rehash_all: a.rehash_all,
    };
    Engine::open(&db_path(&a.db, &a.root), &a.root, opts)
}

/// `store` writes a new snapshot; `quiet_changes` prints only the summary, which
/// is what the first full mapping wants.
fn cmd_refresh(a: &ScanArgs, store: bool, quiet_changes: bool) -> Result<()> {
    let mut e = engine(a)?;
    let r = e.refresh(a.usn, store, a.dirs)?;

    if a.json {
        print_json(&r)?;
        return Ok(());
    }
    if !quiet_changes {
        for c in &r.changes {
            match &c.from {
                Some(from) => println!("{} {}  <- {}", c.kind.tag(), c.path, from),
                None => println!("{} {}", c.kind.tag(), c.path),
            }
        }
    }
    print_summary(&r);
    Ok(())
}

fn cmd_dirty(a: &DirtyArgs) -> Result<()> {
    let mut e = engine(&a.scan)?;
    let r = e.refresh(a.scan.usn, !a.dry_run, a.scan.dirs)?;
    let changed = r.changed_paths();
    let dirty = targets::dirty(&e.conn, &changed)?;

    if a.scan.json {
        let payload = serde_json::json!({
            "method": r.method.label(),
            "root_hash": r.root_hash.map(|h| hash::hex(&h)),
            "changed": changed,
            "dirty_targets": dirty,
            "unclaimed": if a.show_unclaimed { targets::unclaimed(&e.conn, &changed)? } else { Vec::new() },
            "elapsed_ms": r.elapsed_ms,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    for name in &dirty {
        println!("{name}");
    }
    if a.show_unclaimed {
        for p in targets::unclaimed(&e.conn, &changed)? {
            eprintln!("unclaimed: {p}");
        }
    }
    eprintln!(
        "{} changed path(s) -> {} dirty target(s) via {} in {} ms",
        changed.len(),
        dirty.len(),
        r.method.label(),
        r.elapsed_ms
    );
    Ok(())
}

fn cmd_diff(a: &DiffArgs) -> Result<()> {
    let path = a
        .db
        .db
        .clone()
        .unwrap_or_else(|| PathBuf::from(".fbt/map.db"));
    let conn = db::open(&path)?;
    for id in [a.old, a.new] {
        if db::snapshot_by_id(&conn, id)?.is_none() {
            bail!("snapshot {id} does not exist");
        }
    }
    let changes = diff::diff_snapshots(&conn, a.old, a.new, a.dirs)?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&changes)?);
        return Ok(());
    }
    for c in &changes {
        match &c.from {
            Some(from) => println!("{} {}  <- {}", c.kind.tag(), c.path, from),
            None => println!("{} {}", c.kind.tag(), c.path),
        }
    }
    eprintln!("{} change(s)", changes.len());
    Ok(())
}

fn cmd_snapshots(a: &DbArgs) -> Result<()> {
    let path = a.db.clone().unwrap_or_else(|| PathBuf::from(".fbt/map.db"));
    let conn = db::open(&path)?;
    println!("{:>5}  {:>9}  {:<14}  {:<20}  root", "id", "entries", "root hash", "usn");
    for s in db::list_snapshots(&conn, 50)? {
        println!(
            "{:>5}  {:>9}  {:<14}  {:<20}  {}",
            s.id,
            s.entry_count,
            s.root_hash.map(|h| hash::hex_short(&h)).unwrap_or_else(|| "-".into()),
            s.usn.map(|u| u.to_string()).unwrap_or_else(|| "-".into()),
            s.root
        );
    }
    Ok(())
}

fn cmd_prune(a: &PruneArgs) -> Result<()> {
    let path = a.db.db.clone().unwrap_or_else(|| PathBuf::from(".fbt/map.db"));
    let conn = db::open(&path)?;
    let removed = db::prune(&conn, a.keep)?;
    conn.execute_batch("VACUUM")?;
    println!("removed {removed} snapshot(s)");
    Ok(())
}

fn cmd_targets(c: TargetCmd) -> Result<()> {
    match c {
        TargetCmd::Import { file, db } => {
            let path = db.db.unwrap_or_else(|| PathBuf::from(".fbt/map.db"));
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            let mut conn = db::open(&path)?;
            let n = targets::import(&mut conn, &file)?;
            println!("imported {n} target(s)");
        }
        TargetCmd::List { db } => {
            let path = db.db.unwrap_or_else(|| PathBuf::from(".fbt/map.db"));
            let conn = db::open(&path)?;
            for t in targets::list(&conn)? {
                println!("{}", t.name);
                for i in &t.inputs {
                    println!("  input {i}");
                }
                for d in &t.deps {
                    println!("  dep   {d}");
                }
            }
        }
    }
    Ok(())
}

fn print_summary(r: &Refresh) {
    let root_hash = r
        .root_hash
        .map(|h| hash::hex_short(&h))
        .unwrap_or_else(|| "-".into());
    let same = match (r.previous_root_hash, r.root_hash) {
        (Some(a), Some(b)) if a == b => " (unchanged)",
        _ => "",
    };
    eprintln!(
        "{} entries, {} change(s), {} read from disk, root {}{}, {} in {} ms",
        r.entry_count,
        r.changes.len(),
        r.read_from_disk,
        root_hash,
        same,
        r.method.label(),
        r.elapsed_ms
    );
    if let Some(id) = r.snapshot_id {
        eprintln!("stored as snapshot {id}");
    }
    if let Some(note) = &r.usn_note {
        eprintln!("journal fast path skipped: {note}");
    }
}

fn print_json(r: &Refresh) -> Result<()> {
    let payload = serde_json::json!({
        "method": r.method.label(),
        "entries": r.entry_count,
        "read_from_disk": r.read_from_disk,
        "root_hash": r.root_hash.map(|h| hash::hex(&h)),
        "previous_root_hash": r.previous_root_hash.map(|h| hash::hex(&h)),
        "elapsed_ms": r.elapsed_ms,
        "snapshot_id": r.snapshot_id,
        "usn_note": r.usn_note,
        "changes": r.changes,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}
