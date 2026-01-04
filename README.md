# duomic

Split your stereo USB microphone into separate virtual mono microphones on macOS.

## What is this?

**duomic** takes a multi-channel USB microphone and creates independent virtual microphone devices for each channel. This allows you to:

- Use the left channel in Zoom while the right channel goes to Discord
- Record podcast guests on separate tracks in your DAW
- Route different mic channels to different apps simultaneously

Think of it as a free, open-source alternative to [Loopback](https://rogueamoeba.com/loopback/) for microphone splitting.

## Requirements

- macOS 13.0 or later
- A multi-channel USB microphone (stereo or more)

## Quick Start

```bash
# Build and install the driver
cd Driver/duomicDriver
mkdir -p build && cd build
cmake ..
make -j4
cd ../../..
sudo ./install.sh

# Build the CLI
cd CLI
swift build -c release
sudo cp .build/release/duomic /usr/local/bin/

# Run
duomic
```

## Usage

```bash
# Start capturing (interactive device selection)
duomic

# Use a specific device
duomic run --device "BOYALINK"

# Configure virtual microphones interactively
duomic setup

# Add/remove devices at runtime (no restart needed!)
duomic add "Podcast Host" --channel 0
duomic add "Podcast Guest" --channel 1
duomic remove "Podcast Host"

# List active virtual devices
duomic list

# Check driver status
duomic status
```

## Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│   USB Mic       │────▶│   duomic     │────▶│  Virtual Mics   │
│  (Stereo L/R)   │     │   CLI        │     │  ├─ duomic L    │
└─────────────────┘     │              │     │  └─ duomic R    │
                        │  (captures   │     └─────────────────┘
                        │   audio)     │              │
                        └──────┬───────┘              │
                               │                      ▼
                        Shared Memory          Apps (Zoom,
                               │               Discord, DAW)
                        ┌──────▼───────┐
                        │ HAL Plugin   │
                        │ (Driver)     │
                        └──────────────┘
```

**Components:**
- **Driver**: AudioServerPlugin (HAL Plugin) built with [libASPL](https://github.com/gavv/libASPL)
- **CLI**: Swift application using ArgumentParser
- **IPC**: Unix socket for commands + shared memory (mmap) for audio data

## Technical Details

- Sample rate: 48kHz
- Sample format: SInt16 (16-bit)
- Latency: ~21ms (1024 samples)
- Ring buffer: 8192 frames (~170ms)

## Troubleshooting

### Driver not loading
```bash
sudo chmod -R a+rX /Library/Audio/Plug-Ins/HAL/duomicDriver.driver
sudo killall coreaudiod
```

### Virtual devices not appearing
```bash
system_profiler SPAudioDataType | grep duomic
```

### CLI can't connect to driver
Make sure the driver is installed and coreaudiod has been restarted.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [libASPL](https://github.com/gavv/libASPL) - C++ library for AudioServerPlugin development
- [BlackHole](https://github.com/ExistentialAudio/BlackHole) - Inspiration for virtual audio architecture
