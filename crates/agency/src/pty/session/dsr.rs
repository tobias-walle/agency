use termwiz::escape::csi::Cursor as CsiCursor;
use termwiz::escape::parser::Parser as EscapeParser;
use termwiz::escape::{Action as EscapeAction, CSI as EscapeCsi};

/// Detects CSI "Request Active Position Report" sequences that background CLI
/// agents emit to make sure the terminal is still alive. Some agents would
/// previously quit because the daemon never answered these probes.
pub(crate) struct CursorRequestDetector {
  parser: EscapeParser,
}

impl CursorRequestDetector {
  pub(crate) fn new() -> Self {
    Self {
      parser: EscapeParser::new(),
    }
  }

  pub(crate) fn consume(&mut self, chunk: &[u8]) -> usize {
    let mut matches = 0;
    self.parser.parse(chunk, |action| {
      if matches_cursor_request(&action) {
        matches += 1;
      }
    });
    matches
  }
}

fn matches_cursor_request(action: &EscapeAction) -> bool {
  matches!(
    action,
    EscapeAction::CSI(EscapeCsi::Cursor(CsiCursor::RequestActivePositionReport))
  )
}

#[cfg(test)]
mod tests {
  use super::CursorRequestDetector;

  #[test]
  fn detects_standard_dsr() {
    let mut detector = CursorRequestDetector::new();
    assert_eq!(detector.consume(b"\x1b[6n"), 1);
  }

  #[test]
  fn detects_dec_private_dsr() {
    let mut detector = CursorRequestDetector::new();
    assert_eq!(detector.consume(b"\x1b[?6n"), 1);
  }

  #[test]
  fn ignores_similar_sequences() {
    let mut detector = CursorRequestDetector::new();
    assert_eq!(detector.consume(b"\x1b[16n"), 0);
  }

  #[test]
  fn handles_chunk_boundaries() {
    let mut detector = CursorRequestDetector::new();
    assert_eq!(detector.consume(b"\x1b["), 0);
    assert_eq!(detector.consume(b"6"), 0);
    assert_eq!(detector.consume(b"n"), 1);
  }

  #[test]
  fn counts_multiple_requests() {
    let mut detector = CursorRequestDetector::new();
    assert_eq!(detector.consume(b"\x1b[6n\x1b[?6n"), 2);
  }
}
