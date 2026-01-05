use ratatui::{
    prelude::*,
    widgets::{Block, Widget},
};

use crate::audio::amplitude_to_db;

/// A gradient audio level meter widget
///
/// Displays audio level with gradient colors:
/// - Green: -60dB to -12dB (safe)
/// - Yellow: -12dB to -6dB (caution)
/// - Red: -6dB to 0dB (peak)
pub struct LevelMeter<'a> {
    /// Current level (0.0 to 1.0 linear amplitude)
    level: f32,
    /// Label to display
    label: Option<&'a str>,
    /// Show dB value
    show_db: bool,
    /// Block for borders
    block: Option<Block<'a>>,
}

impl<'a> LevelMeter<'a> {
    pub fn new(level: f32) -> Self {
        Self {
            level: level.clamp(0.0, 1.0),
            label: None,
            show_db: true,
            block: None,
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn show_db(mut self, show: bool) -> Self {
        self.show_db = show;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Get color for a given dB level
    fn color_for_db(db: f32) -> Color {
        if db >= -6.0 {
            Color::Red
        } else if db >= -12.0 {
            Color::Yellow
        } else {
            Color::Green
        }
    }
}

impl Widget for LevelMeter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render block if present
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < 10 || inner.height < 1 {
            return;
        }

        // Calculate layout
        let label_width = self.label.map(|l| l.len() as u16 + 1).unwrap_or(0);
        let db_width = if self.show_db { 8 } else { 0 }; // " -12dB "
        let meter_width = inner.width.saturating_sub(label_width + db_width);

        if meter_width < 5 {
            return;
        }

        let y = inner.y;
        let mut x = inner.x;

        // Render label
        if let Some(label) = self.label {
            buf.set_string(x, y, label, Style::default().fg(Color::White));
            x += label_width;
        }

        // Calculate meter fill
        let db = amplitude_to_db(self.level);
        let db_normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let fill_width = (meter_width as f32 * db_normalized) as u16;

        // Render meter bar with gradient
        for i in 0..meter_width {
            let char_db = -60.0 + (i as f32 / meter_width as f32) * 60.0;
            let color = Self::color_for_db(char_db);

            let (symbol, style) = if i < fill_width {
                ("█", Style::default().fg(color))
            } else {
                ("░", Style::default().fg(Color::DarkGray))
            };

            buf.set_string(x + i, y, symbol, style);
        }

        // Render dB value
        if self.show_db {
            let db_str = if db <= -60.0 {
                " -∞dB".to_string()
            } else {
                format!(" {:>3.0}dB", db)
            };
            buf.set_string(
                x + meter_width,
                y,
                &db_str,
                Style::default().fg(Color::Gray),
            );
        }
    }
}

/// Multi-channel level meter display
pub struct MultiLevelMeter<'a> {
    /// Channel levels (linear amplitude 0.0 to 1.0)
    levels: &'a [f32],
    /// Channel labels
    labels: &'a [String],
}

impl<'a> MultiLevelMeter<'a> {
    pub fn new(levels: &'a [f32], labels: &'a [String]) -> Self {
        Self { levels, labels }
    }
}

impl Widget for MultiLevelMeter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let max_channels = area.height as usize;

        for (i, (level, label)) in self
            .levels
            .iter()
            .zip(self.labels.iter())
            .take(max_channels)
            .enumerate()
        {
            let row = Rect {
                x: area.x,
                y: area.y + i as u16,
                width: area.width,
                height: 1,
            };

            LevelMeter::new(*level)
                .label(label)
                .show_db(true)
                .render(row, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_for_db() {
        assert_eq!(LevelMeter::color_for_db(-30.0), Color::Green);
        assert_eq!(LevelMeter::color_for_db(-10.0), Color::Yellow);
        assert_eq!(LevelMeter::color_for_db(-3.0), Color::Red);
    }
}
