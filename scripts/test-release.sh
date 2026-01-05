#!/bin/bash
set -e

SCRIPT_DIR="$(dirname "$0")"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION="${1:-0.1.0}"

echo "=========================================="
echo "  duomic Release Test (v$VERSION)"
echo "=========================================="
echo ""

# Create dist directory
DIST_DIR="$PROJECT_ROOT/dist"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# Step 1: Build CLI
echo "▶ [1/4] Building CLI..."
cd "$PROJECT_ROOT/cli"

# Check if we can cross-compile
if rustup target list --installed | grep -q "x86_64-apple-darwin"; then
    echo "  Building x86_64..."
    cargo build --release --target x86_64-apple-darwin
    echo "  Building arm64..."
    cargo build --release --target aarch64-apple-darwin
    echo "  Creating universal binary..."
    lipo -create \
        target/x86_64-apple-darwin/release/duomic \
        target/aarch64-apple-darwin/release/duomic \
        -output "$DIST_DIR/duomic"
else
    echo "  Cross-compile targets not installed. Building native only..."
    cargo build --release
    cp target/release/duomic "$DIST_DIR/duomic"
fi
echo "✓ CLI built"
echo ""

# Step 2: Build Driver
echo "▶ [2/4] Building Driver..."
"$PROJECT_ROOT/scripts/check-driver.sh" > /dev/null 2>&1
cp -R "$PROJECT_ROOT/Driver/duomicDriver/build/duomicDriver.driver" "$DIST_DIR/"
echo "✓ Driver built"
echo ""

# Step 3: Create PKG structure (without signing)
echo "▶ [3/4] Creating PKG structure..."
PKG_ROOT="$DIST_DIR/pkg-root"
mkdir -p "$PKG_ROOT/driver"
mkdir -p "$PKG_ROOT/cli"

cp -R "$DIST_DIR/duomicDriver.driver" "$PKG_ROOT/driver/"
cp "$DIST_DIR/duomic" "$PKG_ROOT/cli/"

# Build component packages
echo "  Building driver package..."
pkgbuild --root "$PKG_ROOT/driver" \
    --identifier com.duomic.driver \
    --version "$VERSION" \
    --install-location /Library/Audio/Plug-Ins/HAL \
    --scripts "$PROJECT_ROOT/scripts/driver" \
    "$DIST_DIR/driver.pkg" > /dev/null

echo "  Building CLI package..."
pkgbuild --root "$PKG_ROOT/cli" \
    --identifier com.duomic.cli \
    --version "$VERSION" \
    --install-location /usr/local/bin \
    "$DIST_DIR/cli.pkg" > /dev/null

# Create distribution
cat > "$DIST_DIR/distribution.xml" << EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>duomic</title>
    <organization>com.duomic</organization>
    <domains enable_localSystem="true"/>
    <options customize="never" require-scripts="true" rootVolumeOnly="true"/>
    <pkg-ref id="com.duomic.driver"/>
    <pkg-ref id="com.duomic.cli"/>
    <choices-outline>
        <line choice="default">
            <line choice="com.duomic.driver"/>
            <line choice="com.duomic.cli"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="com.duomic.driver" visible="false">
        <pkg-ref id="com.duomic.driver"/>
    </choice>
    <choice id="com.duomic.cli" visible="false">
        <pkg-ref id="com.duomic.cli"/>
    </choice>
    <pkg-ref id="com.duomic.driver" version="$VERSION">driver.pkg</pkg-ref>
    <pkg-ref id="com.duomic.cli" version="$VERSION">cli.pkg</pkg-ref>
</installer-gui-script>
EOF

echo "  Building product archive..."
productbuild --distribution "$DIST_DIR/distribution.xml" \
    --package-path "$DIST_DIR" \
    "$DIST_DIR/duomic-$VERSION-unsigned.pkg" > /dev/null

echo "✓ PKG created (unsigned)"
echo ""

# Step 4: Verify
echo "▶ [4/4] Verifying..."
echo ""
echo "CLI binary:"
file "$DIST_DIR/duomic"
lipo -info "$DIST_DIR/duomic" 2>/dev/null || echo "  (single arch)"
echo ""
echo "Driver bundle:"
file "$DIST_DIR/duomicDriver.driver/Contents/MacOS/duomicDriver"
lipo -info "$DIST_DIR/duomicDriver.driver/Contents/MacOS/duomicDriver"
echo ""
echo "PKG installer:"
ls -lh "$DIST_DIR/duomic-$VERSION-unsigned.pkg"
echo ""

echo "=========================================="
echo "  ✓ Release test complete!"
echo "=========================================="
echo ""
echo "Artifacts in: $DIST_DIR"
echo ""
echo "To test install (unsigned):"
echo "  sudo installer -pkg $DIST_DIR/duomic-$VERSION-unsigned.pkg -target /"
echo ""
echo "NOTE: For signed release, you need:"
echo "  - Apple Developer ID Application certificate"
echo "  - Apple Developer ID Installer certificate"
echo "  - App-specific password for notarization"
