use anyhow::Result;
use std::io::Write;

use crate::config::Config;
use crate::ipc::DriverClient;

pub fn execute() -> Result<()> {
    let config = Config::load().unwrap_or_default();

    println!();
    println!("╭─────────────────────────────────────────╮");
    println!("│           duomic status                 │");
    println!("╰─────────────────────────────────────────╯");
    println!();

    // Check driver status
    print!("Driver status: ");
    std::io::stdout().flush()?;

    let driver_ok = if DriverClient::is_driver_available() {
        let mut client = DriverClient::new();
        if client.connect().is_ok() {
            match client.ping() {
                Ok(true) => {
                    println!("\x1b[32m● Connected\x1b[0m");
                    true
                }
                _ => {
                    println!("\x1b[33m○ Socket exists but not responding\x1b[0m");
                    false
                }
            }
        } else {
            println!("\x1b[31m○ Failed to connect\x1b[0m");
            false
        }
    } else {
        println!("\x1b[31m○ Not running\x1b[0m");
        println!("         (socket not found at /tmp/duomic.sock)");
        false
    };

    println!();

    // Show config info
    println!("Configuration:");
    match Config::path() {
        Ok(path) => {
            if path.exists() {
                println!("  Path: \x1b[36m{}\x1b[0m", path.display());
            } else {
                println!("  Path: \x1b[33m{} (not created)\x1b[0m", path.display());
            }
        }
        Err(_) => {
            println!("  Path: \x1b[31mCould not determine\x1b[0m");
        }
    }

    if let Some(ref device) = config.device.name {
        println!("  Device: {}", device);
        println!("  Sample rate: {} Hz", config.device.sample_rate);
    } else {
        println!("  Device: \x1b[33m(not configured)\x1b[0m");
    }

    println!();

    // Show virtual microphones
    println!("Virtual Microphones:");

    if driver_ok {
        // Get live list from driver
        let mut client = DriverClient::new();
        if client.connect().is_ok() {
            match client.list_devices() {
                Ok(devices) if !devices.is_empty() => {
                    for device in devices {
                        println!(
                            "  \x1b[32m●\x1b[0m {} \x1b[90m(channel {})\x1b[0m",
                            device.name, device.channel
                        );
                    }
                }
                Ok(_) => {
                    println!("  \x1b[33m(no active devices)\x1b[0m");
                }
                Err(e) => {
                    println!("  \x1b[31mFailed to query: {}\x1b[0m", e);
                }
            }
        }
    } else if !config.virtual_mics.is_empty() {
        // Show from config (offline mode)
        println!("  \x1b[33m(from config, driver not running)\x1b[0m");
        for mic in &config.virtual_mics {
            println!(
                "  \x1b[90m○\x1b[0m {} \x1b[90m(channel {})\x1b[0m",
                mic.name, mic.channel
            );
        }
    } else {
        println!("  \x1b[33m(none configured)\x1b[0m");
    }

    println!();

    // Quick help
    if !driver_ok {
        println!("To start the driver:");
        println!("  1. Ensure driver is installed: \x1b[36msudo ./install.sh\x1b[0m");
        println!("  2. Restart CoreAudio: \x1b[36msudo killall coreaudiod\x1b[0m");
        println!();
    }

    if config.virtual_mics.is_empty() {
        println!("To configure virtual microphones:");
        println!("  Run: \x1b[36mduomic setup\x1b[0m");
        println!();
    }

    Ok(())
}
