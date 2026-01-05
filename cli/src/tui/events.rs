use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::thread;
use std::time::Duration;

/// Terminal events that can be handled by the TUI
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Key press event
    Key(KeyEvent),
    /// Tick event for UI updates
    Tick,
    /// Window resize
    Resize(u16, u16),
}

/// Event handler for terminal input
pub struct EventHandler {
    receiver: Receiver<AppEvent>,
    _handle: thread::JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler with the specified tick rate
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = bounded(100);

        let handle = thread::spawn(move || {
            Self::event_loop(sender, tick_rate);
        });

        Self {
            receiver,
            _handle: handle,
        }
    }

    fn event_loop(sender: Sender<AppEvent>, tick_rate: Duration) {
        loop {
            // Poll for events with timeout
            if event::poll(tick_rate).unwrap_or(false) {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if sender.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Resize(w, h)) => {
                        if sender.send(AppEvent::Resize(w, h)).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // Send tick event
            if sender.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    }

    /// Get the next event
    pub fn next(&self) -> Result<AppEvent> {
        Ok(self.receiver.recv()?)
    }

    /// Try to get the next event without blocking
    pub fn try_next(&self) -> Option<AppEvent> {
        self.receiver.try_recv().ok()
    }
}

/// Common key actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    Up,
    Down,
    Left,
    Right,
    Select,
    Cancel,
    Yes,
    No,
    Restart,
    Setup,
    Retry,
    Backspace,
    Char(char),
    None,
}

impl KeyAction {
    /// Convert KeyEvent to KeyAction for navigation/menu contexts
    /// Use this when NOT in text input mode
    pub fn from_navigation(key: KeyEvent) -> Self {
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return KeyAction::Quit;
        }

        match key.code {
            KeyCode::Char('q') => KeyAction::Quit,
            KeyCode::Char('y') => KeyAction::Yes,
            KeyCode::Char('n') => KeyAction::No,
            KeyCode::Char('r') => KeyAction::Restart,
            KeyCode::Char('s') => KeyAction::Setup,
            KeyCode::Up => KeyAction::Up,
            KeyCode::Down => KeyAction::Down,
            KeyCode::Left => KeyAction::Left,
            KeyCode::Right => KeyAction::Right,
            KeyCode::Enter => KeyAction::Select,
            KeyCode::Esc => KeyAction::Cancel,
            KeyCode::Backspace => KeyAction::Backspace,
            KeyCode::Char(c) => KeyAction::Char(c),
            _ => KeyAction::None,
        }
    }

    /// Convert KeyEvent to KeyAction for text input contexts
    /// All character keys pass through as Char(c)
    pub fn from_text_input(key: KeyEvent) -> Self {
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return KeyAction::Quit;
        }

        match key.code {
            KeyCode::Enter => KeyAction::Select,
            KeyCode::Esc => KeyAction::Cancel,
            KeyCode::Backspace => KeyAction::Backspace,
            KeyCode::Char(c) => KeyAction::Char(c), // All chars pass through!
            _ => KeyAction::None,
        }
    }
}

impl From<KeyEvent> for KeyAction {
    fn from(key: KeyEvent) -> Self {
        // Default behavior: navigation mode (for backwards compatibility)
        KeyAction::from_navigation(key)
    }
}
