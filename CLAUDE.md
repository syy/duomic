# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**duomic** splits a stereo USB microphone into separate virtual mono microphones on macOS. It consists of:
- **C++ Driver** (`Driver/duomicDriver/`) - AudioServerPlugin (HAL Plugin) using libASPL framework
- **Rust CLI** (`cli/`) - TUI application using ratatui for device selection and monitoring

## Build Commands

### Driver (C++ / CMake)
```bash
cd Driver/duomicDriver
mkdir -p build && cd build
cmake ..
make -j4

# Install (requires sudo, restarts coreaudiod)
cd ../../..
sudo ./install.sh
```

### CLI (Rust / Cargo)
```bash
cd cli
cargo build --release
sudo cp target/release/duomic /usr/local/bin/
```

### libASPL Submodule
```bash
cd Driver/libASPL
make          # Release build
make test     # Run test suite (builds debug first)
make fmt      # Format with clang-format
```

## Running

```bash
duomic              # Interactive TUI (device selection → channel config → dashboard)
duomic status       # Check driver connection and config
duomic run -v       # Verbose logging (-vv debug, -vvv trace)
```

## IPC Protocol Critical Details

### Unix Socket (`/tmp/duomic.sock`)

**⚠️ Driver closes connection after EACH command - must reconnect for every command:**
```rust
// Each command needs fresh connection
self.connect()?;
stream.write_all(b"ADD MyMic:0\n")?;  // Commands MUST end with \n
```

**Commands:** `ADD name:channel\n`, `REMOVE name\n`, `LIST\n`, `PING\n`

### Shared Memory (`/tmp/duomic_audio`)

**⚠️ writePos must be monotonically increasing (wraps at u32::MAX, NOT buffer size):**
```rust
// ❌ WRONG - causes audio glitch every ~170ms
write_pos = (write_pos + 1) % 8192;

// ✅ CORRECT
write_pos = write_pos.wrapping_add(1);
```

**⚠️ Memory barrier required before updating writePos:**
```rust
fence(Ordering::Release);
self.set_write_pos(write_pos);
```

**Header (16 bytes):** writePos(u32) | channelCount(u32) | sampleRate(u32) | active(u32)
**Data:** Interleaved Float32 samples, 8192 frames ring buffer

## Architecture

### Audio Pipeline
1. CLI captures via cpal (lock-free callback, no mutex)
2. Float32 samples written to shared memory ring buffer
3. Driver reads shared memory, converts to SInt16 for CoreAudio
4. Latency: ~21ms (1024 samples @ 48kHz)

### CLI State Machine (`commands/run.rs`)
`AskAction` → `SelectDevice` → `SelectChannels` → `EnterNames` → `Running` → `Quit`

### Key Files
- `cli/src/ipc/shm.rs` - Shared memory ring buffer (monotonic writePos)
- `cli/src/ipc/socket.rs` - Unix socket client (reconnect per command)
- `cli/src/audio/capture.rs` - Lock-free audio capture
- `Driver/duomicDriver/Driver.cpp` - HAL plugin implementation

## Sample Format

- **Shared Memory:** Float32 [-1.0, 1.0]
- **Driver Output:** SInt16 (libASPL default)
- Using wrong format causes distorted/mechanical audio

## Troubleshooting

```bash
# Check if driver loaded
system_profiler SPAudioDataType | grep duomic

# Test socket manually
echo "PING" | nc -U /tmp/duomic.sock

# Restart driver
sudo killall coreaudiod

# Fix permissions
sudo chmod -R a+rX /Library/Audio/Plug-Ins/HAL/duomicDriver.driver
```

## Documentation

- `SPEC.md` - Full technical specification
- `Driver/DRIVER_IPC.md` - Detailed IPC protocol with common mistakes
