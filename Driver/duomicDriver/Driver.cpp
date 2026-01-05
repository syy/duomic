// duomic - Stereo USB Microphone Splitter Driver
// Creates virtual mono microphones from USB mic channels
// Supports runtime device management via Unix socket IPC

#include <aspl/Driver.hpp>

#include <CoreAudio/AudioServerPlugIn.h>

#include <atomic>
#include <cmath>
#include <cstring>
#include <fstream>
#include <limits>
#include <memory>
#include <mutex>
#include <sstream>
#include <thread>
#include <vector>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

namespace {

constexpr UInt32 SampleRate = 48000;
constexpr UInt32 ChannelCount = 1;

// IPC paths
constexpr const char* SOCKET_PATH = "/tmp/duomic.sock";
constexpr const char* SHM_PATH = "/tmp/duomic_audio";
constexpr const char* CONFIG_PATH = "/tmp/duomic_config";

constexpr size_t MAX_CHANNELS = 8;
constexpr size_t RING_BUFFER_FRAMES = 8192;
constexpr size_t HEADER_SIZE = 16;

// Forward declarations
class DuomicIOHandler;

// Device info
struct DeviceInfo {
    std::string name;
    int channel;
    std::shared_ptr<aspl::Device> device;
    std::shared_ptr<DuomicIOHandler> handler;
};

// Global state
static std::shared_ptr<aspl::Context> g_context;
static std::shared_ptr<aspl::Plugin> g_plugin;
static std::vector<DeviceInfo> g_devices;
static std::mutex g_devicesMutex;
static std::atomic<bool> g_running{true};
static std::thread g_ipcThread;

// Shared memory accessor
class SharedAudioBuffer {
public:
    SharedAudioBuffer() = default;
    ~SharedAudioBuffer() { disconnect(); }

    void connect() {
        if (ptr_.load(std::memory_order_acquire)) return;
        int fd = open(SHM_PATH, O_RDONLY);
        if (fd >= 0) {
            off_t size = lseek(fd, 0, SEEK_END);
            lseek(fd, 0, SEEK_SET);
            if (size > 0) {
                bufferSize_ = static_cast<size_t>(size);
                void* mapped = mmap(nullptr, bufferSize_, PROT_READ, MAP_SHARED, fd, 0);
                if (mapped != MAP_FAILED) {
                    fd_ = fd;
                    ptr_.store(mapped, std::memory_order_release);
                } else {
                    close(fd);
                }
            } else {
                close(fd);
            }
        }
    }

    void disconnect() {
        void* p = ptr_.exchange(nullptr, std::memory_order_acq_rel);
        if (p && p != MAP_FAILED) munmap(p, bufferSize_);
        if (fd_ >= 0) { close(fd_); fd_ = -1; }
    }

    bool isActive() const {
        void* p = ptr_.load(std::memory_order_acquire);
        if (!p) return false;
        return static_cast<const uint32_t*>(p)[3] == 1;
    }

    uint32_t getChannelCount() const {
        void* p = ptr_.load(std::memory_order_acquire);
        if (!p) return 2;
        return static_cast<const uint32_t*>(p)[1];
    }

    const float* getSamples() const {
        void* p = ptr_.load(std::memory_order_acquire);
        if (!p) return nullptr;
        return reinterpret_cast<const float*>(static_cast<const char*>(p) + HEADER_SIZE);
    }

    uint32_t getWritePos() const {
        void* p = ptr_.load(std::memory_order_acquire);
        if (!p) return 0;
        // Memory barrier: ensure we see all writes that happened before
        // the CLI's Release fence when updating writePos
        std::atomic_thread_fence(std::memory_order_acquire);
        return *static_cast<const uint32_t*>(p);
    }

private:
    std::atomic<void*> ptr_{nullptr};
    int fd_ = -1;
    size_t bufferSize_ = 0;
};

static SharedAudioBuffer g_sharedBuffer;

inline SInt16 ConvertToSInt16(float sample) {
    // Clamp to [-1.0, 1.0] range first, then scale to SInt16
    // This ensures symmetric clipping and correct conversion
    if (sample >= 1.0f) return 32767;
    if (sample <= -1.0f) return -32768;
    return static_cast<SInt16>(sample * 32767.0f);
}

class DuomicIOHandler : public aspl::ControlRequestHandler, public aspl::IORequestHandler
{
public:
    explicit DuomicIOHandler(int channelIndex)
        : channelIndex_(channelIndex)
    {}

    void OnReadClientInput(const std::shared_ptr<aspl::Client>& client,
        const std::shared_ptr<aspl::Stream>& stream,
        Float64 zeroTimestamp,
        Float64 timestamp,
        void* bytes,
        UInt32 bytesCount) override
    {
        SInt16* samples = static_cast<SInt16*>(bytes);
        UInt32 numSamples = bytesCount / sizeof(SInt16) / ChannelCount;

        if (!g_sharedBuffer.isActive()) {
            std::memset(bytes, 0, bytesCount);
            return;
        }

        const float* shmSamples = g_sharedBuffer.getSamples();
        if (!shmSamples) {
            std::memset(bytes, 0, bytesCount);
            return;
        }

        uint32_t writePos = g_sharedBuffer.getWritePos();
        uint32_t inputChannels = g_sharedBuffer.getChannelCount();

        if (channelIndex_ >= (int)inputChannels) {
            std::memset(bytes, 0, bytesCount);
            return;
        }

        constexpr uint32_t TARGET_LATENCY = 1024;

        if (readPos_ == 0 && writePos > TARGET_LATENCY) {
            readPos_ = writePos - TARGET_LATENCY;
        }

        uint32_t available = writePos - readPos_;

        if (available > RING_BUFFER_FRAMES - 512) {
            readPos_ = writePos - TARGET_LATENCY;
            available = TARGET_LATENCY;
        }

        if (writePos <= readPos_ || available < numSamples) {
            std::memset(bytes, 0, bytesCount);
            return;
        }

        uint32_t samplesToRead = std::min(numSamples, writePos - readPos_);

        for (UInt32 i = 0; i < samplesToRead; i++) {
            uint32_t frameIdx = (readPos_ + i) % RING_BUFFER_FRAMES;
            uint32_t sampleIdx = frameIdx * inputChannels + channelIndex_;
            samples[i] = ConvertToSInt16(shmSamples[sampleIdx]);
        }

        if (samplesToRead < numSamples) {
            std::memset(samples + samplesToRead, 0, (numSamples - samplesToRead) * sizeof(SInt16));
        }

        readPos_ += samplesToRead;
    }

private:
    int channelIndex_;
    uint32_t readPos_ = 0;
};

// Add a new virtual device at runtime
bool AddVirtualDevice(const std::string& name, int channel) {
    std::lock_guard<std::mutex> lock(g_devicesMutex);

    // Check if device with this name already exists
    for (const auto& dev : g_devices) {
        if (dev.name == name) {
            return false;
        }
    }

    aspl::DeviceParameters params;
    params.Name = name.c_str();
    params.Manufacturer = "duomic";
    params.SampleRate = SampleRate;
    params.ChannelCount = ChannelCount;

    auto device = std::make_shared<aspl::Device>(g_context, params);
    device->AddStreamWithControlsAsync(aspl::Direction::Input);

    auto handler = std::make_shared<DuomicIOHandler>(channel);
    device->SetControlHandler(handler);
    device->SetIOHandler(handler);

    g_plugin->AddDevice(device);

    g_devices.push_back({name, channel, device, handler});

    return true;
}

// Remove a virtual device at runtime
bool RemoveVirtualDevice(const std::string& name) {
    std::lock_guard<std::mutex> lock(g_devicesMutex);

    for (auto it = g_devices.begin(); it != g_devices.end(); ++it) {
        if (it->name == name) {
            g_plugin->RemoveDevice(it->device);
            g_devices.erase(it);
            return true;
        }
    }
    return false;
}

// List current devices
std::string ListDevices() {
    std::lock_guard<std::mutex> lock(g_devicesMutex);
    std::stringstream ss;
    for (const auto& dev : g_devices) {
        ss << dev.name << ":" << dev.channel << "\n";
    }
    return ss.str();
}

// Handle IPC command
std::string HandleCommand(const std::string& cmd) {
    std::istringstream iss(cmd);
    std::string command;
    iss >> command;

    if (command == "ADD") {
        std::string name;
        int channel;
        std::getline(iss >> std::ws, name, ':');
        iss >> channel;

        if (name.empty()) return "ERROR:Invalid name\n";
        if (channel < 0 || channel >= (int)MAX_CHANNELS) return "ERROR:Invalid channel\n";

        if (AddVirtualDevice(name, channel)) {
            return "OK:Device added\n";
        } else {
            return "ERROR:Device already exists\n";
        }
    }
    else if (command == "REMOVE") {
        std::string name;
        std::getline(iss >> std::ws, name);

        if (name.empty()) return "ERROR:Invalid name\n";

        if (RemoveVirtualDevice(name)) {
            return "OK:Device removed\n";
        } else {
            return "ERROR:Device not found\n";
        }
    }
    else if (command == "LIST") {
        return "OK\n" + ListDevices();
    }
    else if (command == "PING") {
        return "PONG\n";
    }

    return "ERROR:Unknown command\n";
}

// IPC thread function
void IPCThread() {
    // Remove old socket
    unlink(SOCKET_PATH);

    // Create socket
    int serverFd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (serverFd < 0) return;

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, SOCKET_PATH, sizeof(addr.sun_path) - 1);

    if (bind(serverFd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        close(serverFd);
        return;
    }

    // Make socket accessible
    chmod(SOCKET_PATH, 0666);

    if (listen(serverFd, 5) < 0) {
        close(serverFd);
        return;
    }

    // Set non-blocking for graceful shutdown
    fcntl(serverFd, F_SETFL, O_NONBLOCK);

    while (g_running) {
        fd_set readSet;
        FD_ZERO(&readSet);
        FD_SET(serverFd, &readSet);

        struct timeval timeout;
        timeout.tv_sec = 1;
        timeout.tv_usec = 0;

        int result = select(serverFd + 1, &readSet, nullptr, nullptr, &timeout);
        if (result <= 0) continue;

        int clientFd = accept(serverFd, nullptr, nullptr);
        if (clientFd < 0) continue;

        // Read command
        char buffer[1024];
        ssize_t n = read(clientFd, buffer, sizeof(buffer) - 1);
        if (n > 0) {
            buffer[n] = '\0';
            std::string response = HandleCommand(buffer);
            write(clientFd, response.c_str(), response.size());
        }

        close(clientFd);
    }

    close(serverFd);
    unlink(SOCKET_PATH);
}

// Read initial config file
std::vector<std::pair<std::string, int>> ReadConfig() {
    std::vector<std::pair<std::string, int>> devices;
    std::ifstream file(CONFIG_PATH);

    if (!file.is_open()) {
        devices.push_back({"duomic L", 0});
        devices.push_back({"duomic R", 1});
        return devices;
    }

    std::string line;
    while (std::getline(file, line)) {
        if (line.empty() || line[0] == '#') continue;

        size_t colonPos = line.find(':');
        if (colonPos != std::string::npos) {
            std::string name = line.substr(0, colonPos);
            int channel = std::stoi(line.substr(colonPos + 1));
            if (channel >= 0 && channel < (int)MAX_CHANNELS) {
                devices.push_back({name, channel});
            }
        }
    }

    if (devices.empty()) {
        devices.push_back({"duomic L", 0});
        devices.push_back({"duomic R", 1});
    }

    return devices;
}

std::shared_ptr<aspl::Driver> CreateDuomicDriver()
{
    g_context = std::make_shared<aspl::Context>();
    g_plugin = std::make_shared<aspl::Plugin>(g_context);

    // Connect to shared memory
    g_sharedBuffer.connect();

    // Read initial config and create devices
    auto config = ReadConfig();
    for (const auto& [name, channel] : config) {
        AddVirtualDevice(name, channel);
    }

    // Start IPC thread
    g_ipcThread = std::thread(IPCThread);
    g_ipcThread.detach();

    return std::make_shared<aspl::Driver>(g_context, g_plugin);
}

} // namespace

extern "C" void* DuomicDriverEntryPoint(CFAllocatorRef allocator, CFUUIDRef typeUUID)
{
    if (!CFEqual(typeUUID, kAudioServerPlugInTypeUUID)) {
        return nullptr;
    }

    static std::shared_ptr<aspl::Driver> driver = CreateDuomicDriver();
    return driver->GetReference();
}
