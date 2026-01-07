mod branch;
mod command;
mod query;
mod repo;
mod worktree;

// Re-export public APIs from branch module
pub use branch::{
  current_branch_name_at, delete_branch_if_exists, delete_branch_if_exists_at, ensure_branch_at,
  head_branch, update_branch_ref_at,
};

// Re-export public APIs from command module
pub use command::{hard_reset_to_head_at, rebase_onto, stash_pop, stash_push};

// Re-export public APIs from query module
pub use query::{
  commits_ahead_at, is_fast_forward_at, rev_parse, uncommitted_numstat_at, worktree_is_clean_at,
};

// Re-export public APIs from repo module
pub use repo::{git_workdir, open_main_repo, repo_workdir_or, resolve_main_workdir};

// Re-export public APIs from worktree module
pub use worktree::{add_worktree_for_branch, prune_worktree_if_exists, prune_worktree_if_exists_at};
