use anyhow::Result;
use clap::Parser;

/// Agency - An AI agent manager and orchestrator in your command line.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {}

pub fn parse() -> Cli {
  Cli::parse()
}

pub fn run() -> Result<()> {
  let _cli = parse();
  Ok(())
}
