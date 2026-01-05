#!/bin/bash
set -e

echo "=== duomic Uninstaller ==="
echo ""

# Check for root
if [ "$EUID" -ne 0 ]; then
    echo "Please run with sudo: sudo ./uninstall.sh"
    exit 1
fi

# Stop any running duomic processes
echo "Stopping duomic processes..."
pkill -f duomic 2>/dev/null || true

# Remove driver
DRIVER_PATH="/Library/Audio/Plug-Ins/HAL/duomicDriver.driver"
if [ -d "$DRIVER_PATH" ]; then
    echo "Removing driver..."
    rm -rf "$DRIVER_PATH"
else
    echo "Driver not found (already removed?)"
fi

# Remove CLI
CLI_PATH="/usr/local/bin/duomic"
if [ -f "$CLI_PATH" ]; then
    echo "Removing CLI..."
    rm -f "$CLI_PATH"
else
    echo "CLI not found (already removed?)"
fi

# Remove IPC files
echo "Cleaning up IPC files..."
rm -f /tmp/duomic.sock 2>/dev/null || true
rm -f /tmp/duomic_audio 2>/dev/null || true

# Ask about config
CONFIG_PATH="$HOME/.config/duomic"
if [ -d "$CONFIG_PATH" ]; then
    read -p "Remove configuration files? ($CONFIG_PATH) [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$CONFIG_PATH"
        echo "Configuration removed."
    else
        echo "Configuration kept."
    fi
fi

# Restart coreaudiod
echo "Restarting audio service..."
killall coreaudiod 2>/dev/null || true

echo ""
echo "duomic has been uninstalled successfully."
echo ""
echo "Note: If you installed via Homebrew, use: brew uninstall --cask duomic"
