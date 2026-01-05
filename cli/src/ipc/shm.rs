use anyhow::{Context, Result};
use memmap2::MmapMut;
use std::fs::OpenOptions;
use std::sync::atomic::{fence, Ordering};

const SHM_PATH: &str = "/tmp/duomic_audio";
const RING_BUFFER_FRAMES: usize = 8192;
const HEADER_SIZE: usize = 16;

/// Shared memory audio buffer for IPC with the driver
///
/// Memory layout:
/// - Bytes 0-3:   writePos (uint32) - CLI write position
/// - Bytes 4-7:   channelCount (uint32) - Number of channels
/// - Bytes 8-11:  sampleRate (uint32) - Sample rate in Hz
/// - Bytes 12-15: active (uint32) - CLI active flag (0/1)
/// - Bytes 16+:   Audio data (interleaved float samples)
pub struct SharedAudioBuffer {
    mmap: MmapMut,
    channel_count: u32,
    sample_rate: u32,
}

impl SharedAudioBuffer {
    /// Create or open the shared memory buffer
    pub fn open(channel_count: u32, sample_rate: u32) -> Result<Self> {
        let data_size = RING_BUFFER_FRAMES * channel_count as usize * std::mem::size_of::<f32>();
        let total_size = HEADER_SIZE + data_size;

        // Create or open the shared memory file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(SHM_PATH)
            .context("Failed to open shared memory file")?;

        // Set file size
        file.set_len(total_size as u64)
            .context("Failed to set shared memory size")?;

        // Memory map the file
        let mut mmap =
            unsafe { MmapMut::map_mut(&file).context("Failed to memory map shared memory")? };

        // Initialize header
        let header = mmap.as_mut();

        // Write channel count (bytes 4-7)
        header[4..8].copy_from_slice(&channel_count.to_ne_bytes());

        // Write sample rate (bytes 8-11)
        header[8..12].copy_from_slice(&sample_rate.to_ne_bytes());

        // Set active flag (bytes 12-15)
        header[12..16].copy_from_slice(&1u32.to_ne_bytes());

        tracing::debug!(
            "Opened shared memory: {} channels, {} Hz, {} frames",
            channel_count,
            sample_rate,
            RING_BUFFER_FRAMES
        );

        Ok(Self {
            mmap,
            channel_count,
            sample_rate,
        })
    }

    /// Get the write position
    pub fn write_pos(&self) -> u32 {
        let header = self.mmap.as_ref();
        u32::from_ne_bytes([header[0], header[1], header[2], header[3]])
    }

    /// Set the write position
    fn set_write_pos(&mut self, pos: u32) {
        let header = self.mmap.as_mut();
        header[0..4].copy_from_slice(&pos.to_ne_bytes());
    }

    /// Write audio samples to the ring buffer
    ///
    /// `samples` should be interleaved: [ch0, ch1, ch0, ch1, ...]
    ///
    /// IMPORTANT: write_pos is monotonically increasing (wraps at u32::MAX, not at RING_BUFFER_FRAMES)
    /// The driver calculates available samples as: writePos - readPos (unsigned arithmetic)
    /// Buffer indexing uses modulo only when accessing the actual ring buffer data
    pub fn write_samples(&mut self, samples: &[f32]) -> Result<()> {
        let frames = samples.len() / self.channel_count as usize;
        if frames == 0 {
            return Ok(());
        }

        let mut write_pos = self.write_pos();
        let buffer_frames = RING_BUFFER_FRAMES;

        // Get audio data region
        let data_offset = HEADER_SIZE;
        let sample_size = std::mem::size_of::<f32>();
        let frame_size = self.channel_count as usize * sample_size;

        let buffer = self.mmap.as_mut();

        for frame_idx in 0..frames {
            let src_offset = frame_idx * self.channel_count as usize;
            // Use modulo ONLY for buffer indexing, not for position tracking
            let buffer_idx = (write_pos as usize) % buffer_frames;
            let dst_offset = data_offset + buffer_idx * frame_size;

            for ch in 0..self.channel_count as usize {
                let sample = samples[src_offset + ch];
                let bytes = sample.to_ne_bytes();
                let byte_offset = dst_offset + ch * sample_size;
                buffer[byte_offset..byte_offset + sample_size].copy_from_slice(&bytes);
            }

            // Monotonically increase - wraps naturally at u32::MAX (~24 hours at 48kHz)
            write_pos = write_pos.wrapping_add(1);
        }

        // Memory barrier: ensure all audio data writes are visible before updating write_pos
        // This is critical for correct producer-consumer synchronization with the driver
        // The driver must use an Acquire fence when reading write_pos
        fence(Ordering::Release);

        self.set_write_pos(write_pos);
        Ok(())
    }

    /// Set the active flag
    pub fn set_active(&mut self, active: bool) {
        let header = self.mmap.as_mut();
        let value: u32 = if active { 1 } else { 0 };
        header[12..16].copy_from_slice(&value.to_ne_bytes());
    }

    /// Get channel count
    pub fn channel_count(&self) -> u32 {
        self.channel_count
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get ring buffer capacity in frames
    pub fn capacity_frames(&self) -> usize {
        RING_BUFFER_FRAMES
    }
}

impl Drop for SharedAudioBuffer {
    fn drop(&mut self) {
        // Mark as inactive when dropped
        self.set_active(false);
        tracing::debug!("Shared audio buffer closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_buffer_creation() {
        // Skip if we can't create temp files
        if std::fs::metadata("/tmp").is_err() {
            return;
        }

        let result = SharedAudioBuffer::open(2, 48000);
        // May fail in test environment without proper permissions
        if let Ok(buffer) = result {
            assert_eq!(buffer.channel_count(), 2);
            assert_eq!(buffer.sample_rate(), 48000);
        }
    }
}
