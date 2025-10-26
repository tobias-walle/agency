use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

pub struct TaskFileName {
  pub id: u32,
  pub slug: String,
}

impl TaskFileName {
  pub fn parse(path: &Path) -> Option<Self> {
    let name = path.file_name()?.to_str()?;
    // Cached regex: ^(\d+)-(.+)\.md$
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(\d+)-(.+)\.md$").expect("valid regex"));
    let caps = re.captures(name)?;
    let id_str = caps.get(1)?.as_str();
    let slug = caps.get(2)?.as_str().to_string();
    let Ok(id) = id_str.parse::<u32>() else {
      return None;
    };
    Some(TaskFileName { id, slug })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;
  use std::fs::File;
  use std::io::Write;
  use std::path::PathBuf;

  #[test]
  fn parses_valid_names() {
    let file_path = create_temp_file("12-märchen-test.md");
    let parsed = TaskFileName::parse(&file_path).expect("Should parse valid file name");
    assert_eq!(parsed.id, 12);
    assert_eq!(parsed.slug, "märchen-test");
    std::fs::remove_file(&file_path).unwrap();
  }

  #[test]
  fn rejects_invalid_names() {
    let bad = ["readme.md", "abc-slug.md", "12-.md", "12-slug.txt"];
    for name in bad.iter() {
      let file_path = create_temp_file(name);
      assert!(
        TaskFileName::parse(&file_path).is_none(),
        "Should reject: {}",
        name
      );
      std::fs::remove_file(&file_path).unwrap();
    }
  }

  /// Helper to create a temp file with given name and dummy content, returns its path
  fn create_temp_file(name: &str) -> PathBuf {
    let dir = env::temp_dir();
    let file_path = dir.join(name);
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "# dummy").unwrap();
    file_path
  }
}
