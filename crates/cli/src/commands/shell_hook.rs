pub fn print_shell_hook() {
  let hook = r#"# Agency shell hook: cd into a task worktree by id or slug
agcd() {
  if [ -z "$1" ]; then
    echo "usage: agcd <id|slug>" 1>&2
    return 1
  fi
  cd "$(agency path "$1")" || return 1
}
"#;
  println!("{}", hook);
}
