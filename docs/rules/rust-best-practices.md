# Rust Readability Cheat Sheet

> Assumes you already run **rustfmt** and **clippy**. Focused on examples and practical patterns.

---

## 1) Code Organization (crates, modules, visibility)

**Keep related items together; expose only what you intend.** Prefer a thin binary (`main.rs`) and a fat library (`lib.rs`). Co-locate a type and its `impl` in the same module; use `pub(crate)` for internal APIs and re-exports for a clean surface.

```rust
// src/lib.rs
pub mod http {
    pub mod client {
        use std::time::Duration;

        pub struct Client {
            timeout: Duration,
        }

        impl Client {
            pub fn new(timeout: Duration) -> Self { Self { timeout } }
            // idiomatic getter: no `get_`
            pub fn timeout(&self) -> Duration { self.timeout }
        }
    }

    // Re-export for a flatter public API:
    pub use client::Client;
}

// src/main.rs (thin binary crate around the library)
use mycrate::http::Client;

fn main() {
    let c = Client::new(std::time::Duration::from_secs(2));
    println!("timeout = {:?}", c.timeout());
}
```

_Tips:_

- Split giant “utils” into focused modules; keep functions small.
- Prefer `pub(crate)` for crate-internal surfaces; only `pub` what you must.
- Co-locate tests with the code:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn constructs() { assert_eq!(Client::new(Default::default()).timeout(), Default::default()); }
  }
  ```

---

## 2) Naming (clarity over brevity)

Follow Rust casing: **UpperCamelCase** for types/traits; **snake_case** for fns/vars/modules; **SCREAMING_SNAKE_CASE** for consts. Prefer descriptive names; avoid unnecessary abbreviations. Don’t use `get_` for simple field access. Use `as_/to_/into_` for conversions.

```rust
pub struct DataPoint { value: i64 }

impl DataPoint {
    pub fn value(&self) -> i64 { self.value }        // getter w/o `get_`
    pub fn to_f64(&self) -> f64 { self.value as f64 } // conversion naming
}

const MAX_CONNECTIONS: usize = 1024;

fn compute_average(points: &[DataPoint]) -> f64 {
    let sum: i64 = points.iter().map(|p| p.value()).sum();
    sum as f64 / points.len() as f64
}
```

---

## 3) Error Handling (libs: `thiserror`; apps: `anyhow`)

**Use `Result` + `?`** to keep the happy path flat. Libraries model precise error enums with `thiserror`. Applications use `anyhow` for flexible errors + context.

```toml
# Cargo.toml
[dependencies]
thiserror = "1"
anyhow = "1"
```

**Library error type:**

```rust
// src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid key: {0}")]
    InvalidKey(String),
}

// src/lib.rs
use std::fs;
pub use crate::error::StoreError;

pub fn load(path: &str) -> Result<String, StoreError> {
    let s = fs::read_to_string(path)?;          // propagates as StoreError::Io
    if s.is_empty() { return Err(StoreError::InvalidKey("empty".into())); }
    Ok(s)
}
```

**Application entrypoint:**

```rust
// src/main.rs
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let cfg = std::fs::read_to_string("config.toml")
        .context("reading config.toml failed")?;
    println!("loaded {} bytes", cfg.len());
    Ok(())
}
```

_Guidelines:_

- Prefer `Result` over panics; reserve `panic!` for unrecoverable invariants.
- Avoid unchecked `unwrap()`/`expect()`; use `?` or handle explicitly.
- Add helpful context at boundaries (I/O, network, DB).

---

## 4) Documentation That Reads Like a Spec

Write `///` docs for public items. Start with a short summary, then `# Examples`, `# Errors`, `# Panics`, `# Safety` as needed. Doc examples are compiled by `cargo test`.

````rust
/// Parses a port from a string.
///
/// # Examples
/// ```
/// use mycrate::parse_port;
/// assert_eq!(parse_port("80").unwrap(), 80);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// Returns an error if the input is not a positive `u16`.
pub fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    let n: u16 = s.parse()?;
    Ok(n)
}
````

For libraries, add crate-level docs in `lib.rs` with `//!` summarizing purpose and a quick start.

---

## 5) Idiomatic Patterns for Clarity & Maintainability

**Think in expressions; minimize mutation:**

```rust
let action = if user.is_admin() { "allow" } else { "deny" };
```

**Use iterator combinators to make intent pop:**

```rust
let ids: Vec<_> = users.iter().filter(|u| u.active).map(|u| u.id).collect();
```

**Pattern matching & `if let`:**

```rust
if let Some(user) = users.iter().find(|u| u.id == target) {
    println!("found {user:?}");
}
```

**Make invalid states unrepresentable:**

```rust
// Bad
struct Job { running: bool }

// Good
enum JobState { Running, Stopped }
struct Job { state: JobState }
```

**Builder pattern for complex construction (readable call sites):**

```rust
let cfg = AppConfig::builder().port(80).address("0.0.0.0").enable_tls(true).build();
```

**Prefer `const`/`const fn` when possible; minimize `unsafe`;**
derive common traits (`Debug`, `Display`, `Default`, `Clone`, `Eq`, …) to integrate with the ecosystem.

---

## 6) Refs & Lifetimes (General)

**Accept borrows; return owned.** Avoid leaking lifetimes in public APIs unless you truly need them.

```rust
use std::path::Path;

// Flexible input; ergonomic owned output
fn read_all<P: AsRef<Path>>(p: P) -> std::io::Result<String> {
    std::fs::read_to_string(p)
}

// Avoid lifetime on public struct unless necessary:
pub struct User { name: String }        // prefer owned
// pub struct User<'a> { name: &'a str } // only if you truly need it
```

**Use `Cow` to “borrow if possible, own if necessary”:**

```rust
use std::borrow::Cow;

fn trimmed<'a>(s: &'a str) -> Cow<'a, str> {
    if s.trim().len() == s.len() {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.trim().to_owned())
    }
}
```

---

## 7) Async/Await with Tokio (practical patterns)

### 7.1 Don’t hold borrows across `.await`

Shrink the borrow; take/clone what you need first.

```rust
// 👎 borrow spans await (lifetime/Send headaches)
async fn save_bad(buf: &mut String) -> anyhow::Result<()> {
    let slice = &buf[..];
    send_to_server(slice).await?;
    Ok(())
}

// 👍 limit borrow before awaiting
async fn save_ok(buf: &mut String) -> anyhow::Result<()> {
    let owned = buf.clone();                // or std::mem::take if appropriate
    send_to_server(&owned).await?;
    Ok(())
}
```

**Mutating fields across await:** temporarily take, then put back.

```rust
struct Conn { buf: String }

impl Conn {
    async fn flush(&mut self) -> anyhow::Result<()> {
        let buf = std::mem::take(&mut self.buf); // take ownership
        send_to_server(&buf).await?;
        self.buf = buf;                           // restore
        Ok(())
    }
}
```

### 7.2 Prefer `Arc` for shared state (clarity > micro-perf)

Tokio’s multi-threaded runtime requires `Send + 'static` for spawned tasks. Default to **`Arc`** (and `Arc<Mutex/_>` or `Arc<tokio::sync::Mutex/_>` for mutability) instead of references/`Rc`.

```rust
use std::sync::Arc;

#[derive(Default)]
struct Config { port: u16 }

#[tokio::main]
async fn main() {
    let cfg = Arc::new(Config { port: 8080 });
    let cfg2 = Arc::clone(&cfg);

    tokio::spawn(async move {
        println!("listening on {}", cfg2.port);
    }).await.unwrap();
}
```

**Shared mutable state:** keep critical sections short; don’t hold locks across `.await`.

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

let state = Arc::new(Mutex::new(Vec::<u8>::new()));

let s = state.clone();
tokio::spawn(async move {
    {
        let mut v = s.lock().await;
        v.push(1); // do minimal work while holding the lock
    }             // guard dropped before awaiting other work
    do_other_async_work().await;
});
```

### 7.3 Spawning, cancellation, timeouts, and racing

**Spawning (`Send + 'static`):**

```rust
let handle = tokio::spawn(async move {
    Ok::<_, anyhow::Error>(work().await?)
});
```

**Cancellation:** dropping a `JoinHandle` detaches; call `abort()` to cancel.

```rust
let handle = tokio::spawn(async { loop { tick().await; } });
// ...
handle.abort(); // cooperative cancellation at next .await
```

**Timeouts & racing:**

```rust
use tokio::{select, time::{self, Duration}};

let r = time::timeout(Duration::from_secs(2), do_io()).await;

select! {
    _ = background_task() => {}
    _ = time::sleep(Duration::from_secs(5)) => { /* timed out */ }
}
```

**Don’t block the runtime:** offload blocking/CPU-heavy work.

```rust
let res = tokio::task::spawn_blocking(|| do_cpu_heavy()).await?;
```

**Prefer channels for coordination:**

```rust
use tokio::sync::{mpsc, oneshot, watch, broadcast};

let (tx, mut rx) = mpsc::channel(128);    // many -> one
let (wtx, wrx) = watch::channel(State::default()); // latest value
let (btx, _brx) = broadcast::channel(64); // pub-sub
```

**Local tasks for `!Send` data:**

```rust
use std::rc::Rc;
use tokio::task::LocalSet;

let local = LocalSet::new();
local.spawn_local(async {
    let rc = Rc::new(5); // !Send
    use_rc(rc).await;
});
local.run_until(async {}).await;
```

---

## 8) TL;DR “pin to your editor”

- **Avoid lifetimes** on public structs/APIs unless they unlock real benefits; **own outputs**.
- **Accept borrows** for inputs (`&str`, `&[T]`, `AsRef<_>`); **return owned** (or `Cow`).
- In Tokio, **prefer `Arc`** (even with modest perf cost) for cross-task sharing; clone handles freely.
- **Shrink borrows before `.await`**; don’t hold locks/refs across awaits.
- **`?` everywhere** for propagation; libs: `thiserror`; apps: `anyhow` + `.context(...)`.
- **Prefer channels** over shared mutable state; if locking, keep critical sections tiny.
- **Use `spawn_blocking`** for blocking/CPU-heavy tasks; don’t stall the runtime.
- **Cancel explicitly** with `.abort()`; don’t rely on dropping the handle.
- **Re-export** to flatten public APIs; **pub(crate)** for internals; **small modules** with co-located tests.
- **Name things clearly**; getters without `get_`; use `as_/to_/into_` conventions.
