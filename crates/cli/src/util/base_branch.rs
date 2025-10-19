pub fn resolve_base_branch_default(root: &std::path::Path, provided: &str) -> String {
  if provided != "main" {
    return provided.to_string();
  }
  if let Ok(repo) = git2::Repository::open(root)
    && let Ok(head) = repo.head()
    && head.is_branch()
    && let Some(name) = head.shorthand()
    && !name.is_empty() && name != "main"
  {
    return name.to_string();
  }
  provided.to_string()
}
