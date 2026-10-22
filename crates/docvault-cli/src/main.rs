use anyhow::Result;
use clap::Parser;

mod avi;
mod cli;
mod commands;

use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Encode(args) => commands::encode::run(args),
        Commands::Decode(args) => commands::decode::run(args),
    }
}
