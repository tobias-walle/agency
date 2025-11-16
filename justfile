# https://just.systems

_list:
  @just --list

# Start the app
[no-exit-message]
agency *ARGS:
  #!/usr/bin/env sh
  repo_root="$(dirname $(git rev-parse --path-format=absolute --git-common-dir))"
  socket_path="$repo_root/target/agency.sock"
  AGENCY_SOCKET_PATH="$socket_path" cargo run -p agency -- {{ARGS}}

# Install agency from source globally
install-globally:
  cargo install --path crates/agency

# Run the tests with nextest
test *ARGS:
  RUSTFLAGS="${RUSTFLAGS:-} -Awarnings" cargo nextest run --no-fail-fast --cargo-quiet --status-level leak {{ARGS}}

# Check for compiler or linting errors
check:
  cargo clippy -q --all-targets -- -A warnings

# Check for compiler or linting errors (Verbose)
check-verbose:
  cargo clippy --all-targets

# Format the code
fmt:
  cargo fmt --all

# Fix the linting errors and formatting
fix:
  cargo clippy -q --allow-dirty --allow-staged --all-targets --fix -- -F clippy::pedantic -A clippy::missing-errors-doc
  just fmt

tmux *ARGS:
  tmux -S $AGENCY_TMUX_SOCKET_PATH {{ARGS}}

tmux-kill:
  just tmux kill-server
