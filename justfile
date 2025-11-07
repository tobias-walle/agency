# https://just.systems

_list:
  @just --list

# Start the app
[no-exit-message]
@agency *ARGS:
  cargo run -q -p agency -- {{ARGS}}

# Run the tests with nextest
test *ARGS:
  cargo nextest run --cargo-quiet --status-level leak {{ARGS}}

# Check for compiler or linting errors
check:
  cargo clippy -q --all-targets -- -A warnings

# Format the code
fmt:
  cargo fmt --all

# Fix the linting errors and formatting
fix:
  cargo clippy -q --allow-dirty --allow-staged --all-targets --fix -- -W clippy::pedantic -A clippy::missing-errors-doc
  just fmt
