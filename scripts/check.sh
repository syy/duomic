#!/bin/bash
set -e

cd "$(dirname "$0")/../cli"

echo "=========================================="
echo "  duomic CI Check Script"
echo "=========================================="
echo ""

echo "▶ [1/4] Checking formatting..."
if cargo fmt -- --check; then
    echo "✓ Formatting OK"
else
    echo "✗ Formatting failed. Run: cargo fmt"
    exit 1
fi
echo ""

echo "▶ [2/4] Running Clippy..."
if cargo clippy -- -D warnings; then
    echo "✓ Clippy OK"
else
    echo "✗ Clippy failed"
    exit 1
fi
echo ""

echo "▶ [3/4] Building release..."
if cargo build --release; then
    echo "✓ Build OK"
else
    echo "✗ Build failed"
    exit 1
fi
echo ""

echo "▶ [4/4] Running tests..."
if cargo test; then
    echo "✓ Tests OK"
else
    echo "✗ Tests failed"
    exit 1
fi
echo ""

echo "=========================================="
echo "  ✓ All checks passed!"
echo "=========================================="
