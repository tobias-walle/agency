use std::sync::mpsc;
use std::time::Duration;

use crokey::{Combiner, KeyCombination};
use crossterm::event::Event;
use tracing::debug;

#[derive(Debug, Clone)]
pub enum EventMsg {
  Combo(KeyCombination),
  Paste(String),
}

pub fn spawn_event_reader(tx: mpsc::Sender<EventMsg>) -> std::thread::JoinHandle<()> {
  std::thread::spawn(move || {
    let mut combiner = Combiner::default();
    let enabled = combiner.enable_combining().unwrap_or(false);
    debug!(event = "cli_crokey_combining", enabled, "combiner combining state");
    loop {
      if let Ok(ready) = crossterm::event::poll(Duration::from_millis(50)) {
        if !ready {
          continue;
        }
        match crossterm::event::read() {
          Ok(Event::Key(k)) => {
            if let Some(kc) = combiner.transform(k) {
              let _ = tx.send(EventMsg::Combo(kc.normalized()));
            }
          }
          Ok(Event::Paste(s)) => {
            let _ = tx.send(EventMsg::Paste(s));
          }
          Ok(_) => {}
          Err(_) => break,
        }
      }
    }
  })
}
