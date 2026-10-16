//! `coverly`: command-line interface of Coverly ROMs.
//!
//! Exit codes: 0 ok, 1 I/O error, 2 bad usage, 3 library root not found or
//! not a folder, 4 no layout profile detected and none given.

mod audit;
mod table;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coverly_core::profiles::{builtin_profiles, Media};
use coverly_core::systems::SystemDb;

#[derive(Debug, Parser)]
#[command(name = "coverly", version, about = "Audit and fill ROM cover art")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report which ROMs of a library lack cover, screenshot or title art
    /// (read-only).
    Audit(audit::AuditArgs),
    /// List the built-in layout profiles.
    Profiles,
    /// List the known systems and their folder aliases.
    Systems,
}

/// A failure with its exit code and a message for stderr.
#[derive(Debug)]
pub struct Failure {
    /// Process exit code.
    pub code: u8,
    /// Printed as `error: <message>`.
    pub message: String,
}

impl Failure {
    /// Bad usage that clap could not catch (exit 2).
    pub fn usage(message: impl Into<String>) -> Failure {
        Failure {
            code: 2,
            message: message.into(),
        }
    }
}

impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Failure {
        Failure {
            code: 1,
            message: e.to_string(),
        }
    }
}

fn main() -> ExitCode {
    debug_assert!(!coverly_core::VERSION.is_empty());
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Audit(args) => audit::run(&args),
        Command::Profiles => print_stdout(&profiles_table()),
        Command::Systems => print_stdout(&systems_table()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("error: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

/// Writes `text` to stdout; a closed pipe (`coverly … | head`) is not an
/// error.
pub fn print_stdout(text: &str) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e.into()),
        _ => Ok(()),
    }
}

/// The serialised name of a media, e.g. `cover`.
pub fn media_name(media: Media) -> &'static str {
    match media {
        Media::Cover => "cover",
        Media::Screenshot => "screenshot",
        Media::Title => "title",
    }
}

fn profiles_table() -> String {
    let mut rows = vec![vec![
        "ID".to_string(),
        "NAME".to_string(),
        "ROMS-ROOT".to_string(),
        "MEDIA".to_string(),
    ]];
    for p in builtin_profiles() {
        let media: Vec<&str> = Media::ALL
            .iter()
            .filter(|m| p.slot(**m).is_some())
            .map(|m| media_name(*m))
            .collect();
        let roms_root = if p.roms_root.is_empty() {
            ".".to_string()
        } else {
            p.roms_root.clone()
        };
        rows.push(vec![p.id, p.name, roms_root, media.join(",")]);
    }
    table::render(&rows)
}

fn systems_table() -> String {
    let db = SystemDb::builtin();
    let mut rows = vec![vec![
        "ID".to_string(),
        "NAME".to_string(),
        "ALIASES".to_string(),
    ]];
    for s in db.all() {
        rows.push(vec![s.id.clone(), s.name.clone(), s.aliases.join(", ")]);
    }
    table::render(&rows)
}
