# https://just.systems

_list:
  @just --list

# Start the app
start *ARGS:
  cargo run -p orchestra {{ARGS}}

# Run the tests
test *ARGS:
  cargo test {{ARGS}}

# Check for compiler or linting error
check:
  cargo check --tests
  cargo clippy --tests -- -D warnings

# Format the code
fmt:
  cargo fmt --all

# Fix the linting errors and formatting
fix:
  cargo clippy --allow-dirty --allow-staged --tests --fix
  just fmt
