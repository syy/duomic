#!/bin/bash
# duomic driver installer

set -e

DRIVER_NAME="duomicDriver.driver"
BUILD_PATH="Driver/duomicDriver/build/$DRIVER_NAME"
INSTALL_PATH="/Library/Audio/Plug-Ins/HAL/$DRIVER_NAME"

echo "=== duomic Driver Installer ==="

# Check if build exists
if [ ! -d "$BUILD_PATH" ]; then
    echo "Error: Driver not built. Run 'cd Driver/duomicDriver/build && cmake .. && make' first."
    exit 1
fi

# Remove old driver
if [ -d "$INSTALL_PATH" ]; then
    echo "Removing old driver..."
    sudo rm -rf "$INSTALL_PATH"
fi

# Install new driver
echo "Installing driver..."
sudo cp -r "$BUILD_PATH" "$INSTALL_PATH"

# Fix permissions
echo "Fixing permissions..."
sudo chmod -R a+rX "$INSTALL_PATH"

# Restart coreaudiod
echo "Restarting coreaudiod..."
sudo killall coreaudiod

# Wait a moment
sleep 1

# Verify installation
echo ""
echo "Checking installed devices..."
system_profiler SPAudioDataType 2>/dev/null | grep -A3 "duomic" || echo "Warning: duomic devices not found!"

echo ""
echo "Done!"
