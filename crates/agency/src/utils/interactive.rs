use anyhow::Result;
use crossbeam_channel::{Sender, bounded};
use parking_lot::Mutex;

#[derive(Debug)]
pub enum InteractiveReq {
  Begin { ack: Sender<()> },
  End { ack: Sender<()> },
}

static TX: Mutex<Option<Sender<InteractiveReq>>> = Mutex::new(None);

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
  if let Some(()) = with_tx(|tx| {
    let (ack_tx, ack_rx) = bounded::<()>(0);
    let _ = tx.send(InteractiveReq::Begin { ack: ack_tx });
    let _ = ack_rx.recv();
  }) {
    // handled above
  }
  Ok(())
}

pub fn end() -> Result<()> {
  if let Some(()) = with_tx(|tx| {
    let (ack_tx, ack_rx) = bounded::<()>(0);
    let _ = tx.send(InteractiveReq::End { ack: ack_tx });
    let _ = ack_rx.recv();
  }) {
    // handled above
  }
  Ok(())
}

pub fn scope<F, R>(f: F) -> Result<R>
where
  F: FnOnce() -> Result<R>,
{
  begin()?;
  struct EndGuard;
  impl Drop for EndGuard {
    fn drop(&mut self) {
      let _ = super::interactive::end();
    }
  }
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
    let _ =
      scope(|| Ok::<_, anyhow::Error>(())).expect("scope should succeed without registration");
  }
}
