# https://just.systems

_list:
  @just --list

# Start the app
start:
  cargo run -p orchestra

# Run the tests
test *ARGS:
  cargo test {{ARGS}}

# Check for compiler or linting error
check:
  cargo check
  cargo clippy
