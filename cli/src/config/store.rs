use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub device: DeviceConfig,

    #[serde(default)]
    pub virtual_mics: Vec<VirtualMicConfig>,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceConfig {
    pub name: Option<String>,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
}

fn default_sample_rate() -> u32 {
    48000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualMicConfig {
    pub name: String,
    pub channel: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_true")]
    pub color: bool,
    #[serde(default = "default_meter_style")]
    pub meter_style: MeterStyle,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color: true,
            meter_style: MeterStyle::Gradient,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_meter_style() -> MeterStyle {
    MeterStyle::Gradient
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MeterStyle {
    #[default]
    Gradient,
    Mono,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Get the config file path (~/.config/duomic/config.toml)
    /// Uses XDG standard on all platforms
    pub fn path() -> Result<PathBuf> {
        // Use XDG standard: ~/.config/duomic/config.toml
        let home = dirs::home_dir().context("Could not determine home directory")?;

        let config_dir = home.join(".config").join("duomic");
        Ok(config_dir.join("config.toml"))
    }

    /// Load config from file, or return default if not exists
    pub fn load() -> Result<Self> {
        let path = Self::path()?;

        if !path.exists() {
            tracing::debug!("Config file not found, using defaults");
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {:?}", path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {:?}", path))?;

        tracing::info!("Loaded config from {:?}", path);
        Ok(config)
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;

        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        tracing::info!("Saved config to {:?}", path);
        Ok(())
    }

    /// Add a virtual microphone configuration
    pub fn add_virtual_mic(&mut self, name: String, channel: u32) {
        // Remove existing with same name
        self.virtual_mics.retain(|m| m.name != name);
        self.virtual_mics.push(VirtualMicConfig { name, channel });
    }

    /// Remove a virtual microphone configuration
    pub fn remove_virtual_mic(&mut self, name: &str) -> bool {
        let len_before = self.virtual_mics.len();
        self.virtual_mics.retain(|m| m.name != name);
        self.virtual_mics.len() < len_before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.virtual_mics.is_empty());
        assert!(config.ui.color);
        assert_eq!(config.ui.meter_style, MeterStyle::Gradient);
    }

    #[test]
    fn test_config_serialization() {
        let mut config = Config::default();
        config.device.name = Some("Test Device".to_string());
        config.add_virtual_mic("Test Mic".to_string(), 0);

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.device.name, Some("Test Device".to_string()));
        assert_eq!(deserialized.virtual_mics.len(), 1);
        assert_eq!(deserialized.virtual_mics[0].name, "Test Mic");
    }
}
