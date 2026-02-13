use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::io::Write;

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
    /// Database manipulation
    Db {
        #[clap(subcommand)]
        cmd: DbCommand,
    },
    /// Install shell integrations
    Install {
        #[clap(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum DbCommand {
    /// Print the path of the database file
    Path,
    /// Imports a '.z' file from the user's home directory
    ///
    /// Timestamps are used to resolve conflicting rows
    Import,
}

#[derive(ValueEnum, Copy, Clone)]
enum Shell {
    Fish,
}

fn read_yn(prompt: &str) -> std::io::Result<bool> {
    loop {
        print!("{prompt} [y/n]: ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => break Ok(true),
            "n" | "no" => break Ok(false),
            _ => println!("Invalid input, please enter 'y' or 'n'."),
        }
    }
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
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("could not get base dirs"))?;
    match &args.cmd {
        Command::Add { path } => {
            // Convert to an absolute path
            let path = path
                .canonicalize_utf8()
                .with_context(|| format!("could not find '{path}'"))?;

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
        Command::Db {
            cmd: DbCommand::Import,
        } => {
            let user_dirs = directories::UserDirs::new()
                .ok_or_else(|| anyhow!("could not get user dirs"))?;
            let z_path = user_dirs.home_dir().join(".z");
            let z_text = std::fs::read_to_string(&z_path)
                .with_context(|| format!("could not read '{z_path:?}'"))?;
            let tx = conn.transaction()?;
            {
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
        Command::Db {
            cmd: DbCommand::Path,
        } => {
            let db_path = camino::Utf8PathBuf::try_from(db_file)?;
            println!("{db_path}");
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
                ORDER BY -10000 * rank * (3.75/((0.0001 * (time - ?) + 1) + 0.25))
                LIMIT 1
                ",
                rusqlite::params![pat, now],
                |r| r.get(0),
            )?;
            println!("{path}");
        }
        Command::Install { shell } => match shell {
            Shell::Fish => {
                let home = base_dirs.home_dir();
                copy_check(
                    home.join(".config").join(fish::conf::PATH),
                    fish::conf::SCRIPT,
                )?;
                copy_check(
                    home.join(".config").join(fish::func::PATH),
                    fish::func::SCRIPT,
                )?;
            }
        },
    }
    Ok(())
}

/// Writes `text` to `path`
fn copy_check(path: std::path::PathBuf, text: &str) -> anyhow::Result<()> {
    let path = camino::Utf8PathBuf::from_path_buf(path).unwrap();
    let prev = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    if let Some(prev) = prev {
        if prev == text {
            println!("file at '{path}' exists and matches");
        } else if read_yn(&format!("overwrite existing file at '{path}'?"))? {
            std::fs::write(path, text)?;
        } else {
            bail!("file at '{path}' exists and we won't overwrite it")
        }
    } else {
        println!("writing integration script to '{path}'");
        std::fs::write(path, text)?;
    }
    Ok(())
}

mod fish {
    macro_rules! include_path_str {
        ($mod_name:ident, $path:literal) => {
            pub mod $mod_name {
                pub const PATH: &str = $path;
                pub const SCRIPT: &str = include_str!(concat!("../", $path));
            }
        };
    }

    include_path_str!(conf, "fish/conf.d/z.fish");
    include_path_str!(func, "fish/functions/z.fish");
}
