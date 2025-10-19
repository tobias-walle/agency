use crate::{args, util::task_ref::parse_task_ref};

pub fn print_worktree_path(a: args::PathArgs) {
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&a.task);
  let tasks_dir = agency_core::adapters::fs::tasks_dir(&root);
  let mut found: Option<(u64, String)> = None;
  if let Ok(rd) = std::fs::read_dir(&tasks_dir) {
    for entry in rd.flatten() {
      let name = entry.file_name();
      let name = name.to_string_lossy().to_string();
      if let Ok((tid, slug)) = agency_core::domain::task::Task::parse_filename(&name) {
        let mut ok = false;
        if let Some(id) = tref.id {
          ok = tid.0 == id;
        }
        if !ok && let Some(ref s) = tref.slug {
          ok = &slug == s;
        }
        if ok {
          found = Some((tid.0, slug));
          break;
        }
      }
    }
  }
  let (id, slug) = match found {
    Some(x) => x,
    None => {
      eprintln!("task not found");
      std::process::exit(1);
    }
  };
  let path = agency_core::adapters::fs::worktree_path(&root, id, &slug);
  println!("{}", path.display());
}
