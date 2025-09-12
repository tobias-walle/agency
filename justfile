# https://just.systems

_list:
  @just --list

# Setup the development environment
setup:
  cargo check

# Start the app
agency *ARGS:
  cargo run -p agency -- {{ARGS}}

# Run the tests with nextest
test *ARGS:
  cargo nextest run {{ARGS}}

# Check for compiler or linting error
check:
  cargo clippy --tests

# Format the code
fmt:
  cargo fmt --all

# Fix the linting errors and formatting
fix:
  cargo clippy --allow-dirty --allow-staged --tests --fix
  just fmt
