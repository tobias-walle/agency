use anyhow::Result;
use std::io::{self};

pub fn confirm(prompt: &str) -> Result<bool> {
  anstream::println!("{}", prompt);
  let mut input = String::new();
  io::stdin().read_line(&mut input)?;
  let trimmed = input.trim();
  Ok(trimmed == "y" || trimmed == "Y")
}
