use owo_colors::OwoColorize as _;

fn main() {
  if let Err(err) = agency::run() {
    anstream::eprintln!("{}", err.to_string().red());
    std::process::exit(1);
  }
}
