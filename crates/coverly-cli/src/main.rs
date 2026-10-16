//! `coverly`: command-line interface of Coverly ROMs.

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "coverly", version, about = "Audit and fill ROM cover art")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    // Subcommands (e.g. `audit`) arrive in later milestones; the core version
    // is linked so the CLI and the library never drift apart silently.
    debug_assert!(!coverly_core::VERSION.is_empty());
}
