mod cleanup;
mod content;
mod metadata;
mod paths;
mod resolution;
mod slug;

// Re-export all public items from submodules
pub(crate) use cleanup::cleanup_task_artifacts;
pub(crate) use content::{
  edit_task_description, parse_task_markdown, read_task_content, read_task_frontmatter,
  write_task_content,
};
pub(crate) use metadata::{
  agent_for_task, TaskContent, TaskFrontmatter, TaskFrontmatterExt, TaskRef,
};
pub(crate) use paths::{branch_name, task_file, worktree_dir, worktree_name};
pub(crate) use resolution::{list_tasks, resolve_id_or_slug, resolve_task_ident};
pub(crate) use slug::{compute_unique_slug, next_id, normalize_and_validate_slug};

#[cfg(test)]
mod tests {

  use super::*;
  use crate::config::AgencyPaths;
  use tempfile::TempDir;

  #[test]
  fn parses_valid_task_file() {
    let dir = TempDir::new().expect("tmp");
    let file = dir.path().join("12-sample-task.md");
    std::fs::write(&file, "# test\n").unwrap();
    let tf = TaskRef::from_task_file(&file).expect("should parse valid");
    assert_eq!(tf.id, 12);
    assert_eq!(tf.slug, "sample-task");
  }

  #[test]
  fn normalize_and_validate_slug_rules() {
    // Success cases with slugify behavior
    assert_eq!(
      normalize_and_validate_slug("Alpha World").unwrap(),
      "alpha-world"
    );
    assert_eq!(
      normalize_and_validate_slug("alpha_world").unwrap(),
      "alpha-world"
    );
    assert_eq!(
      normalize_and_validate_slug("alpha---world").unwrap(),
      "alpha-world"
    );
    // Starting with a digit should be rejected
    assert!(normalize_and_validate_slug("1invalid").is_err());

    // Error cases: empty or becomes empty after slugify
    assert!(normalize_and_validate_slug("").is_err());
    assert!(normalize_and_validate_slug("---").is_err());
    assert!(normalize_and_validate_slug("   ").is_err());
    assert!(normalize_and_validate_slug("**").is_err());
  }

  #[test]
  fn resolve_names_and_paths() {
    let task = TaskRef {
      id: 7,
      slug: "alpha".to_string(),
    };
    assert_eq!(branch_name(&task), "agency/7-alpha");
    assert_eq!(worktree_name(&task), "7-alpha");

    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let wt_dir = worktree_dir(&paths, &task);
    assert!(wt_dir.ends_with(".agency/worktrees/7-alpha"));

    let tf_path = task_file(&paths, &task);
    assert!(tf_path.ends_with(".agency/tasks/7-alpha.md"));
  }

  #[test]
  fn parse_task_markdown_with_agent() {
    let input = "---\nagent: sh\n---\n\n# Task 1: alpha\n";
    let (fm, body) = parse_task_markdown(input);
    let fm = fm.expect("front matter present");
    assert_eq!(fm.agent.as_deref(), Some("sh"));
    assert!(body.starts_with("\n# Task 1: alpha\n") || body.starts_with("# Task 1: alpha\n"));
  }

  #[test]
  fn parse_task_markdown_without_front_matter() {
    let input = "# Task 2: beta\n";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn parse_task_markdown_ignores_unclosed_block() {
    let input = "---\nagent: sh\n# Task 3: gamma\n"; // no closing delimiter
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn resolve_id_or_slug_by_id_and_slug() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let tasks = paths.tasks_dir();
    std::fs::create_dir_all(&tasks).unwrap();

    std::fs::write(tasks.join("1-foo.md"), "# foo\n").unwrap();
    std::fs::write(tasks.join("2-bar.md"), "# bar\n").unwrap();

    let r1 = resolve_id_or_slug(&paths, "1").expect("id 1 present");
    assert_eq!(r1.id, 1);
    assert_eq!(r1.slug, "foo");

    let r2 = resolve_id_or_slug(&paths, "bar").expect("slug bar present");
    assert_eq!(r2.id, 2);
    assert_eq!(r2.slug, "bar");

    let not_found = resolve_id_or_slug(&paths, "baz");
    assert!(not_found.is_err());
  }

  #[test]
  fn write_and_read_task_content_without_header() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 1,
      slug: "sample-task".to_string(),
    };
    let frontmatter = TaskFrontmatter {
      agent: Some("agent-one".to_string()),
      base_branch: Some("main".to_string()),
    };
    let body = "Implement the feature\nwith bullet points\n".to_string();
    let content = TaskContent {
      frontmatter: Some(frontmatter.clone()),
      body: body.clone(),
    };

    write_task_content(&paths, &task, &content).expect("write succeeds");
    let stored_path = task_file(&paths, &task);
    let stored = std::fs::read_to_string(stored_path).expect("read stored file");

    assert!(
      !stored.contains("# Sample Task"),
      "stored content should not contain generated headers"
    );

    let roundtrip = read_task_content(&paths, &task).expect("roundtrip read");
    assert_eq!(roundtrip.body, body);
    assert_eq!(roundtrip.frontmatter, Some(frontmatter));
  }

  #[test]
  fn write_task_content_preserves_trailing_newline() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 2,
      slug: "another-task".to_string(),
    };

    let content = TaskContent {
      frontmatter: None,
      body: "Single line body".to_string(),
    };
    write_task_content(&paths, &task, &content).expect("write succeeds");

    let stored_path = task_file(&paths, &task);
    let stored = std::fs::read_to_string(stored_path).expect("read stored file");
    assert!(
      stored.ends_with('\n'),
      "stored body should end with newline, got: {stored:?}"
    );
  }

  #[test]
  fn base_branch_or_returns_stored_value() {
    let fm: Option<TaskFrontmatter> = Some(TaskFrontmatter {
      agent: None,
      base_branch: Some("feature-branch".to_string()),
    });
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "feature-branch");
  }

  #[test]
  fn base_branch_or_uses_fallback_when_none() {
    let fm: Option<TaskFrontmatter> = Some(TaskFrontmatter {
      agent: None,
      base_branch: None,
    });
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "fallback");
  }

  #[test]
  fn base_branch_or_uses_fallback_when_no_frontmatter() {
    let fm: Option<TaskFrontmatter> = None;
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "fallback");
  }
}
