# duomic

Split your stereo USB microphone into separate virtual microphones on macOS.

**duomic** creates virtual audio devices from individual channels of a multi-channel USB microphone. Perfect for podcasters, streamers, and audio professionals who need independent control over each channel.

## Use Cases

- **Podcast Recording** - Record 2 people on separate tracks with a single stereo mic
- **Multi-App Routing** - Send left channel to Zoom, right channel to Discord simultaneously
- **Audio Engineering** - Route specific channels to different DAWs or processing chains

## Requirements

- macOS 12 (Monterey) or later
- Intel or Apple Silicon Mac
- Multi-channel USB microphone (stereo or more)

## Installation

### Homebrew (Recommended)

```bash
brew tap syy/duomic
brew install --cask duomic
```

### Manual Installation

Download the latest `.pkg` installer from [Releases](https://github.com/syy/duomic/releases).

### Building from Source

```bash
# Clone with submodules
git clone --recursive https://github.com/syy/duomic.git
cd duomic

# Build driver
cd Driver/duomicDriver
mkdir -p build && cd build
cmake ..
make -j4

# Install driver
cd ../../..
sudo ./install.sh

# Build CLI
cd cli
cargo build --release
sudo cp target/release/duomic /usr/local/bin/
```

## Quick Start

```bash
# Interactive TUI - select device, channels, and names
duomic

# Check driver status
duomic status

# Start with specific device
duomic run --device "BOYALINK"

# Verbose logging
duomic run -v      # Info
duomic run -vv     # Debug
duomic run -vvv    # Trace
```

## How It Works

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  USB Stereo Mic │────▶│   duomic CLI    │────▶│  Virtual Mics   │
│   (2 channels)  │     │ (captures audio)│     │  "Podcast Host" │
└─────────────────┘     └────────┬────────┘     │  "Podcast Guest"│
                                 │              └─────────────────┘
                                 │ Shared Memory
                                 ▼
                        ┌─────────────────┐
                        │  duomic Driver  │
                        │ (HAL Plugin)    │
                        └─────────────────┘
```

1. **CLI** captures audio from your USB microphone using CoreAudio
2. Audio data is written to shared memory (lock-free, low latency)
3. **Driver** (AudioServerPlugin) reads shared memory and presents virtual devices
4. Apps see separate mono microphones that can be assigned independently

### Architecture

| Component | Technology |
|-----------|------------|
| Driver | AudioServerPlugin (HAL Plugin) |
| Driver Framework | [libASPL](https://github.com/gavv/libASPL) (C++17) |
| CLI | Rust + [ratatui](https://ratatui.rs) TUI |
| IPC | Unix Socket + Shared Memory (mmap) |

## TUI Interface

### Device Selection
```
┌─────────────────────────────────────────────────────────┐
│  duomic - Select Device                                 │
├─────────────────────────────────────────────────────────┤
│  Input Devices                                          │
│                                                         │
│    ○ Built-in Microphone (2 channels)                  │
│  → ● BOYALINK (2 channels)                             │
│    ○ Rode NT-USB (2 channels)                          │
│                                                         │
│  [↑/↓] Select  [Enter] Confirm  [q] Quit               │
└─────────────────────────────────────────────────────────┘
```

### Running Dashboard
```
┌─────────────────────────────────────────────────────────┐
│  duomic | BOYALINK @ 48kHz | ● Running                  │
├─────────────────────────────────────────────────────────┤
│  Virtual Microphones                                    │
│                                                         │
│  Podcast Host  [Ch 0]  ████████████████░░░░            │
│  Podcast Guest [Ch 1]  ██████░░░░░░░░░░░░░░            │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  Latency: 21ms | Buffer: 87% | Duration: 00:15:32      │
│                                                         │
│  [q] Quit  [r] Restart  [s] Setup                      │
└─────────────────────────────────────────────────────────┘
```

## Configuration

duomic saves your settings automatically:

```toml
# ~/.config/duomic/config.toml

[device]
name = "BOYALINK"
sample_rate = 48000

[[virtual_mics]]
name = "Podcast Host"
channel = 0

[[virtual_mics]]
name = "Podcast Guest"
channel = 1
```

## Performance

| Parameter | Value |
|-----------|-------|
| Latency | ~21ms (1024 samples @ 48kHz) |
| Ring Buffer | 8192 frames (~170ms) |
| Sample Rate | 48000 Hz |
| Audio Thread | Lock-free, real-time safe |

## Troubleshooting

### Driver not visible in Audio MIDI Setup

```bash
# Check if driver is loaded
system_profiler SPAudioDataType | grep duomic

# Restart audio service
sudo killall coreaudiod

# Check permissions
sudo chmod -R a+rX /Library/Audio/Plug-Ins/HAL/duomicDriver.driver
```

### Connection refused

```bash
# Test socket connection
echo "PING" | nc -U /tmp/duomic.sock

# Check if CLI is running
duomic status
```

### Audio glitches or distortion

- Ensure no other apps are using the USB mic exclusively
- Try increasing buffer size in your DAW
- Check `duomic run -vvv` for underrun warnings

## Keyboard Shortcuts

| Context | Key | Action |
|---------|-----|--------|
| Navigation | ↑/↓ | Move selection |
| Navigation | Enter | Confirm |
| Navigation | q | Quit |
| Channel select | Space | Toggle channel |
| Text input | Esc | Back |
| Dashboard | r | Restart |
| Dashboard | s | Setup |
| Any | Ctrl+C | Force quit |

## Uninstalling

### Homebrew

```bash
brew uninstall --cask duomic
```

### Manual

```bash
sudo ./uninstall.sh
# Or manually:
sudo rm -rf /Library/Audio/Plug-Ins/HAL/duomicDriver.driver
sudo rm -f /usr/local/bin/duomic
rm -rf ~/.config/duomic
sudo killall coreaudiod
```

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a PR.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [Loopback](https://rogueamoeba.com/loopback/) by Rogue Amoeba
- Built with [libASPL](https://github.com/gavv/libASPL) by Victor Gaydov
- TUI powered by [ratatui](https://ratatui.rs)
- Audio I/O via [cpal](https://github.com/RustAudio/cpal)
