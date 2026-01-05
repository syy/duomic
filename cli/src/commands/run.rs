use anyhow::Result;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::audio::{get_cpal_device, list_input_devices, AudioCapture, AudioDevice};
use crate::config::{Config, VirtualMicConfig};
use crate::ipc::{DeviceInfo, DriverClient, SharedAudioBuffer};
use crate::tui::{
    widgets::{DeviceList, HelpBar, LevelMeter},
    AppEvent, EventHandler, KeyAction, Terminal,
};

/// Ring buffer size (must match shm.rs and Driver)
const RING_BUFFER_FRAMES: u32 = 8192;

/// Global flag for signal-triggered cleanup
static CLEANUP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Unified application state machine
#[derive(Debug, Clone, PartialEq, Eq)]
enum AppState {
    /// Initial: Check if config exists and ask user
    AskAction,
    /// Select input device
    SelectDevice,
    /// Multi-select channels to use
    SelectChannels,
    /// Enter names for selected channels
    EnterNames,
    /// Running with dashboard
    Running,
    /// Error state
    Error(String),
    /// Quit
    Quit,
}

struct App {
    state: AppState,
    config: Config,

    // Device selection
    devices: Vec<AudioDevice>,
    selected_device_idx: usize,
    current_device: Option<AudioDevice>,

    // Channel selection (multi-select)
    channel_selected: Vec<bool>, // Which channels are selected
    channel_cursor: usize,       // Current cursor position
    channel_levels: Vec<f32>,    // Real-time levels for preview

    // Name entry
    channel_names: Vec<String>, // Names for selected channels
    name_cursor: usize,         // Which channel name we're editing
    name_input: String,         // Current input buffer

    // Action selection (for AskAction state)
    action_cursor: usize, // 0 = continue, 1 = new config

    // Dashboard
    dashboard_levels: Vec<f32>,
    dashboard_labels: Vec<String>,
    start_time: Option<Instant>,
    buffer_usage: f32,
}

impl App {
    fn new(devices: Vec<AudioDevice>, config: Config) -> Self {
        let has_config = config.device.name.is_some() && !config.virtual_mics.is_empty();
        let initial_state = if has_config {
            AppState::AskAction
        } else {
            AppState::SelectDevice
        };

        Self {
            state: initial_state,
            config,
            devices,
            selected_device_idx: 0,
            current_device: None,
            channel_selected: Vec::new(),
            channel_cursor: 0,
            channel_levels: Vec::new(),
            channel_names: Vec::new(),
            name_cursor: 0,
            name_input: String::new(),
            action_cursor: 0,
            dashboard_levels: Vec::new(),
            dashboard_labels: Vec::new(),
            start_time: None,
            buffer_usage: 0.0,
        }
    }

    fn handle_key(&mut self, action: KeyAction) -> Option<AppAction> {
        match &self.state {
            AppState::AskAction => self.handle_ask_action(action),
            AppState::SelectDevice => self.handle_select_device(action),
            AppState::SelectChannels => self.handle_select_channels(action),
            AppState::EnterNames => self.handle_enter_names(action),
            AppState::Running => self.handle_running(action),
            AppState::Error(_) => self.handle_error(action),
            AppState::Quit => None,
        }
    }

    fn handle_ask_action(&mut self, action: KeyAction) -> Option<AppAction> {
        match action {
            KeyAction::Up | KeyAction::Down => {
                self.action_cursor = if self.action_cursor == 0 { 1 } else { 0 };
                None
            }
            KeyAction::Select => {
                if self.action_cursor == 0 {
                    // Continue with existing config
                    Some(AppAction::StartWithConfig)
                } else {
                    // New configuration
                    self.state = AppState::SelectDevice;
                    None
                }
            }
            KeyAction::Quit | KeyAction::Cancel => {
                self.state = AppState::Quit;
                None
            }
            _ => None,
        }
    }

    fn handle_select_device(&mut self, action: KeyAction) -> Option<AppAction> {
        match action {
            KeyAction::Up => {
                if self.selected_device_idx > 0 {
                    self.selected_device_idx -= 1;
                }
                None
            }
            KeyAction::Down => {
                if self.selected_device_idx < self.devices.len().saturating_sub(1) {
                    self.selected_device_idx += 1;
                }
                None
            }
            KeyAction::Select => {
                if let Some(device) = self.devices.get(self.selected_device_idx).cloned() {
                    let channels = device.channels as usize;
                    self.current_device = Some(device);
                    self.channel_selected = vec![false; channels];
                    self.channel_cursor = 0;
                    self.channel_levels = vec![0.0; channels];
                    self.state = AppState::SelectChannels;
                    Some(AppAction::StartPreview)
                } else {
                    None
                }
            }
            KeyAction::Quit | KeyAction::Cancel => {
                self.state = AppState::Quit;
                None
            }
            _ => None,
        }
    }

    fn handle_select_channels(&mut self, action: KeyAction) -> Option<AppAction> {
        let channel_count = self.channel_selected.len();

        match action {
            KeyAction::Up => {
                if self.channel_cursor > 0 {
                    self.channel_cursor -= 1;
                }
                None
            }
            KeyAction::Down => {
                if self.channel_cursor < channel_count.saturating_sub(1) {
                    self.channel_cursor += 1;
                }
                None
            }
            KeyAction::Char(' ') => {
                // Toggle selection
                if self.channel_cursor < self.channel_selected.len() {
                    self.channel_selected[self.channel_cursor] =
                        !self.channel_selected[self.channel_cursor];
                }
                None
            }
            KeyAction::Select => {
                // Confirm selection - at least one channel must be selected
                let selected_count = self.channel_selected.iter().filter(|&&s| s).count();
                if selected_count > 0 {
                    // Prepare name entry
                    self.channel_names = self
                        .channel_selected
                        .iter()
                        .filter_map(
                            |&selected| {
                                if selected {
                                    Some(String::new())
                                } else {
                                    None
                                }
                            },
                        )
                        .collect();
                    self.name_cursor = 0;
                    self.name_input.clear();
                    self.state = AppState::EnterNames;
                }
                None
            }
            KeyAction::Cancel => {
                self.state = AppState::SelectDevice;
                Some(AppAction::StopPreview)
            }
            KeyAction::Quit => {
                self.state = AppState::Quit;
                None
            }
            _ => None,
        }
    }

    fn handle_enter_names(&mut self, action: KeyAction) -> Option<AppAction> {
        match action {
            KeyAction::Char(c) => {
                if self.name_input.len() < 32 {
                    self.name_input.push(c);
                }
                None
            }
            KeyAction::Backspace => {
                self.name_input.pop();
                None
            }
            KeyAction::Select => {
                // Save current name and move to next or finish
                self.channel_names[self.name_cursor] = if self.name_input.is_empty() {
                    // Auto-generate name
                    self.generate_default_name(self.name_cursor)
                } else {
                    self.name_input.clone()
                };

                if self.name_cursor + 1 < self.channel_names.len() {
                    self.name_cursor += 1;
                    self.name_input.clear();
                    None
                } else {
                    // All names entered, save config and start
                    Some(AppAction::SaveAndStart)
                }
            }
            KeyAction::Cancel => {
                if self.name_cursor > 0 {
                    self.name_cursor -= 1;
                    self.name_input = self.channel_names[self.name_cursor].clone();
                } else {
                    self.state = AppState::SelectChannels;
                }
                None
            }
            KeyAction::Quit => {
                self.state = AppState::Quit;
                None
            }
            _ => None,
        }
    }

    fn handle_running(&mut self, action: KeyAction) -> Option<AppAction> {
        match action {
            KeyAction::Quit => {
                self.state = AppState::Quit;
                None
            }
            KeyAction::Restart => Some(AppAction::Restart),
            KeyAction::Setup => {
                self.state = AppState::SelectDevice;
                Some(AppAction::StopCapture)
            }
            _ => None,
        }
    }

    fn handle_error(&mut self, action: KeyAction) -> Option<AppAction> {
        match action {
            KeyAction::Char('r') | KeyAction::Restart => Some(AppAction::Retry),
            KeyAction::Quit | KeyAction::Cancel => {
                self.state = AppState::Quit;
                None
            }
            _ => None,
        }
    }

    fn generate_default_name(&self, name_index: usize) -> String {
        let device_name = self
            .current_device
            .as_ref()
            .map(|d| d.name.as_str())
            .unwrap_or("Device");

        // Find which channel this name_index corresponds to
        let channel_num = self
            .channel_selected
            .iter()
            .enumerate()
            .filter(|(_, &selected)| selected)
            .nth(name_index)
            .map(|(i, _)| i)
            .unwrap_or(name_index);

        format!("{} Ch{}", device_name, channel_num)
    }

    fn selected_channels(&self) -> Vec<usize> {
        self.channel_selected
            .iter()
            .enumerate()
            .filter_map(|(i, &selected)| if selected { Some(i) } else { None })
            .collect()
    }

    fn selected_count(&self) -> usize {
        self.channel_selected.iter().filter(|&&s| s).count()
    }

    fn build_virtual_mics(&self) -> Vec<VirtualMicConfig> {
        let selected_channels = self.selected_channels();

        self.channel_names
            .iter()
            .zip(selected_channels.iter())
            .map(|(name, &channel)| VirtualMicConfig {
                name: name.clone(),
                channel: channel as u32,
            })
            .collect()
    }

    fn update_levels(&mut self, levels: &[f32]) {
        match &self.state {
            AppState::SelectChannels => {
                for (i, level) in levels.iter().enumerate() {
                    if i < self.channel_levels.len() {
                        self.channel_levels[i] = self.channel_levels[i].max(*level) * 0.92;
                    }
                }
            }
            AppState::Running => {
                for (i, level) in levels.iter().enumerate() {
                    if i < self.dashboard_levels.len() {
                        let current = self.dashboard_levels[i];
                        self.dashboard_levels[i] = if *level > current {
                            *level
                        } else {
                            current * 0.92
                        };
                    }
                }
            }
            _ => {}
        }
    }

    fn start_running(&mut self) {
        let selected_channels = self.selected_channels();

        self.dashboard_levels = vec![0.0; selected_channels.len()];
        self.dashboard_labels = self.channel_names.clone();
        self.start_time = Some(Instant::now());
        self.state = AppState::Running;
    }

    fn start_with_existing_config(&mut self) {
        self.dashboard_levels = vec![0.0; self.config.virtual_mics.len()];
        self.dashboard_labels = self
            .config
            .virtual_mics
            .iter()
            .map(|m| format!("{} [Ch {}]", m.name, m.channel))
            .collect();
        self.start_time = Some(Instant::now());
        self.state = AppState::Running;
    }

    fn set_error(&mut self, message: String) {
        self.state = AppState::Error(message);
    }

    fn uptime(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}

enum AppAction {
    StartWithConfig,
    StartPreview,
    StopPreview,
    SaveAndStart,
    StopCapture,
    Restart,
    Retry,
}

pub fn execute(device_name: Option<String>) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let devices = list_input_devices()?;

    if devices.is_empty() {
        anyhow::bail!("No input devices found");
    }

    // Setup Ctrl+C handler
    ctrlc::set_handler(|| {
        CLEANUP_REQUESTED.store(true, Ordering::SeqCst);
    })
    .ok();

    // Initial cleanup: remove orphan devices from driver
    cleanup_orphan_devices(&config);

    let mut app = App::new(devices.clone(), config);

    // If device specified via CLI, skip to that device
    if let Some(ref name) = device_name {
        if let Some(idx) = devices
            .iter()
            .position(|d| d.name.to_lowercase().contains(&name.to_lowercase()))
        {
            app.selected_device_idx = idx;
            app.state = AppState::SelectDevice;
        }
    }

    let mut terminal = Terminal::new()?;
    let events = EventHandler::new(Duration::from_millis(50));

    let mut audio_capture: Option<AudioCapture> = None;
    let mut driver_client: Option<DriverClient> = None;

    loop {
        // Check if cleanup was requested via signal
        if CLEANUP_REQUESTED.load(Ordering::SeqCst) {
            app.state = AppState::Quit;
        }
        // Draw UI
        terminal.draw(|frame| {
            draw_ui(frame, &app);
        })?;

        // Handle events
        match events.next()? {
            AppEvent::Key(key) => {
                // Use text input mode when entering names (allows all chars like 's', 'n', etc.)
                let action = if app.state == AppState::EnterNames {
                    KeyAction::from_text_input(key)
                } else {
                    KeyAction::from_navigation(key)
                };
                if let Some(app_action) = app.handle_key(action) {
                    match app_action {
                        AppAction::StartWithConfig => {
                            // Start with existing config
                            if let Some(ref _device_name) = app.config.device.name {
                                match start_capture_from_config(&app.config, &devices) {
                                    Ok((capture, client)) => {
                                        app.start_with_existing_config();
                                        audio_capture = Some(capture);
                                        driver_client = client;
                                    }
                                    Err(e) => {
                                        app.set_error(format!("Failed to start: {}", e));
                                    }
                                }
                            }
                        }
                        AppAction::StartPreview => {
                            // Start audio preview for channel selection
                            if let Some(device) = &app.current_device {
                                if let Ok(cpal_device) = get_cpal_device(&device.name) {
                                    if let Ok(buffer) = SharedAudioBuffer::open(
                                        device.channels as u32,
                                        device.sample_rate,
                                    ) {
                                        if let Ok(capture) =
                                            AudioCapture::start(&cpal_device, buffer)
                                        {
                                            audio_capture = Some(capture);
                                        }
                                    }
                                }
                            }
                        }
                        AppAction::StopPreview | AppAction::StopCapture => {
                            drop(audio_capture.take());
                            drop(driver_client.take());
                        }
                        AppAction::SaveAndStart => {
                            // Build and save config
                            let virtual_mics = app.build_virtual_mics();
                            let mut new_config = Config::default();

                            if let Some(device) = &app.current_device {
                                new_config.device.name = Some(device.name.clone());
                                new_config.device.sample_rate = device.sample_rate;
                            }
                            new_config.virtual_mics = virtual_mics;

                            if let Err(e) = new_config.save() {
                                tracing::warn!("Failed to save config: {}", e);
                            }

                            // Sync driver devices: remove old ones, add new ones
                            if DriverClient::is_driver_available() {
                                let mut client = DriverClient::new();
                                if client.connect().is_ok() {
                                    // Build expected device list
                                    let expected: Vec<DeviceInfo> = new_config
                                        .virtual_mics
                                        .iter()
                                        .map(|m| DeviceInfo {
                                            name: m.name.clone(),
                                            channel: m.channel,
                                        })
                                        .collect();

                                    // Sync: removes orphans, adds missing
                                    if let Err(e) = client.sync_devices(&expected) {
                                        tracing::warn!("Failed to sync devices: {}", e);
                                    }
                                    driver_client = Some(client);
                                }
                            }

                            app.config = new_config;
                            app.start_running();
                        }
                        AppAction::Restart | AppAction::Retry => {
                            drop(audio_capture.take());
                            drop(driver_client.take());

                            match start_capture_from_config(&app.config, &devices) {
                                Ok((capture, client)) => {
                                    app.start_with_existing_config();
                                    audio_capture = Some(capture);
                                    driver_client = client;
                                }
                                Err(e) => {
                                    app.set_error(format!("Failed to restart: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            AppEvent::Tick => {
                // Update audio levels and buffer usage from capture
                if let Some(ref capture) = audio_capture {
                    while let Ok(levels) = capture.peak_receiver().try_recv() {
                        app.update_levels(&levels);
                    }

                    // Update buffer usage from atomic write_pos
                    let write_pos = capture.write_pos() as f32;
                    let capacity = RING_BUFFER_FRAMES as f32;
                    app.buffer_usage = (write_pos % capacity) / capacity;
                }
            }
            AppEvent::Resize(_, _) => {}
        }

        if app.state == AppState::Quit {
            break;
        }
    }

    // Cleanup: remove all virtual devices from driver on exit
    drop(audio_capture);
    drop(driver_client);
    cleanup_all_devices();

    Ok(())
}

/// Remove orphan devices that exist in driver but not in config
fn cleanup_orphan_devices(config: &Config) {
    if !DriverClient::is_driver_available() {
        return;
    }

    let mut client = DriverClient::new();
    if client.connect().is_err() {
        return;
    }

    // Build expected device list from config
    let expected: Vec<DeviceInfo> = config
        .virtual_mics
        .iter()
        .map(|m| DeviceInfo {
            name: m.name.clone(),
            channel: m.channel,
        })
        .collect();

    if let Err(e) = client.sync_devices(&expected) {
        tracing::warn!("Failed to sync devices: {}", e);
    }
}

/// Remove all virtual devices from driver (called on exit)
fn cleanup_all_devices() {
    if !DriverClient::is_driver_available() {
        return;
    }

    let mut client = DriverClient::new();
    if client.connect().is_err() {
        return;
    }

    match client.remove_all_devices() {
        Ok(count) => {
            if count > 0 {
                tracing::info!("Cleaned up {} virtual devices on exit", count);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to cleanup devices: {}", e);
        }
    }
}

fn start_capture_from_config(
    config: &Config,
    devices: &[AudioDevice],
) -> Result<(AudioCapture, Option<DriverClient>)> {
    let device_name = config
        .device
        .name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No device configured"))?;

    let device = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains(&device_name.to_lowercase()))
        .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_name))?;

    let buffer = SharedAudioBuffer::open(device.channels as u32, device.sample_rate)?;
    let cpal_device = get_cpal_device(&device.name)?;
    let capture = AudioCapture::start(&cpal_device, buffer)?;

    let mut driver_client = None;
    if DriverClient::is_driver_available() {
        let mut client = DriverClient::new();
        if client.connect().is_ok() {
            for mic in &config.virtual_mics {
                let _ = client.add_device(&mic.name, mic.channel);
            }
            driver_client = Some(client);
        }
    }

    Ok((capture, driver_client))
}

// ============ UI Drawing ============

fn draw_ui(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    match &app.state {
        AppState::AskAction => draw_ask_action(frame, app),
        AppState::SelectDevice => draw_select_device(frame, app),
        AppState::SelectChannels => draw_select_channels(frame, app),
        AppState::EnterNames => draw_enter_names(frame, app),
        AppState::Running => draw_running(frame, app),
        AppState::Error(msg) => draw_error(frame, msg),
        AppState::Quit => {}
    }
}

fn draw_ask_action(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    // Title
    let title = Block::default()
        .title(" duomic ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(title, chunks[0]);

    // Content
    let content = Block::default()
        .title(" Current Configuration ")
        .borders(Borders::ALL);
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);

    let device_name = app.config.device.name.as_deref().unwrap_or("?");
    let mic_names: Vec<_> = app
        .config
        .virtual_mics
        .iter()
        .map(|m| m.name.as_str())
        .collect();

    let mut lines = vec![
        Line::from(format!("  Device: {}", device_name)),
        Line::from(format!("  Microphones: {}", mic_names.join(", "))),
        Line::from(""),
    ];

    // Options
    let options = [
        ("Start with current settings", 0),
        ("Configure new device", 1),
    ];

    for (label, idx) in options {
        let prefix = if app.action_cursor == idx {
            "→ ●"
        } else {
            "  ○"
        };
        let style = if app.action_cursor == idx {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::styled(format!("  {} {}", prefix, label), style));
    }

    frame.render_widget(Paragraph::new(lines), inner);

    // Help
    let help = HelpBar::new(&[("↑/↓", "Select"), ("Enter", "Confirm"), ("q", "Quit")]);
    frame.render_widget(help, chunks[2]);
}

fn draw_select_device(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Block::default()
        .title(" duomic - Select Device ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(title, chunks[0]);

    let content = Block::default()
        .title(" Input Devices ")
        .borders(Borders::ALL);
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);

    let device_list = DeviceList::new(&app.devices, app.selected_device_idx);
    frame.render_widget(device_list, inner);

    let help = HelpBar::new(&[("↑/↓", "Select"), ("Enter", "Confirm"), ("q", "Quit")]);
    frame.render_widget(help, chunks[2]);
}

fn draw_select_channels(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let device_name = app
        .current_device
        .as_ref()
        .map(|d| d.name.as_str())
        .unwrap_or("?");

    let title = Block::default()
        .title(format!(" {} - Channel Selection ", device_name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(title, chunks[0]);

    // Channel list with multi-select
    let content = Block::default()
        .title(" Select Channels (Space to toggle) ")
        .borders(Borders::ALL);
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);

    let channel_names = [
        "Left",
        "Right",
        "Center",
        "LFE",
        "Rear Left",
        "Rear Right",
        "Side Left",
        "Side Right",
    ];

    for (i, &selected) in app.channel_selected.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }

        let is_cursor = i == app.channel_cursor;
        let checkbox = if selected { "[✓]" } else { "[ ]" };
        let arrow = if is_cursor { "→" } else { " " };
        let ch_name = channel_names.get(i).unwrap_or(&"Channel");
        let level = app.channel_levels.get(i).copied().unwrap_or(0.0);

        // Build line
        let label = format!("{} {} Channel {} ({})", arrow, checkbox, i, ch_name);
        let style = if is_cursor {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        let y = inner.y + i as u16;
        frame.buffer_mut().set_string(inner.x, y, &label, style);

        // Level meter
        let meter_x = inner.x + 28;
        let meter_width = inner.width.saturating_sub(36).min(20);
        if meter_width > 5 {
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
                frame.buffer_mut().set_string(meter_x + j, y, symbol, style);
            }
        }
    }

    // Selection count
    let count_block = Block::default().borders(Borders::ALL);
    let count_inner = count_block.inner(chunks[2]);
    frame.render_widget(count_block, chunks[2]);

    let count_text = format!("Selected: {} channels", app.selected_count());
    let count_style = if app.selected_count() > 0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    frame.render_widget(
        Paragraph::new(count_text).style(count_style).centered(),
        count_inner,
    );

    let help = HelpBar::new(&[
        ("↑/↓", "Navigate"),
        ("Space", "Toggle"),
        ("Enter", "Confirm"),
        ("Esc", "Back"),
    ]);
    frame.render_widget(help, chunks[3]);
}

fn draw_enter_names(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let selected_channels = app.selected_channels();
    let current_channel = selected_channels.get(app.name_cursor).copied().unwrap_or(0);

    let title = Block::default()
        .title(format!(" Name for Channel {} (optional) ", current_channel))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(title, chunks[0]);

    let content = Block::default()
        .title(format!(
            " {}/{} ",
            app.name_cursor + 1,
            app.channel_names.len()
        ))
        .borders(Borders::ALL);
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);

    let default_name = app.generate_default_name(app.name_cursor);

    let lines = vec![
        Line::from(""),
        Line::from(format!("  > {}█", app.name_input)).style(Style::default().fg(Color::White)),
        Line::from(""),
        Line::from(format!("  Leave empty for: \"{}\"", default_name))
            .style(Style::default().fg(Color::DarkGray)),
    ];

    frame.render_widget(Paragraph::new(lines), inner);

    let help = HelpBar::new(&[("Enter", "Confirm"), ("Esc", "Back")]);
    frame.render_widget(help, chunks[2]);
}

fn draw_running(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

    let device_name = app.config.device.name.as_deref().unwrap_or("?");
    let sample_rate = app.config.device.sample_rate / 1000;

    let header = Block::default()
        .title(format!(
            " duomic | {} @ {}kHz | ● Running ",
            device_name, sample_rate
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    frame.render_widget(header, chunks[0]);

    // Level meters
    let meters = Block::default()
        .title(" Virtual Microphones ")
        .borders(Borders::ALL);
    let meters_inner = meters.inner(chunks[1]);
    frame.render_widget(meters, chunks[1]);

    for (i, (level, label)) in app
        .dashboard_levels
        .iter()
        .zip(app.dashboard_labels.iter())
        .enumerate()
    {
        if i as u16 >= meters_inner.height {
            break;
        }

        let row = Rect {
            x: meters_inner.x,
            y: meters_inner.y + i as u16,
            width: meters_inner.width,
            height: 1,
        };

        let meter = LevelMeter::new(*level).label(label);
        frame.render_widget(meter, row);
    }

    // Stats
    let uptime = app.uptime();
    let hours = uptime.as_secs() / 3600;
    let minutes = (uptime.as_secs() % 3600) / 60;
    let seconds = uptime.as_secs() % 60;

    let stats = Block::default()
        .title(format!(
            " Latency: 21ms | Buffer: {:.0}% | Duration: {:02}:{:02}:{:02} ",
            app.buffer_usage * 100.0,
            hours,
            minutes,
            seconds
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(stats, chunks[2]);

    let help = HelpBar::new(&[("q", "Quit"), ("r", "Restart"), ("s", "Setup")]);
    frame.render_widget(help, chunks[3]);
}

fn draw_error(frame: &mut Frame, message: &str) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Block::default()
        .title(" ⚠ Error ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    frame.render_widget(title, chunks[0]);

    let content = Block::default().borders(Borders::ALL);
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);

    let lines = vec![
        Line::from(message).style(Style::default().fg(Color::Red)),
        Line::from(""),
        Line::from("Suggestions:"),
        Line::from("  1. Make sure the device is connected"),
        Line::from("  2. Restart the driver: sudo killall coreaudiod"),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    let help = HelpBar::new(&[("r", "Retry"), ("q", "Quit")]);
    frame.render_widget(help, chunks[2]);
}
