use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand};

/// Directory tracker for easy jumping
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Do not print errors on failure (exit code is still set)
    #[clap(short, long)]
    quiet: bool,
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Adds a directory to the database and updates ranks
    Add {
        /// Path to add
        path: camino::Utf8PathBuf,
    },
    /// Finds a pattern, printing the match if present or returning an error
    Find {
        #[clap(allow_hyphen_values = true)]
        pattern: Vec<String>,
    },
    /// Imports a '.z' file from the user's home directory
    ///
    /// If the `clear` flag is not set, then timestamps are used to resolve
    /// conflicting rows
    Import {
        /// Clear the database before importing
        #[clap(long)]
        clear: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let r = inner(&args);
    if args.quiet {
        let exit_code = match r {
            Ok(()) => 0,
            Err(_) => 1,
        };
        std::process::exit(exit_code);
    } else {
        r
    }
}

fn inner(args: &Args) -> anyhow::Result<()> {
    let dirs = directories::ProjectDirs::from("", "", "zipzap")
        .ok_or_else(|| anyhow!("could not get project directory"))?;
    let data_dir = dirs.data_local_dir();
    match std::fs::create_dir(data_dir) {
        Ok(()) => (),
        Err(e) => match e.kind() {
            std::io::ErrorKind::AlreadyExists => (),
            _ => return Err(e.into()),
        },
    };
    let db_file = data_dir.join("db.sqlite");
    let mut conn = rusqlite::Connection::open_with_flags(
        &db_file,
        rusqlite::OpenFlags::SQLITE_OPEN_CREATE
            | rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .with_context(|| {
        format!("failed to open / create database at '{db_file:?}'")
    })?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS zipzap (
            path  TEXT PRIMARY KEY,
            rank  REAL NOT NULL,
            time  INTEGER NOT NULL
        )",
        (), // empty list of parameters.
    )?;

    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .try_into()
        .unwrap();
    match &args.cmd {
        Command::Add { path } => {
            // Convert to an absolute path
            let path = path
                .canonicalize_utf8()
                .with_context(|| format!("could not find '{path}'"))?;
            let base_dirs = directories::BaseDirs::new()
                .ok_or_else(|| anyhow!("could not get base dirs"))?;

            // Ignore home and root directories
            if path == base_dirs.home_dir() || path.components().count() == 1 {
                return Ok(());
            }
            conn.execute(
                "
                INSERT INTO zipzap (path, rank, time) VALUES (:path, 1, :now)
                ON CONFLICT(path)
                DO UPDATE SET rank = rank + 1, time = :now;
                ",
                rusqlite::named_params! {
                    ":path": path.as_str(),
                    ":now": now,
                },
            )?;
            // If the total rank is above a threshold, perform aging
            let total: f64 =
                conn.query_one("SELECT SUM(rank) from zipzap;", [], |r| {
                    r.get(0)
                })?;
            if total >= 9000.0 {
                conn.execute("UPDATE zipzap SET rank = rank * 0.99;", [])?;
            }
        }
        Command::Import { clear } => {
            let user_dirs = directories::UserDirs::new()
                .ok_or_else(|| anyhow!("could not get user dirs"))?;
            let z_path = user_dirs.home_dir().join(".z");
            let z_text = std::fs::read_to_string(&z_path)
                .with_context(|| format!("could not read '{z_path:?}'"))?;
            let tx = conn.transaction()?;
            {
                if *clear {
                    tx.execute("DELETE FROM zipzap;", [])?;
                }
                let mut stmt = tx.prepare(
                    "
                    INSERT INTO zipzap (path, rank, time) VALUES (?, ?, ?)
                    ON CONFLICT (path)
                    DO UPDATE SET
                        rank = excluded.rank,
                        time = excluded.time
                    WHERE excluded.time > zipzap.time;
                    ",
                )?;
                for line in z_text.lines() {
                    let mut iter = line.split('|');
                    let path = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing path in '{line}'"))?;
                    let rank: f64 = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing rank in '{line}'"))?
                        .parse()?;
                    let time: i64 = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing time in '{line}'"))?
                        .parse()?;
                    stmt.execute(rusqlite::params![path, rank, time])?;
                }
            }
            tx.commit()?;
        }
        Command::Find { pattern } => {
            // Build a wildcard pattern
            let mut pat = "%".to_string();
            for p in pattern {
                pat += "%";
                pat += p;
            }
            pat += "%";
            // Find the best match by "frecency"
            let path: String = conn.query_one(
                "
                SELECT path FROM zipzap WHERE path like ?
                ORDER BY 10000 * rank * (3.75/((0.0001 * (time - ?) + 1) + 0.25))
                LIMIT 1
                ",
                rusqlite::params![pat, now],
                |r| r.get(0),
            )?;
            println!("{path}");
        }
    }
    Ok(())
}
