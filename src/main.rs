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
        shell: Option<Shell>,
    },
    /// Prints code to be loaded into a shell's environment
    Source {
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
    Bash,
    Zsh,
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
                    ":path": path.as_str().to_lowercase(),
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
            let mut n = 0;
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
                        .ok_or_else(|| anyhow!("missing path in '{line}'"))?
                        .to_lowercase();
                    let rank: f64 = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing rank in '{line}'"))?
                        .parse()?;
                    let time: i64 = iter
                        .next()
                        .ok_or_else(|| anyhow!("missing time in '{line}'"))?
                        .parse()?;
                    stmt.execute(rusqlite::params![path, rank, time])?;
                    n += 1;
                }
            }
            tx.commit()?;
            println!("imported {n} rows");
        }
        Command::Db {
            cmd: DbCommand::Path,
        } => {
            let db_path = camino::Utf8PathBuf::try_from(db_file)?;
            println!("{db_path}");
        }
        Command::Find { pattern } => {
            if pattern.is_empty() {
                return Ok(());
            }
            // Build a wildcard pattern
            let mut pat = String::new();
            for p in pattern {
                pat += "%";
                pat += &p.to_lowercase();
            }
            pat += "%";
            // Find the best match by "frecency"
            let path: String = conn.query_one(
                "
                SELECT path FROM zipzap WHERE path like ?
                ORDER BY rank * (3.75/((0.0001 * (? - time) + 1) + 0.25)) DESC
                LIMIT 1
                ",
                rusqlite::params![pat, now],
                |r| r.get(0),
            )?;
            println!("{path}");
        }
        Command::Install { shell } => {
            let shell = match shell {
                Some(s) => *s,
                None if sysinfo::IS_SUPPORTED_SYSTEM => {
                    let mut system = sysinfo::System::new_all();
                    system.refresh_processes(
                        sysinfo::ProcessesToUpdate::All,
                        true,
                    );
                    let my_pid = sysinfo::get_current_pid()
                        .expect("unable to get PID of the current process");
                    let parent_pid = system
                        .process(my_pid)
                        .expect("no self process?")
                        .parent()
                        .expect("unable to get parent process");
                    let parent_process = system
                        .process(parent_pid)
                        .expect("unable to get parent process");
                    let parent_name = parent_process.name();
                    let shell = match parent_name.to_str().ok_or_else(|| {
                        anyhow!("parent process name is not utf-8 (?!)")
                    })? {
                        "fish" => Shell::Fish,
                        "bash" => Shell::Bash,
                        "zsh" => Shell::Zsh,
                        s => bail!(
                            "unknown shell '{s}'; please specify with argument"
                        ),
                    };
                    println!(
                        "auto-detected '{}' shell",
                        match shell {
                            Shell::Fish => "fish",
                            Shell::Bash => "bash",
                            Shell::Zsh => "zsh",
                        }
                    );
                    shell
                }
                None => {
                    bail!(
                        "cannot auto-detect shell; please specify with argument"
                    );
                }
            };
            let home = base_dirs.home_dir();
            let changed = match shell {
                Shell::Fish => {
                    let mut changed = false;
                    changed |= copy_check(
                        home.join(".config/fish/conf.d/z.fish"),
                        include_str!("../fish/conf.d/z.fish"),
                    )?;
                    changed |= copy_check(
                        home.join(".config/fish/functions/z.fish"),
                        include_str!("../fish/functions/z.fish"),
                    )?;
                    changed
                }
                Shell::Bash => edit_rc(home, "bash")?,
                Shell::Zsh => edit_rc(home, "zsh")?,
            };
            if changed {
                println!("done; please restart your shell");
            }
        }
        Command::Source { shell } => match shell {
            Shell::Bash => println!("{}", include_str!("../bash/z.sh")),
            Shell::Zsh => println!("{}", include_str!("../zsh/z.zsh")),
            Shell::Fish => println!("# fish does not need a sourced script"),
        },
    }
    Ok(())
}

/// Sends a question to the user, expecting a `[y,n]` reply
fn read_yn(prompt: &str) -> std::io::Result<bool> {
    loop {
        print!("{prompt} [y/n]: ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => break Ok(true),
            "n" | "no" => break Ok(false),
            _ => println!("invalid input, please enter 'y' or 'n'."),
        }
    }
}

/// Appends an `eval` line to an `rc` file, returning `true` if things changed
fn edit_rc(home: &std::path::Path, shell: &str) -> anyhow::Result<bool> {
    let file = format!(".{shell}rc");
    let rc = home.join(&file);
    let mut text = match std::fs::read_to_string(&rc) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "\n".to_owned(),
        Err(e) => return Err(e.into()),
    };
    let zipzap_eval = format!(
        "eval \"$(zipzap source {shell})\" # added by 'zipzap install {shell}'"
    );
    // Search in reverse-order to bail out quickly
    if text.lines().rev().any(|line| line == zipzap_eval) {
        println!("shell integration is already installed");
        Ok(false)
    } else if read_yn(&format!("append shell integration to {file}?"))? {
        if !text.ends_with('\n') {
            text += "\n";
        }
        text += &zipzap_eval;
        text += "\n";

        // Atomically move the file into place
        let mut tmp = tempfile::NamedTempFile::new_in(home)?;
        tmp.write_all(text.as_bytes())?;
        tmp.flush()?;
        std::fs::rename(tmp.path(), rc)?;
        Ok(true)
    } else {
        bail!("exiting without editing {file}");
    }
}

/// Writes `text` to `path`, returning `true` if things changed
fn copy_check(path: std::path::PathBuf, text: &str) -> anyhow::Result<bool> {
    // Make sure the config path exists
    std::fs::create_dir_all(path.parent().expect("must have parent dir"))?;
    let prev = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    if let Some(prev) = prev {
        if prev == text {
            println!("'{path:?}' exists and matches");
            Ok(false)
        } else if read_yn(&format!("overwrite existing file at '{path:?}'?"))? {
            std::fs::write(path, text)?;
            Ok(true)
        } else {
            bail!("file at '{path:?}' exists and we won't overwrite it")
        }
    } else {
        println!("writing integration script to '{path:?}'");
        std::fs::write(path, text)?;
        Ok(true)
    }
}
