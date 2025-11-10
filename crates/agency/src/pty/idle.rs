use crate::utils::term::strip_ansi_control_codes;
use std::borrow::Cow;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleState {
  Active,
  Idle,
}

impl Default for IdleState {
  fn default() -> Self {
    Self::Active
  }
}

#[derive(Debug, Clone, Copy)]
pub struct IdleThresholds {
  pub visible_quiet: Duration,
  pub enter_idle_after: Duration,
}

impl IdleThresholds {
  #[must_use]
  pub const fn new(visible_quiet: Duration, enter_idle_after: Duration) -> Self {
    Self {
      visible_quiet,
      enter_idle_after,
    }
  }
}

impl Default for IdleThresholds {
  fn default() -> Self {
    Self {
      visible_quiet: Duration::from_secs(3),
      enter_idle_after: Duration::from_millis(500),
    }
  }
}

#[derive(Debug)]
pub struct IdleTracker {
  thresholds: IdleThresholds,
  last_raw_activity: Instant,
  last_visible_activity: Instant,
  pending_idle_since: Option<Instant>,
  state: IdleState,
}

impl IdleTracker {
  #[must_use]
  pub fn new(now: Instant) -> Self {
    Self::with_thresholds(now, IdleThresholds::default())
  }

  #[must_use]
  pub fn with_thresholds(now: Instant, thresholds: IdleThresholds) -> Self {
    Self {
      thresholds,
      last_raw_activity: now,
      last_visible_activity: now,
      pending_idle_since: None,
      state: IdleState::default(),
    }
  }

  pub fn record_output(&mut self, now: Instant, chunk: &[u8]) {
    self.last_raw_activity = now;
    self.pending_idle_since = None;
    if chunk_has_visible_text(chunk) {
      self.last_visible_activity = now;
    }
  }

  pub fn record_input(&mut self, now: Instant) {
    self.last_raw_activity = now;
    self.last_visible_activity = now;
    self.pending_idle_since = None;
  }

  pub fn poll(&mut self, now: Instant) -> (IdleState, bool) {
    let should_be_idle = self.should_enter_idle(now);
    match self.state {
      IdleState::Active => {
        if should_be_idle {
          match self.pending_idle_since {
            Some(since) if now.duration_since(since) >= self.thresholds.enter_idle_after => {
              self.state = IdleState::Idle;
              self.pending_idle_since = None;
              (self.state, true)
            }
            Some(_) => (self.state, false),
            None => {
              self.pending_idle_since = Some(now);
              (self.state, false)
            }
          }
        } else {
          self.pending_idle_since = None;
          (self.state, false)
        }
      }
      IdleState::Idle => {
        if should_be_idle {
          (self.state, false)
        } else {
          self.state = IdleState::Active;
          self.pending_idle_since = None;
          (self.state, true)
        }
      }
    }
  }

  #[must_use]
  pub fn state(&self) -> IdleState {
    self.state
  }

  fn should_enter_idle(&self, now: Instant) -> bool {
    now.duration_since(self.last_visible_activity) >= self.thresholds.visible_quiet
  }
}

fn chunk_has_visible_text(chunk: &[u8]) -> bool {
  if chunk.is_empty() {
    return false;
  }
  let lossy: Cow<'_, str> = String::from_utf8_lossy(chunk);
  let stripped = strip_ansi_control_codes(lossy.as_ref());
  stripped.chars().any(|ch| !ch.is_control())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tracker(now: Instant) -> IdleTracker {
    IdleTracker::with_thresholds(
      now,
      IdleThresholds::new(Duration::from_millis(500), Duration::from_millis(100)),
    )
  }

  #[test]
  fn remains_active_during_recent_output() {
    let start = Instant::now();
    let mut tracker = tracker(start);

    tracker.record_output(start, "hello".as_bytes());
    let (state, changed) = tracker.poll(start + Duration::from_millis(150));
    assert_eq!(state, IdleState::Active);
    assert!(!changed);
  }

  #[test]
  fn transitions_to_idle_after_quiet_period() {
    let start = Instant::now();
    let mut tracker = tracker(start);
    tracker.record_output(start, "data".as_bytes());

    let first_check = start + Duration::from_millis(600);
    let (state, changed) = tracker.poll(first_check);
    assert_eq!(state, IdleState::Active);
    assert!(!changed);

    let confirm = first_check + Duration::from_millis(150);
    let (state, changed) = tracker.poll(confirm);
    assert_eq!(state, IdleState::Idle);
    assert!(changed);
  }

  #[test]
  fn resumes_active_on_visible_output() {
    let start = Instant::now();
    let mut tracker = tracker(start);

    // Make it idle
    let idle_after = start + Duration::from_millis(800);
    tracker.poll(idle_after);
    tracker.poll(idle_after + Duration::from_millis(150));
    assert_eq!(tracker.state(), IdleState::Idle);

    let resume_time = idle_after + Duration::from_millis(200);
    tracker.record_output(resume_time, "ping".as_bytes());
    let (state, changed) = tracker.poll(resume_time + Duration::from_millis(10));
    assert_eq!(state, IdleState::Active);
    assert!(changed);
  }

  #[test]
  fn control_sequences_do_not_reset_visible_timer() {
    let start = Instant::now();
    let mut tracker = tracker(start);
    tracker.record_output(start, "visible".as_bytes());

    // Only control sequences follow
    let ctrl = b"\x1b[2J\x1b[H";
    tracker.record_output(start + Duration::from_millis(50), ctrl);
    tracker.record_output(start + Duration::from_millis(100), ctrl);

    let quiet = start + Duration::from_millis(700);
    let (_, changed) = tracker.poll(quiet);
    assert!(!changed);

    let confirm = quiet + Duration::from_millis(120);
    let (state, changed) = tracker.poll(confirm);
    assert_eq!(state, IdleState::Idle);
    assert!(changed);
  }

  #[test]
  fn utf8_boundaries_are_handled() {
    let start = Instant::now();
    let mut tracker = tracker(start);
    // Split a multi-byte character into two calls; both should be safe.
    tracker.record_output(start, &[0xC3]);
    tracker.record_output(start + Duration::from_millis(10), &[0xA9]);
    let (state, _) = tracker.poll(start + Duration::from_millis(100));
    assert_eq!(state, IdleState::Active);
  }
}
