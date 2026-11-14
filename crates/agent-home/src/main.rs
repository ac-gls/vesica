//! agent-home CLI entry point.
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "agent-home", version, about = "Team agent home kit installer")]
struct Cli {}

fn main() {
    let _ = Cli::parse();
}
