use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/duomic.sock";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Driver IPC client for sending commands via Unix socket
pub struct DriverClient {
    stream: Option<UnixStream>,
}

impl DriverClient {
    /// Create a new driver client (not connected yet)
    pub fn new() -> Self {
        Self { stream: None }
    }

    /// Check if driver socket exists
    pub fn is_driver_available() -> bool {
        Path::new(SOCKET_PATH).exists()
    }

    /// Connect to the driver socket
    pub fn connect(&mut self) -> Result<()> {
        let stream =
            UnixStream::connect(SOCKET_PATH).context("Failed to connect to driver socket")?;

        stream
            .set_read_timeout(Some(TIMEOUT))
            .context("Failed to set read timeout")?;
        stream
            .set_write_timeout(Some(TIMEOUT))
            .context("Failed to set write timeout")?;

        self.stream = Some(stream);
        tracing::debug!("Connected to driver socket at {}", SOCKET_PATH);
        Ok(())
    }

    /// Disconnect from the driver
    pub fn disconnect(&mut self) {
        self.stream = None;
        tracing::debug!("Disconnected from driver socket");
    }

    /// Send a command and receive response
    fn send_command(&mut self, command: &str) -> Result<String> {
        let stream = self.stream.as_mut().context("Not connected to driver")?;

        // Send command with newline terminator
        let command_with_newline = format!("{}\n", command);
        stream
            .write_all(command_with_newline.as_bytes())
            .context("Failed to send command to driver")?;
        stream.flush().context("Failed to flush command")?;

        tracing::debug!("Sent command: {}", command);

        // Read response
        let mut buffer = [0u8; 1024];
        let n = stream
            .read(&mut buffer)
            .context("Failed to read response from driver")?;

        let response = String::from_utf8_lossy(&buffer[..n]).to_string();
        tracing::debug!("Received response: {}", response);

        Ok(response)
    }

    /// Parse response into Result
    fn parse_response(response: &str) -> Result<String> {
        if response.starts_with("OK") {
            // OK or OK:message
            let message = response
                .strip_prefix("OK:")
                .or_else(|| response.strip_prefix("OK"))
                .unwrap_or("")
                .trim()
                .to_string();
            Ok(message)
        } else if response.starts_with("ERROR:") {
            let error = response.strip_prefix("ERROR:").unwrap_or("Unknown error");
            bail!("{}", error.trim())
        } else if response == "PONG" {
            Ok("PONG".to_string())
        } else {
            // Treat as success with raw response
            Ok(response.to_string())
        }
    }

    /// Ping the driver to check if it's responsive
    pub fn ping(&mut self) -> Result<bool> {
        // Driver closes connection after each command, so reconnect
        self.connect()?;
        let response = self.send_command("PING")?;
        Ok(response.trim() == "PONG")
    }

    /// Add a virtual device (reconnects for each command)
    pub fn add_device(&mut self, name: &str, channel: u32) -> Result<()> {
        // Driver closes connection after each command, so reconnect
        self.connect()?;
        let command = format!("ADD {}:{}", name, channel);
        let response = self.send_command(&command)?;
        Self::parse_response(&response)?;
        tracing::info!("Added virtual device: {} (channel {})", name, channel);
        Ok(())
    }

    /// Remove a virtual device (reconnects for each command)
    pub fn remove_device(&mut self, name: &str) -> Result<()> {
        // Driver closes connection after each command, so reconnect
        self.connect()?;
        let command = format!("REMOVE {}", name);
        let response = self.send_command(&command)?;
        Self::parse_response(&response)?;
        tracing::info!("Removed virtual device: {}", name);
        Ok(())
    }

    /// List active virtual devices (reconnects for each command)
    pub fn list_devices(&mut self) -> Result<Vec<DeviceInfo>> {
        // Driver closes connection after each command, so reconnect
        self.connect()?;
        let response = self.send_command("LIST")?;
        let message = Self::parse_response(&response)?;

        if message.is_empty() || message == "OK" {
            return Ok(Vec::new());
        }

        // Parse device list - supports both formats:
        // - Newline separated: "name1:channel1\nname2:channel2"
        // - Comma separated: "name1:channel1,name2:channel2"
        let devices = message
            .split([',', '\n'])
            .filter_map(|entry| {
                let entry = entry.trim();
                if entry.is_empty() {
                    return None;
                }
                let parts: Vec<&str> = entry.split(':').collect();
                if parts.len() >= 2 {
                    Some(DeviceInfo {
                        name: parts[0].to_string(),
                        channel: parts[1].parse().unwrap_or(0),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(devices)
    }

    /// Remove all virtual devices from driver
    pub fn remove_all_devices(&mut self) -> Result<usize> {
        let devices = self.list_devices()?;
        let count = devices.len();

        for device in devices {
            if let Err(e) = self.remove_device(&device.name) {
                tracing::warn!("Failed to remove device {}: {}", device.name, e);
            }
        }

        tracing::info!("Removed {} virtual devices", count);
        Ok(count)
    }

    /// Sync driver devices with expected list
    /// Removes devices not in expected list, adds missing ones
    pub fn sync_devices(&mut self, expected: &[DeviceInfo]) -> Result<()> {
        let current = self.list_devices()?;

        // Find devices to remove (in driver but not in expected)
        for device in &current {
            let should_exist = expected.iter().any(|e| e.name == device.name);
            if !should_exist {
                tracing::info!("Removing orphan device: {}", device.name);
                if let Err(e) = self.remove_device(&device.name) {
                    tracing::warn!("Failed to remove orphan {}: {}", device.name, e);
                }
            }
        }

        // Find devices to add (in expected but not in driver)
        for device in expected {
            let exists = current.iter().any(|c| c.name == device.name);
            if !exists {
                tracing::info!("Adding missing device: {}", device.name);
                if let Err(e) = self.add_device(&device.name, device.channel) {
                    tracing::warn!("Failed to add {}: {}", device.name, e);
                }
            }
        }

        Ok(())
    }
}

impl Default for DriverClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a virtual device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub channel: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response_ok() {
        assert!(DriverClient::parse_response("OK").is_ok());
        assert!(DriverClient::parse_response("OK:success").is_ok());
        assert!(DriverClient::parse_response("PONG").is_ok());
    }

    #[test]
    fn test_parse_response_error() {
        let result = DriverClient::parse_response("ERROR:Device not found");
        assert!(result.is_err());
    }
}
