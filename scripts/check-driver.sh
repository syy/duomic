#!/bin/bash
set -e

SCRIPT_DIR="$(dirname "$0")"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DRIVER_DIR="$PROJECT_ROOT/Driver/duomicDriver"
BUILD_DIR="$DRIVER_DIR/build"

echo "=========================================="
echo "  duomic Driver Build Script"
echo "=========================================="
echo ""

# Check submodule
echo "▶ [1/4] Checking libASPL submodule..."
if [ ! -f "$PROJECT_ROOT/Driver/libASPL/CMakeLists.txt" ]; then
    echo "✗ libASPL submodule not initialized"
    echo "  Run: git submodule update --init --recursive"
    exit 1
fi
echo "✓ Submodule OK"
echo ""

# Clean build directory
echo "▶ [2/4] Preparing build directory..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
echo "✓ Build directory ready"
echo ""

# Configure
echo "▶ [3/4] Running CMake configure..."
cd "$BUILD_DIR"
if cmake .. \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 \
    -DCMAKE_OSX_ARCHITECTURES="x86_64;arm64" \
    -DCMAKE_BUILD_TYPE=Release; then
    echo "✓ CMake configure OK"
else
    echo "✗ CMake configure failed"
    exit 1
fi
echo ""

# Build
echo "▶ [4/4] Building driver (Universal Binary)..."
if make -j4; then
    echo "✓ Build OK"
else
    echo "✗ Build failed"
    exit 1
fi
echo ""

# Verify universal binary
echo "▶ Verifying Universal Binary..."
BINARY="$BUILD_DIR/duomicDriver.driver/Contents/MacOS/duomicDriver"
if [ -f "$BINARY" ]; then
    echo "Binary info:"
    lipo -info "$BINARY"
    echo ""
    file "$BINARY"
else
    echo "✗ Binary not found at $BINARY"
    exit 1
fi
echo ""

echo "=========================================="
echo "  ✓ Driver build successful!"
echo "=========================================="
echo ""
echo "Driver location: $BUILD_DIR/duomicDriver.driver"
echo ""
echo "To install:"
echo "  sudo $PROJECT_ROOT/install.sh"
