fn main() {
  // Initialize structured logging early
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let cfg = orchestra_core::config::load(Some(&root))
    .unwrap_or_else(|_| orchestra_core::config::Config::default());
  let log_path = orchestra_core::adapters::fs::logs_path(&root);
  orchestra_core::logging::init(&log_path, cfg.log_level);

  cli::run();
}
