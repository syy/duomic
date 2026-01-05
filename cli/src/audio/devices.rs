use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use std::collections::HashSet;

use crate::ipc::DriverClient;

/// Information about an audio input device
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub index: usize,
}

impl std::fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} channels)", self.name, self.channels)
    }
}

/// Get list of virtual device names from driver
fn get_virtual_device_names() -> HashSet<String> {
    let mut names = HashSet::new();

    // Always filter "duomic" prefix devices
    names.insert("duomic".to_lowercase());

    // Try to get dynamic list from driver
    if DriverClient::is_driver_available() {
        let mut client = DriverClient::new();
        if client.connect().is_ok() {
            if let Ok(devices) = client.list_devices() {
                for device in devices {
                    names.insert(device.name.to_lowercase());
                }
            }
        }
    }

    names
}

/// Check if a device name matches any virtual device
fn is_virtual_device(name: &str, virtual_names: &HashSet<String>) -> bool {
    let name_lower = name.to_lowercase();

    // Check exact match
    if virtual_names.contains(&name_lower) {
        return true;
    }

    // Check if name starts with any virtual device prefix
    for vname in virtual_names {
        if name_lower.starts_with(vname) || vname.starts_with(&name_lower) {
            return true;
        }
        // Also check contains for "duomic" keyword
        if vname == "duomic" && name_lower.contains("duomic") {
            return true;
        }
    }

    false
}

/// Get list of available input devices (excluding our virtual devices)
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    // Get virtual device names to filter out
    let virtual_names = get_virtual_device_names();
    tracing::debug!("Virtual device names to filter: {:?}", virtual_names);

    let input_devices = host
        .input_devices()
        .context("Failed to enumerate input devices")?;

    for (index, device) in input_devices.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        // Skip our own virtual devices
        if is_virtual_device(&name, &virtual_names) {
            tracing::debug!("Filtering out virtual device: {}", name);
            continue;
        }

        // Get default config to determine channels and sample rate
        if let Ok(config) = device.default_input_config() {
            devices.push(AudioDevice {
                name,
                channels: config.channels(),
                sample_rate: config.sample_rate().0,
                index,
            });
        }
    }

    tracing::debug!("Found {} input devices", devices.len());
    Ok(devices)
}

/// Find a device by name (partial match)
pub fn find_device_by_name(name: &str) -> Result<Option<AudioDevice>> {
    let devices = list_input_devices()?;
    let name_lower = name.to_lowercase();

    Ok(devices
        .into_iter()
        .find(|d| d.name.to_lowercase().contains(&name_lower)))
}

/// Get the cpal device by name
pub fn get_cpal_device(name: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    let input_devices = host
        .input_devices()
        .context("Failed to enumerate input devices")?;

    let name_lower = name.to_lowercase();

    for device in input_devices {
        if let Ok(device_name) = device.name() {
            if device_name.to_lowercase().contains(&name_lower) {
                return Ok(device);
            }
        }
    }

    anyhow::bail!("Device not found: {}", name)
}

/// Get default input device
pub fn get_default_input_device() -> Result<cpal::Device> {
    let host = cpal::default_host();
    host.default_input_device()
        .context("No default input device available")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        // This test may fail in CI without audio devices
        let result = list_input_devices();
        // Just check it doesn't panic
        let _ = result;
    }
}
