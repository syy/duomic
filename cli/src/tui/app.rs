use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, Stdout};

/// Terminal wrapper for TUI applications
pub struct Terminal {
    terminal: ratatui::Terminal<CrosstermBackend<Stdout>>,
}

impl Terminal {
    /// Create a new terminal and enter alternate screen mode
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = ratatui::Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Draw a frame
    pub fn draw<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        self.terminal.draw(f)?;
        Ok(())
    }

    /// Get terminal size
    pub fn size(&self) -> Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    /// Clear the terminal
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear()?;
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Restore terminal state
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

/// Application state enum for state machine pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Selecting input device
    SelectDevice,
    /// Configuring channels
    ConfigureChannels,
    /// Entering device name
    EnterName,
    /// Running capture with dashboard
    Running,
    /// Showing status
    Status,
    /// Error state with retry option
    Error,
    /// Application should quit
    Quit,
}

/// Selection list state
#[derive(Debug, Clone)]
pub struct ListState {
    pub selected: usize,
    pub items: Vec<String>,
}

impl ListState {
    pub fn new(items: Vec<String>) -> Self {
        Self { selected: 0, items }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.items.len() - 1);
        }
    }

    pub fn selected_item(&self) -> Option<&String> {
        self.items.get(self.selected)
    }
}

/// Text input state
#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_value(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }

    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn delete(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.value.remove(self.cursor);
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }
}
