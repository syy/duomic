# Contributing to duomic

Thank you for your interest in contributing to duomic! This document provides guidelines and instructions for contributing.

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Getting Started

### Prerequisites

- macOS 12 (Monterey) or later
- Xcode Command Line Tools (`xcode-select --install`)
- Rust toolchain ([rustup.rs](https://rustup.rs))
- CMake 3.12+ (`brew install cmake`)

### Development Setup

1. **Clone with submodules**:
   ```bash
   git clone --recursive https://github.com/syy/duomic.git
   cd duomic
   ```

2. **Build the driver**:
   ```bash
   cd Driver/duomicDriver
   mkdir -p build && cd build
   cmake ..
   make -j4
   ```

3. **Install the driver** (requires sudo):
   ```bash
   cd ../../..
   sudo ./install.sh
   ```

4. **Build the CLI**:
   ```bash
   cd cli
   cargo build
   ```

5. **Run in development**:
   ```bash
   cargo run -- run -vvv
   ```

### Project Structure

```
duomic/
├── Driver/
│   ├── libASPL/           # Git submodule - HAL plugin framework
│   └── duomicDriver/
│       ├── Driver.cpp     # Main driver implementation
│       ├── CMakeLists.txt
│       └── Info.plist.in
├── cli/
│   └── src/
│       ├── main.rs        # Entry point + clap CLI
│       ├── commands/      # run, status commands
│       ├── tui/           # Terminal UI (ratatui)
│       ├── audio/         # Audio capture (cpal)
│       ├── ipc/           # Socket + shared memory
│       └── config/        # TOML configuration
├── install.sh
├── uninstall.sh
└── SPEC.md                # Technical specification
```

## How to Contribute

### Reporting Bugs

Before submitting a bug report:

1. Check existing [issues](https://github.com/syy/duomic/issues) for duplicates
2. Collect diagnostic information:
   - macOS version (`sw_vers`)
   - duomic version (`duomic --version`)
   - USB microphone model
   - Output of `duomic run -vvv`

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md) when creating an issue.

### Suggesting Features

Feature requests are welcome! Please use the [feature request template](.github/ISSUE_TEMPLATE/feature_request.md) and include:

- Clear description of the problem it solves
- Proposed solution
- Alternative approaches considered

### Pull Requests

1. **Fork** the repository
2. **Create a branch** from `main`:
   ```bash
   git checkout -b feature/amazing-feature
   ```
3. **Make your changes** following the code style guidelines
4. **Test** your changes:
   ```bash
   cd cli
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   ```
5. **Commit** with clear messages:
   ```bash
   git commit -m "Add amazing feature"
   ```
6. **Push** to your fork:
   ```bash
   git push origin feature/amazing-feature
   ```
7. **Open a Pull Request** against `main`

### PR Guidelines

- Keep PRs focused on a single change
- Update documentation if needed
- Add tests for new functionality
- Ensure CI passes before requesting review
- Link related issues in the PR description

## Code Style

### Rust

- Follow `rustfmt` defaults
- Run `cargo fmt` before committing
- Address all `cargo clippy` warnings
- Use descriptive variable names
- Document public APIs with doc comments

```rust
/// Captures audio from the specified device.
///
/// # Arguments
/// * `device` - The audio input device to capture from
/// * `config` - Stream configuration (sample rate, channels)
///
/// # Returns
/// A handle to the running capture stream
pub fn start_capture(device: &Device, config: &StreamConfig) -> Result<CaptureHandle> {
    // ...
}
```

### C++

- Follow Google C++ style with modifications
- Use `clang-format` for formatting
- Modern C++17 features are encouraged
- Document with Doxygen-style comments

```cpp
/**
 * @brief Reads audio samples from shared memory
 * @param frames Number of frames to read
 * @param buffer Output buffer for samples
 * @return Number of frames actually read
 */
UInt32 ReadFromSharedMemory(UInt32 frames, SInt16* buffer);
```

### Commit Messages

- Use present tense ("Add feature" not "Added feature")
- Use imperative mood ("Fix bug" not "Fixes bug")
- Keep first line under 72 characters
- Reference issues when applicable

Good examples:
```
Add volume control to TUI dashboard

Fix audio underrun when buffer is nearly empty

Update libASPL to v2.1.0

Closes #42
```

## Testing

### Running Tests

```bash
# Rust tests
cd cli
cargo test

# With logging
cargo test -- --nocapture
```

### Manual Testing Checklist

Before submitting a PR that affects audio:

- [ ] Test device selection with multiple USB mics
- [ ] Test channel assignment (L/R split)
- [ ] Verify audio quality (no distortion/glitches)
- [ ] Test with Zoom, Discord, or other VoIP apps
- [ ] Check CPU usage during extended operation
- [ ] Test disconnect/reconnect handling

## Development Tips

### Debugging the Driver

Driver logs go to the system log:
```bash
log stream --predicate 'subsystem == "com.duomic.driver"' --level debug
```

### Debugging IPC

Test socket manually:
```bash
echo "PING" | nc -U /tmp/duomic.sock
echo "LIST" | nc -U /tmp/duomic.sock
```

### Rebuilding After Changes

Driver changes require reinstall:
```bash
cd Driver/duomicDriver/build
make -j4
cd ../../..
sudo ./install.sh
```

CLI changes just need rebuild:
```bash
cd cli
cargo build
```

## Release Process

Releases are handled by maintainers:

1. Update version in `Cargo.toml` and `CMakeLists.txt`
2. Update CHANGELOG (via GitHub Releases)
3. Create and push version tag: `git tag v0.2.0`
4. GitHub Actions builds, signs, and publishes

## Questions?

- Open a [Discussion](https://github.com/syy/duomic/discussions) for general questions
- Check existing issues and discussions first
- Be patient - this is a volunteer-maintained project

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
