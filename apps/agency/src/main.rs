fn main() {
  // Initialize structured logging early
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let cfg = agency_core::config::load(Some(&root))
    .unwrap_or_else(|_| agency_core::config::Config::default());
  let log_path = root.join(".agency").join("cli.logs.jsonl");
  agency_core::logging::init(&log_path, cfg.log_level);

  cli::run();
}
