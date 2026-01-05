# duomic Driver IPC Protocol Documentation

This document describes how to communicate with the duomic driver. **Read this carefully before implementing a client** - there are several non-obvious behaviors that can cause bugs.

---

## Overview

The driver uses two IPC mechanisms:
1. **Unix Socket** (`/tmp/duomic.sock`) - For commands (ADD, REMOVE, LIST, PING)
2. **Shared Memory** (`/tmp/duomic_audio`) - For audio data transfer

---

## Unix Socket Protocol

### ⚠️ CRITICAL: Connection Behavior

**The driver closes the connection after EACH command.**

This means:
- You MUST reconnect for every command
- You CANNOT send multiple commands on the same connection
- If you try to reuse a connection, the second command will fail

```rust
// ❌ WRONG - Will fail on second command
socket.connect()?;
socket.send("ADD Mic1:0\n")?;  // Works
socket.send("ADD Mic2:1\n")?;  // FAILS - connection already closed by driver

// ✅ CORRECT - Reconnect for each command
fn send_command(&mut self, cmd: &str) -> Result<String> {
    self.connect()?;  // Always reconnect
    // ... send and receive ...
}

self.send_command("ADD Mic1:0\n")?;  // Works
self.send_command("ADD Mic2:1\n")?;  // Works - new connection
```

### ⚠️ CRITICAL: Command Termination

**Commands MUST end with a newline (`\n`).**

```rust
// ❌ WRONG - No newline, driver may not process correctly
socket.write_all(b"ADD MyMic:0")?;

// ✅ CORRECT - Include newline
socket.write_all(b"ADD MyMic:0\n")?;

// ✅ CORRECT - Using format!
let cmd = format!("ADD {}:{}\n", name, channel);
socket.write_all(cmd.as_bytes())?;
```

### Socket Path

```
/tmp/duomic.sock
```

The socket is created when the driver loads (when coreaudiod starts) and removed when unloaded.

### Connection Example (Rust)

```rust
use std::os::unix::net::UnixStream;
use std::io::{Read, Write, BufRead, BufReader};
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/duomic.sock";

pub struct DriverClient {
    stream: Option<UnixStream>,
}

impl DriverClient {
    pub fn new() -> Self {
        Self { stream: None }
    }

    pub fn is_driver_available() -> bool {
        std::path::Path::new(SOCKET_PATH).exists()
    }

    pub fn connect(&mut self) -> Result<()> {
        // Always create a NEW connection
        let stream = UnixStream::connect(SOCKET_PATH)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn send_command(&mut self, command: &str) -> Result<String> {
        // MUST reconnect for each command
        self.connect()?;

        let stream = self.stream.as_mut().unwrap();

        // MUST include newline
        let cmd_with_newline = if command.ends_with('\n') {
            command.to_string()
        } else {
            format!("{}\n", command)
        };

        stream.write_all(cmd_with_newline.as_bytes())?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response)?;

        // For LIST command, read additional lines
        if command.starts_with("LIST") {
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,  // EOF
                    Ok(_) => response.push_str(&line),
                    Err(_) => break,
                }
            }
        }

        Ok(response)
    }
}
```

---

## Commands

### ADD - Create Virtual Device

Creates a new virtual microphone that reads from a specific channel.

**Format:**
```
ADD <name>:<channel>\n
```

**Parameters:**
- `name`: Device name (shown in System Settings > Sound)
- `channel`: Source channel index (0-7)

**Examples:**
```
ADD Podcast Host:0\n
ADD Podcast Guest:1\n
ADD duomic L:0\n
```

**Responses:**
```
OK:Device added\n        # Success
ERROR:Device already exists\n  # Name already in use
ERROR:Invalid name\n     # Empty name
ERROR:Invalid channel\n  # Channel < 0 or >= 8
```

### REMOVE - Delete Virtual Device

Removes a virtual microphone by name.

**Format:**
```
REMOVE <name>\n
```

**Examples:**
```
REMOVE Podcast Host\n
REMOVE duomic L\n
```

**Responses:**
```
OK:Device removed\n      # Success
ERROR:Device not found\n # No device with this name
ERROR:Invalid name\n     # Empty name
```

### LIST - List Virtual Devices

Returns all active virtual devices.

**Format:**
```
LIST\n
```

**Response Format:**
```
OK\n
<name1>:<channel1>\n
<name2>:<channel2>\n
...
```

**Example Response:**
```
OK
Podcast Host:0
Podcast Guest:1
```

**Parsing Notes:**
- First line is always `OK\n`
- Each subsequent line is `name:channel\n`
- Empty list = only `OK\n` is returned
- Read until EOF (connection closes after response)

### PING - Connection Test

Tests if the driver is responsive.

**Format:**
```
PING\n
```

**Response:**
```
PONG\n
```

---

## Shared Memory Protocol

### File Path

```
/tmp/duomic_audio
```

This file is created by the CLI, not the driver. The driver opens it read-only.

### Memory Layout

```
Offset  Size    Type      Description
──────────────────────────────────────────────────────
0       4       uint32    writePos - CLI write position
4       4       uint32    channelCount - Number of channels
8       4       uint32    sampleRate - Sample rate (e.g., 48000)
12      4       uint32    active - CLI active flag (0 or 1)
16      N       float[]   Audio data (interleaved)
```

**Total Size:** `16 + (RING_BUFFER_FRAMES × channelCount × sizeof(float))` bytes

### Audio Data Format

- **Sample Format:** 32-bit float, range [-1.0, 1.0]
- **Channel Layout:** Interleaved
- **Ring Buffer Size:** 8192 frames (~170ms at 48kHz)

```
Frame 0: [ch0, ch1, ch2, ...]
Frame 1: [ch0, ch1, ch2, ...]
...
Frame 8191: [ch0, ch1, ch2, ...]
```

### ⚠️ CRITICAL: writePos Protocol

**writePos is MONOTONICALLY INCREASING - it wraps at u32::MAX, NOT at buffer size!**

```rust
// ❌ WRONG - Causes audio glitch every ~170ms
write_pos = (write_pos + 1) % RING_BUFFER_FRAMES;

// ✅ CORRECT - Monotonic increase, wraps at u32::MAX
write_pos = write_pos.wrapping_add(1);
```

**Why this matters:**

The driver calculates available samples as:
```cpp
uint32_t available = writePos - readPos;
```

If writePos wraps at 8192:
- writePos goes 8191 → 0
- available becomes 0 - 5000 = underflow (huge number)
- Driver thinks there are billions of samples → audio glitch

If writePos is monotonic:
- writePos goes 4294967295 → 0 (at u32::MAX)
- This only happens after ~24 hours at 48kHz
- Normal operation is unaffected

### Memory Barriers

**Always use a memory barrier before updating writePos!**

```rust
use std::sync::atomic::{fence, Ordering};

// Write audio data first
for sample in samples {
    buffer[write_idx] = sample;
    write_idx = (write_idx + 1) % buffer_len;
}

// Memory barrier - ensures all writes are visible
fence(Ordering::Release);

// NOW update writePos
self.set_write_pos(write_pos);
```

Without the barrier, the driver might see the new writePos before the audio data is actually written, causing garbage audio.

### Reading Samples (Driver Side)

```cpp
uint32_t writePos = getWritePos();
uint32_t available = writePos - readPos_;  // Works due to unsigned wraparound

// Calculate frame index using modulo
uint32_t frameIdx = readPos_ % RING_BUFFER_FRAMES;
uint32_t sampleIdx = frameIdx * channelCount + channelIndex;
float sample = samples[sampleIdx];

readPos_++;
```

---

## Initialization Sequence

### Driver Startup (automatic)

1. coreaudiod loads the driver
2. Driver creates Unix socket at `/tmp/duomic.sock`
3. Driver tries to connect to shared memory `/tmp/duomic_audio`
4. Driver reads config from `/tmp/duomic_config` (or uses defaults)
5. Driver creates initial virtual devices

### CLI Startup (your responsibility)

1. Create shared memory file `/tmp/duomic_audio`
2. Initialize header (channelCount, sampleRate, active=1)
3. Connect to driver socket `/tmp/duomic.sock`
4. Send ADD commands for virtual devices
5. Start audio capture loop
6. Write audio to shared memory, update writePos

### Cleanup

On CLI exit:
1. Set `active` flag to 0 in shared memory
2. Send REMOVE commands for all devices
3. Close shared memory

The driver detects `active=0` and outputs silence.

---

## Common Mistakes

### 1. Reusing Socket Connection

```rust
// ❌ WRONG
let mut stream = UnixStream::connect(path)?;
stream.write_all(b"ADD Mic1:0\n")?;
stream.write_all(b"ADD Mic2:1\n")?;  // FAILS!

// ✅ CORRECT
fn add_device(&mut self, name: &str, ch: u32) {
    self.connect()?;  // New connection each time
    self.stream.write_all(format!("ADD {}:{}\n", name, ch))?;
    // ...
}
```

### 2. Missing Newline

```rust
// ❌ WRONG
stream.write_all(b"PING")?;

// ✅ CORRECT
stream.write_all(b"PING\n")?;
```

### 3. writePos Modulo

```rust
// ❌ WRONG - Audio glitch every 170ms
write_pos = (write_pos + 1) % 8192;

// ✅ CORRECT
write_pos = write_pos.wrapping_add(1);
```

### 4. Missing Memory Barrier

```rust
// ❌ WRONG - Race condition
buffer[idx] = sample;
header.write_pos = new_pos;  // Driver might read old data!

// ✅ CORRECT
buffer[idx] = sample;
fence(Ordering::Release);
header.write_pos = new_pos;
```

### 5. Lock in Audio Callback

```rust
// ❌ WRONG - Can cause audio dropouts
let callback = move |data: &[f32], _| {
    let mut buffer = shared_buffer.lock().unwrap();  // MUTEX IN REALTIME!
    buffer.write(data);
};

// ✅ CORRECT - Lock-free
let callback = move |data: &[f32], _| {
    // shared_buffer is owned by callback, no lock needed
    shared_buffer.write(data);
};
```

### 6. Parsing LIST Response

```rust
// ❌ WRONG - Only reads first line
let response = reader.read_line()?;  // Gets "OK\n" only

// ✅ CORRECT - Read all lines
let mut response = String::new();
reader.read_line(&mut response)?;  // "OK\n"
loop {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => break,  // EOF
        Ok(_) => response.push_str(&line),
        Err(_) => break,
    }
}
```

---

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| SOCKET_PATH | `/tmp/duomic.sock` | Unix socket path |
| SHM_PATH | `/tmp/duomic_audio` | Shared memory path |
| CONFIG_PATH | `/tmp/duomic_config` | Initial config path |
| RING_BUFFER_FRAMES | 8192 | Ring buffer size in frames |
| HEADER_SIZE | 16 | Shared memory header size (bytes) |
| MAX_CHANNELS | 8 | Maximum supported channels |
| SAMPLE_RATE | 48000 | Audio sample rate |
| TARGET_LATENCY | 1024 | Target latency in samples (~21ms) |

---

## Debugging Tips

### Check if Driver is Loaded

```bash
system_profiler SPAudioDataType | grep duomic
```

### Check Socket Exists

```bash
ls -la /tmp/duomic.sock
```

### Test Connection Manually

```bash
echo "PING" | nc -U /tmp/duomic.sock
# Should respond: PONG
```

### List Devices Manually

```bash
echo "LIST" | nc -U /tmp/duomic.sock
```

### Restart Driver

```bash
sudo killall coreaudiod
# coreaudiod restarts automatically
```

### Check Shared Memory

```bash
ls -la /tmp/duomic_audio
hexdump -C /tmp/duomic_audio | head -5
```

---

## Version History

| Version | Changes |
|---------|---------|
| 1.0 | Initial protocol |
| 2.0 | Added SYNC command (removed), monotonic writePos |
