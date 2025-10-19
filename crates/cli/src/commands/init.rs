pub fn init_project() {
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  if let Err(e) = agency_core::adapters::fs::ensure_layout(&root) {
    eprintln!("failed to create .agency layout: {e}");
    std::process::exit(1);
  }
  if let Err(e) = agency_core::config::write_default_project_config(&root) {
    eprintln!("failed to write config: {e}");
    std::process::exit(1);
  }
  println!("initialized .agency at {}", root.join(".agency").display());
}
