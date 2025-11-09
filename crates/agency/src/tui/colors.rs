use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style as AnStyle};
use anstyle_parse::Parser;
use ratatui::style::{Color as TuiColor, Modifier, Style as TuiStyle};
use ratatui::text::Span;

#[allow(
  clippy::too_many_lines,
  clippy::cast_possible_truncation,
  clippy::match_same_arms
)]
pub fn ansi_to_spans(s: &str) -> Vec<Span<'static>> {
  struct Segments {
    cur_style: AnStyle,
    cur_text: String,
    segs: Vec<(String, AnStyle)>,
  }
  impl Segments {
    fn flush(&mut self) {
      if !self.cur_text.is_empty() {
        self
          .segs
          .push((std::mem::take(&mut self.cur_text), self.cur_style));
      }
    }
  }
  struct Performer(Segments);
  impl anstyle_parse::Perform for Performer {
    fn print(&mut self, c: char) {
      self.0.cur_text.push(c);
    }
    fn csi_dispatch(
      &mut self,
      params: &anstyle_parse::Params,
      _intermediates: &[u8],
      _ignore: bool,
      action: u8,
    ) {
      if action != b'm' {
        return;
      }
      // Flush and update style per parameter
      for p in params {
        for sub in p {
          match sub {
            0 => {
              self.0.flush();
              self.0.cur_style = AnStyle::new();
            }
            1 => {
              self.0.flush();
              self.0.cur_style = self.0.cur_style.bold();
            }
            3 => {
              self.0.flush();
              self.0.cur_style = self.0.cur_style.italic();
            }
            4 => {
              self.0.flush();
              self.0.cur_style = self.0.cur_style.underline();
            }
            30..=37 => {
              self.0.flush();
              let idx = (*sub as u8) - 30;
              let fg = Color::Ansi(match idx {
                0 => AnsiColor::Black,
                1 => AnsiColor::Red,
                2 => AnsiColor::Green,
                3 => AnsiColor::Yellow,
                4 => AnsiColor::Blue,
                5 => AnsiColor::Magenta,
                6 => AnsiColor::Cyan,
                _ => AnsiColor::White,
              });
              self.0.cur_style = self.0.cur_style.fg_color(Some(fg));
            }
            39 => {
              self.0.flush();
              self.0.cur_style = self.0.cur_style.fg_color(None);
            }
            90..=97 => {
              self.0.flush();
              let idx = (*sub as u8) - 90;
              let fg = Color::Ansi(match idx {
                0 => AnsiColor::BrightBlack,
                1 => AnsiColor::BrightRed,
                2 => AnsiColor::BrightGreen,
                3 => AnsiColor::BrightYellow,
                4 => AnsiColor::BrightBlue,
                5 => AnsiColor::BrightMagenta,
                6 => AnsiColor::BrightCyan,
                _ => AnsiColor::BrightWhite,
              });
              self.0.cur_style = self.0.cur_style.fg_color(Some(fg));
            }
            _ => {}
          }
        }
      }
    }
  }

  let mut parser = Parser::<anstyle_parse::DefaultCharAccumulator>::new();
  let mut perf = Performer(Segments {
    cur_style: AnStyle::new(),
    cur_text: String::new(),
    segs: Vec::new(),
  });
  for b in s.as_bytes() {
    parser.advance(&mut perf, *b);
  }
  perf.0.flush();
  perf
    .0
    .segs
    .into_iter()
    .map(|(text, st)| Span::styled(text, anstyle_to_style(&st)))
    .collect()
}

pub fn anstyle_to_style(s: &AnStyle) -> TuiStyle {
  let mut st = TuiStyle::default();
  if let Some(fg) = s.get_fg_color() {
    st = st.fg(to_tui_color(fg));
  }
  if let Some(bg) = s.get_bg_color() {
    st = st.bg(to_tui_color(bg));
  }
  let eff = s.get_effects();
  if eff.contains(Effects::BOLD) {
    st = st.add_modifier(Modifier::BOLD);
  }
  if eff.contains(Effects::ITALIC) {
    st = st.add_modifier(Modifier::ITALIC);
  }
  if eff.contains(Effects::UNDERLINE) {
    st = st.add_modifier(Modifier::UNDERLINED);
  }
  st
}

fn to_tui_color(c: Color) -> TuiColor {
  match c {
    Color::Ansi(AnsiColor::Black) => TuiColor::Black,
    Color::Ansi(AnsiColor::Red) => TuiColor::Red,
    Color::Ansi(AnsiColor::Green) => TuiColor::Green,
    Color::Ansi(AnsiColor::Yellow) => TuiColor::Yellow,
    Color::Ansi(AnsiColor::Blue) => TuiColor::Blue,
    Color::Ansi(AnsiColor::Magenta) => TuiColor::Magenta,
    Color::Ansi(AnsiColor::Cyan) => TuiColor::Cyan,
    Color::Ansi(AnsiColor::White) => TuiColor::White,
    Color::Ansi(AnsiColor::BrightBlack | AnsiColor::BrightWhite) => TuiColor::Gray,
    Color::Ansi(AnsiColor::BrightRed) => TuiColor::LightRed,
    Color::Ansi(AnsiColor::BrightGreen) => TuiColor::LightGreen,
    Color::Ansi(AnsiColor::BrightYellow) => TuiColor::LightYellow,
    Color::Ansi(AnsiColor::BrightBlue) => TuiColor::LightBlue,
    Color::Ansi(AnsiColor::BrightMagenta) => TuiColor::LightMagenta,
    Color::Ansi(AnsiColor::BrightCyan) => TuiColor::LightCyan,
    Color::Rgb(RgbColor(r, g, b)) => TuiColor::Rgb(r, g, b),
    Color::Ansi256(Ansi256Color(idx)) => TuiColor::Indexed(idx),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_basic_color_sequences() {
    let spans = ansi_to_spans("\x1b[31mred\x1b[0m plain");
    assert!(spans.len() >= 2);
    assert_eq!(spans[0].content, "red");
    assert_eq!(spans[1].content, " plain");
  }
}
