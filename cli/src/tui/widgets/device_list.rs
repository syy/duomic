use ratatui::{
    prelude::*,
    widgets::{Block, List, ListItem, ListState as RatatuiListState, Widget},
};

use crate::audio::AudioDevice;

/// A selectable device list widget with arrow key navigation
pub struct DeviceList<'a> {
    devices: &'a [AudioDevice],
    selected: usize,
    block: Option<Block<'a>>,
}

impl<'a> DeviceList<'a> {
    pub fn new(devices: &'a [AudioDevice], selected: usize) -> Self {
        Self {
            devices,
            selected,
            block: None,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl Widget for DeviceList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = self
            .devices
            .iter()
            .enumerate()
            .map(|(i, device)| {
                let prefix = if i == self.selected { "● " } else { "○ " };
                let content = format!("{}{} ({} channels)", prefix, device.name, device.channels);

                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(content).style(style)
            })
            .collect();

        let mut list = List::new(items).highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

        if let Some(block) = self.block {
            list = list.block(block);
        }

        // Create list state for highlighting
        let mut state = RatatuiListState::default();
        state.select(Some(self.selected));

        // Render with state
        ratatui::widgets::StatefulWidget::render(list, area, buf, &mut state);
    }
}

/// Help bar for navigation hints
pub struct HelpBar<'a> {
    hints: &'a [(&'a str, &'a str)],
}

impl<'a> HelpBar<'a> {
    pub fn new(hints: &'a [(&'a str, &'a str)]) -> Self {
        Self { hints }
    }
}

impl Widget for HelpBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut x = area.x;

        for (key, action) in self.hints {
            if x >= area.x + area.width {
                break;
            }

            // Render key in brackets
            let key_str = format!("[{}]", key);
            buf.set_string(
                x,
                area.y,
                &key_str,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
            x += key_str.len() as u16 + 1;

            // Render action
            buf.set_string(x, area.y, *action, Style::default().fg(Color::Gray));
            x += action.len() as u16 + 2;
        }
    }
}

/// Status indicator widget
pub struct StatusIndicator<'a> {
    status: &'a str,
    is_ok: bool,
}

impl<'a> StatusIndicator<'a> {
    pub fn new(status: &'a str, is_ok: bool) -> Self {
        Self { status, is_ok }
    }
}

impl Widget for StatusIndicator<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (symbol, color) = if self.is_ok {
            ("●", Color::Green)
        } else {
            ("○", Color::Red)
        };

        buf.set_string(area.x, area.y, symbol, Style::default().fg(color));
        buf.set_string(
            area.x + 2,
            area.y,
            self.status,
            Style::default().fg(Color::White),
        );
    }
}
