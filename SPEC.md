# duomic - Stereo USB Microphone Splitter

## Project Summary

**duomic** is a CLI application that presents the left (L) and right (R) channels of a stereo USB microphone as separate virtual microphones on macOS. This allows assigning one channel to Zoom and another to Discord, or recording 2 people on separate tracks in podcast recordings.

### Primary Use Case
- **Multi-app routing**: Zoom + Discord can work simultaneously with different microphones
- Developed as an alternative to [Loopback](https://rogueamoeba.com/loopback/) (Rogue Amoeba)

---

## Architecture

### Technology Stack

| Component | Technology |
|-----------|------------|
| Driver | AudioServerPlugin (HAL Plugin) |
| Driver Framework | [libASPL](https://github.com/gavv/libASPL) (C++17) |
| CLI | Rust + ratatui TUI |
| IPC | Unix Socket + Shared Memory (mmap) |

**Note**: Apple doesn't grant DriverKit entitlements for virtual audio devices, so **AudioServerPlugin (HAL Plugin)** is used instead.

### Project Structure
```
sound-spliter/
├── Driver/
│   ├── libASPL/                    # Git submodule - HAL plugin framework
│   └── duomicDriver/
│       ├── Driver.cpp              # Main driver code
│       ├── CMakeLists.txt
│       └── Info.plist.in
├── cli/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 # Entry point + clap setup
│       ├── commands/
│       │   ├── run.rs              # Audio capture + TUI dashboard
│       │   └── status.rs           # Driver status check
│       ├── tui/
│       │   ├── app.rs              # Terminal wrapper
│       │   ├── events.rs           # Keyboard/terminal events
│       │   └── widgets/
│       │       ├── device_list.rs  # Device picker widget
│       │       ├── channel_picker.rs
│       │       └── level_meter.rs  # Audio level meter
│       ├── audio/
│       │   ├── capture.rs          # cpal audio capture (lock-free)
│       │   └── devices.rs          # Device enumeration
│       ├── ipc/
│       │   ├── socket.rs           # Unix socket communication
│       │   └── shm.rs              # Shared memory ring buffer
│       └── config/
│           └── store.rs            # TOML config management
├── install.sh                      # Driver installer
└── SPEC.md
```

---

## Driver Implementation

### libASPL Usage
```cpp
// Device creation
aspl::DeviceParameters params;
params.Name = "duomic L";
params.Manufacturer = "duomic";
params.SampleRate = 48000;
params.ChannelCount = 1;  // Mono

auto device = std::make_shared<aspl::Device>(context, params);
device->AddStreamWithControlsAsync(aspl::Direction::Input);

// IO Handler
class DuomicIOHandler : public aspl::IORequestHandler {
    void OnReadClientInput(...) override {
        // Read from shared memory, return in SInt16 format
    }
};
```

### Critical Technical Details

1. **Sample Format**: libASPL uses **SInt16** (16-bit integer) by default. Writing float caused distorted audio - this was a critical bug fix.

2. **Realtime Audio Thread Rules**:
   - NEVER open/close files in `OnReadClientInput` callback
   - Only atomic operations and memory reads allowed
   - System calls will lock coreaudiod

3. **Ring Buffer Synchronization**:
   - `writePos` is written by CLI (monotonically increasing, wraps at u32::MAX)
   - `readPos` is maintained separately for each device in driver
   - Target latency: ~1024 samples (~21ms @ 48kHz)

---

## IPC Mechanism

### Unix Socket (Command Channel)
```
Socket: /tmp/duomic.sock

Commands:
- ADD name:channel  → Create new device
- REMOVE name       → Delete device
- LIST              → List devices
- PING              → Connection test

Responses:
- OK:message
- ERROR:error
- PONG
```

### Shared Memory (Audio Data)
```
File: /tmp/duomic_audio

Header (16 bytes):
[0-3]   writePos (uint32)     - CLI write position (monotonic)
[4-7]   channelCount (uint32) - Channel count
[8-11]  sampleRate (uint32)   - Sample rate
[12-15] active (uint32)       - CLI active flag (0/1)

Data (16+ bytes):
Interleaved float samples: [ch0, ch1, ch2, ...] × RING_BUFFER_FRAMES
```

### Dynamic Device Management (Loopback-style)
```
CLI                    Driver
 │                       │
 │── ADD "Mic":0 ──────►│ plugin->AddDevice()
 │◄── OK ───────────────│
 │                       │
 │── REMOVE "Mic" ─────►│ plugin->RemoveDevice()
 │◄── OK ───────────────│
```

**No coreaudiod restart required!** Runtime device add/remove works.

---

## CLI Commands

```bash
# Default - start audio capture with TUI
duomic [run]

# Start with specific device
duomic run --device "BOYALINK"

# Check driver status
duomic status

# Verbose modes
duomic run -v      # Info level
duomic run -vv     # Debug level
duomic run -vvv    # Trace level
```

### Interactive TUI Flow

1. **Device Selection**: Arrow keys to navigate, Enter to select
2. **Channel Selection**: Space to toggle channels, Enter to confirm
3. **Name Entry**: Type custom names or leave empty for auto-generated
4. **Running Dashboard**: Real-time level meters with stats

---

## Lock-Free Audio Architecture

### Rust Implementation

The CLI uses a **lock-free** design for real-time audio:

```rust
// Audio callback owns the buffer directly - no mutex
pub fn start(device: &cpal::Device, mut shm: SharedAudioBuffer) -> Result<Self> {
    // Pre-allocated buffers
    let mut sample_buffer: Vec<f32> = Vec::with_capacity(4096 * channels);
    let mut peaks = [0.0f32; MAX_CHANNELS];  // Fixed-size, no allocation

    let stream = device.build_input_stream(
        &config,
        move |data: &[T], _| {
            // No locks, no allocations in callback
            // Write directly to shared memory
            shm.write_frame(&samples);

            // Memory barrier before updating position
            fence(Ordering::Release);
        },
        // ...
    );
}
```

### Ring Buffer Protocol

```rust
// writePos is monotonically increasing (wraps at u32::MAX, not buffer size)
// This prevents driver from miscalculating available samples
write_pos = write_pos.wrapping_add(1);

// Memory barrier ensures all audio data is visible before position update
fence(Ordering::Release);
self.set_write_pos(write_pos);
```

---

## Configuration

### Config File (TOML)
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

---

## Installation

### Driver Installation
```bash
# Build
cd Driver/duomicDriver
mkdir -p build && cd build
cmake ..
make -j4

# Install (once)
cd ../../..
sudo ./install.sh
```

### CLI Installation
```bash
cd cli
cargo build --release
sudo cp target/release/duomic /usr/local/bin/
```

---

## Performance

| Parameter | Value |
|-----------|-------|
| Ring Buffer | 8192 frames (~170ms @ 48kHz) |
| Target Latency | 1024 samples (~21ms) |
| Sample Rate | 48000 Hz |
| Sample Format | SInt16 (driver) / Float32 (CLI) |

---

## Troubleshooting

### 1. Driver not loading / not visible
**Cause**: Info.plist permission issue
**Solution**: `sudo chmod -R a+rX /Library/Audio/Plug-Ins/HAL/duomicDriver.driver`

### 2. Distorted/mechanical audio
**Cause**: Float format instead of SInt16
**Solution**: Use `ConvertToSInt16()` for conversion

### 3. coreaudiod lockup
**Cause**: File operations in audio thread
**Solution**: Move all I/O to background thread

### 4. Audio glitches every ~170ms
**Cause**: writePos wrapping at buffer size instead of monotonic
**Solution**: Use `wrapping_add(1)` without modulo

---

## Completed Features (v2.0.0)

- [x] L/R channel separation
- [x] Dynamic device creation/deletion (runtime)
- [x] User-defined microphone names
- [x] Unix socket IPC
- [x] Shared memory audio transfer
- [x] TUI with real-time level meters
- [x] Interactive device/channel selection
- [x] Lock-free audio implementation
- [x] ~21ms latency
- [x] Rust CLI with ratatui TUI

### Planned Features
- [ ] Hot-plug / auto-reconnect
- [ ] Daemon mode (launchd)
- [ ] Menu bar app (system tray)
- [ ] Multi-device support

---

## TUI Screenshots

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

---

## Keyboard Shortcuts

| Context | Key | Action |
|---------|-----|--------|
| List navigation | ↑/↓ | Move selection |
| List navigation | Enter | Confirm |
| List navigation | q | Quit |
| Channel selection | Space | Toggle channel |
| Text input | Enter | Confirm |
| Text input | Esc | Back |
| Dashboard | q | Quit |
| Dashboard | r | Restart |
| Dashboard | s | Setup |
| Any | Ctrl+C | Force quit |

---

## References

- [libASPL](https://github.com/gavv/libASPL) - AudioServerPlugin framework
- [Loopback](https://rogueamoeba.com/loopback/) - Inspiration
- [BlackHole](https://github.com/ExistentialAudio/BlackHole) - Open source virtual audio driver
- [ratatui](https://ratatui.rs) - Rust TUI framework
- [cpal](https://github.com/RustAudio/cpal) - Cross-platform audio I/O
