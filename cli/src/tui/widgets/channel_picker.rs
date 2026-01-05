use ratatui::{
    prelude::*,
    widgets::{Block, Widget},
};

/// Channel picker with real-time level preview
pub struct ChannelPicker<'a> {
    channels: u16,
    selected: usize,
    levels: &'a [f32],
    prompt: &'a str,
    block: Option<Block<'a>>,
}

impl<'a> ChannelPicker<'a> {
    pub fn new(channels: u16, selected: usize, levels: &'a [f32]) -> Self {
        Self {
            channels,
            selected,
            levels,
            prompt: "Create virtual mic?",
            block: None,
        }
    }

    pub fn prompt(mut self, prompt: &'a str) -> Self {
        self.prompt = prompt;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl Widget for ChannelPicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.height < 2 {
            return;
        }

        let channel_names = [
            "Left", "Right", "Center", "LFE", "Rear L", "Rear R", "Side L", "Side R",
        ];

        for i in 0..self.channels as usize {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let is_selected = i == self.selected;
            let level = self.levels.get(i).copied().unwrap_or(0.0);
            let channel_name = channel_names.get(i).unwrap_or(&"Channel");

            // Selection indicator
            let indicator = if is_selected { "→ " } else { "  " };
            let indicator_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            buf.set_string(inner.x, y, indicator, indicator_style);

            // Channel label
            let label = format!("Channel {} ({}):", i, channel_name);
            let label_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            buf.set_string(inner.x + 2, y, &label, label_style);

            // Level meter (inline, compact)
            let meter_start = inner.x + 2 + label.len() as u16 + 1;
            let meter_width = 16u16;

            if meter_start + meter_width < inner.x + inner.width {
                let fill = (level * meter_width as f32) as u16;

                for j in 0..meter_width {
                    let color = if j < meter_width * 3 / 4 {
                        Color::Green
                    } else if j < meter_width * 7 / 8 {
                        Color::Yellow
                    } else {
                        Color::Red
                    };

                    let (symbol, style) = if j < fill {
                        ("█", Style::default().fg(color))
                    } else {
                        ("░", Style::default().fg(Color::DarkGray))
                    };

                    buf.set_string(meter_start + j, y, symbol, style);
                }

                // dB value
                let db = if level <= 0.0 {
                    -60.0
                } else {
                    20.0 * level.log10()
                };
                let db_str = format!(" {:>3.0}dB", db);
                buf.set_string(
                    meter_start + meter_width,
                    y,
                    &db_str,
                    Style::default().fg(Color::Gray),
                );
            }
        }

        // Prompt at bottom
        if self.channels + 2 <= inner.height {
            let prompt_y = inner.y + self.channels + 1;
            let prompt_text = format!("→ {}: [y/n]", self.prompt);
            buf.set_string(
                inner.x,
                prompt_y,
                &prompt_text,
                Style::default().fg(Color::Yellow),
            );
        }
    }
}

/// Text input widget with cursor
pub struct TextInput<'a> {
    value: &'a str,
    cursor: usize,
    label: &'a str,
    block: Option<Block<'a>>,
}

impl<'a> TextInput<'a> {
    pub fn new(value: &'a str, cursor: usize) -> Self {
        Self {
            value,
            cursor,
            label: "Input:",
            block: None,
        }
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl Widget for TextInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
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

        // Render label
        buf.set_string(
            inner.x,
            inner.y,
            self.label,
            Style::default().fg(Color::Gray),
        );

        // Render input field
        let input_x = inner.x + self.label.len() as u16 + 1;
        let input_width = inner.width.saturating_sub(self.label.len() as u16 + 2);

        // Background for input field
        buf.set_string(input_x, inner.y, "> ", Style::default().fg(Color::Yellow));

        // Render value with cursor
        let display_value = if self.value.len() > input_width as usize - 3 {
            &self.value[self.value.len() - (input_width as usize - 3)..]
        } else {
            self.value
        };

        buf.set_string(
            input_x + 2,
            inner.y,
            display_value,
            Style::default().fg(Color::White),
        );

        // Render cursor
        let cursor_x = input_x + 2 + self.cursor.min(display_value.len()) as u16;
        buf.set_string(
            cursor_x,
            inner.y,
            "█",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::SLOW_BLINK),
        );
    }
}
