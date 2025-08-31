use clap::{CommandFactory, Parser};

#[derive(Debug, Parser)]
#[command(author, version, about = "Orchestra CLI", long_about = None, bin_name = "orchestra")]
pub struct Cli {}

impl Cli {
  pub fn print_help_and_exit() {
    let mut cmd = Cli::command();
    cmd.print_help().expect("print help");
    println!();
  }
}
