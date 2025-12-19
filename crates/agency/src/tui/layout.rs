use ratatui::layout::Rect;

/// Center a rectangle within a parent area.
///
/// # Arguments
/// - `area`: The parent rectangle to center within
/// - `width_pct`: Width as a percentage of parent width (0-100)
/// - `height_rows`: Fixed height in rows
pub fn centered_rect(area: Rect, width_pct: u16, height_rows: u16) -> Rect {
  let w = area.width * width_pct / 100;
  let h = height_rows;
  let x = area.x + (area.width.saturating_sub(w)) / 2;
  let y = area.y + (area.height.saturating_sub(h)) / 2;
  Rect {
    x,
    y,
    width: w,
    height: h,
  }
}

/// Get the inner area of a rectangle, excluding 1-cell borders.
pub fn inner(area: Rect) -> Rect {
  Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn centered_rect_calculation() {
    let parent = Rect::new(0, 0, 100, 50);
    let result = centered_rect(parent, 50, 10);
    assert_eq!(result.width, 50);
    assert_eq!(result.height, 10);
    assert_eq!(result.x, 25);
    assert_eq!(result.y, 20);
  }

  #[test]
  fn inner_subtracts_borders() {
    let area = Rect::new(10, 10, 20, 10);
    let result = inner(area);
    assert_eq!(result.x, 11);
    assert_eq!(result.y, 11);
    assert_eq!(result.width, 18);
    assert_eq!(result.height, 8);
  }
}
