#[must_use]
pub fn get_version() -> &'static str {
  env!("CARGO_PKG_VERSION")
}
