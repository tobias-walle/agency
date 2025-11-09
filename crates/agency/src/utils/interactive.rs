use anyhow::{Result, anyhow};
use crossbeam_channel::{Sender, bounded};
use parking_lot::Mutex;

#[derive(Debug)]
pub enum InteractiveReq {
  Begin { ack: Sender<()> },
  End { ack: Sender<()> },
}

static TX: Mutex<Option<Sender<InteractiveReq>>> = Mutex::new(None);

struct EndGuard;

impl Drop for EndGuard {
  fn drop(&mut self) {
    let _ = end();
  }
}

pub fn register_sender(sender: Sender<InteractiveReq>) {
  *TX.lock() = Some(sender);
}

fn with_tx<F, R>(f: F) -> Option<R>
where
  F: FnOnce(&Sender<InteractiveReq>) -> R,
{
  TX.lock().as_ref().map(f)
}

pub fn begin() -> Result<()> {
  if let Some(result) = with_tx(|tx| {
    let (ack_tx, ack_rx) = bounded::<()>(0);
    tx.send(InteractiveReq::Begin { ack: ack_tx })
      .map_err(|err| anyhow!("failed to send begin request: {err}"))?;
    ack_rx
      .recv()
      .map_err(|err| anyhow!("failed to receive begin acknowledgment: {err}"))
  }) {
    result?;
  }
  Ok(())
}

pub fn end() -> Result<()> {
  if let Some(result) = with_tx(|tx| {
    let (ack_tx, ack_rx) = bounded::<()>(0);
    tx.send(InteractiveReq::End { ack: ack_tx })
      .map_err(|err| anyhow!("failed to send end request: {err}"))?;
    ack_rx
      .recv()
      .map_err(|err| anyhow!("failed to receive end acknowledgment: {err}"))
  }) {
    result?;
  }
  Ok(())
}

pub fn scope<F, R>(f: F) -> Result<R>
where
  F: FnOnce() -> Result<R>,
{
  begin()?;
  let guard = EndGuard;
  let result = f();
  drop(guard);
  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_op_when_unregistered() {
    begin().unwrap();
    end().unwrap();
    let () =
      scope(|| Ok::<_, anyhow::Error>(())).expect("scope should succeed without registration");
  }
}
