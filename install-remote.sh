#!/bin/bash
# duomic installer - downloads and installs the latest release
# Usage: curl -fsSL https://raw.githubusercontent.com/syy/duomic/main/install-remote.sh | bash

set -e

REPO="syy/duomic"
BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BOLD}duomic installer${NC}"
echo ""

# Check macOS version
OS_VERSION=$(sw_vers -productVersion)
MAJOR_VERSION=$(echo "$OS_VERSION" | cut -d. -f1)

if [ "$MAJOR_VERSION" -lt 12 ]; then
    echo -e "${RED}Error: duomic requires macOS 12 (Monterey) or later.${NC}"
    echo "Current version: $OS_VERSION"
    exit 1
fi

# Get latest release version
echo "Fetching latest release..."
LATEST_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')

if [ -z "$LATEST_VERSION" ]; then
    echo -e "${RED}Error: Could not determine latest version.${NC}"
    exit 1
fi

echo "Latest version: v${LATEST_VERSION}"

# Download URL
PKG_URL="https://github.com/${REPO}/releases/download/v${LATEST_VERSION}/duomic-${LATEST_VERSION}.pkg"
PKG_FILE="/tmp/duo-mic-${LATEST_VERSION}.pkg"

# Download
echo "Downloading duomic-${LATEST_VERSION}.pkg..."
curl -fsSL -o "$PKG_FILE" "$PKG_URL"

if [ ! -f "$PKG_FILE" ]; then
    echo -e "${RED}Error: Download failed.${NC}"
    exit 1
fi

# Install
echo ""
echo "Installing (requires sudo)..."
sudo installer -pkg "$PKG_FILE" -target /

# Cleanup
rm -f "$PKG_FILE"

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Get started:"
echo "  duomic              # Interactive TUI"
echo "  duomic status       # Check driver status"
echo ""
echo "Note: If audio apps don't see the virtual mics, restart them or run:"
echo "  sudo killall coreaudiod"
