use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crate::ipc::SharedAudioBuffer;

/// Maximum supported channels (matches driver)
const MAX_CHANNELS: usize = 8;

/// Audio capture state
pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    peak_receiver: Receiver<[f32; MAX_CHANNELS]>,
    channel_count: u16,
    /// Shared write position for UI display (updated by callback)
    write_pos: Arc<AtomicU32>,
}

impl AudioCapture {
    /// Start capturing audio from the specified device
    ///
    /// Takes ownership of SharedAudioBuffer - the callback will own it directly
    /// to avoid mutex locking in the real-time audio thread.
    pub fn start(device: &cpal::Device, shm: SharedAudioBuffer) -> Result<Self> {
        let config = device
            .default_input_config()
            .context("Failed to get default input config")?;

        let channel_count = config.channels();
        let sample_format = config.sample_format();
        let stream_config: StreamConfig = config.into();

        tracing::info!(
            "Starting audio capture: {} channels, {} Hz, {:?}",
            channel_count,
            stream_config.sample_rate.0,
            sample_format
        );

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Atomic write position for UI display
        let write_pos = Arc::new(AtomicU32::new(0));
        let write_pos_clone = write_pos.clone();

        // Channel for sending peak levels to the UI (fixed-size array, no allocation)
        let (peak_sender, peak_receiver) = bounded::<[f32; MAX_CHANNELS]>(16);

        let stream = match sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(
                device,
                &stream_config,
                shm,
                running_clone,
                peak_sender,
                channel_count,
                write_pos_clone,
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                device,
                &stream_config,
                shm,
                running_clone,
                peak_sender,
                channel_count,
                write_pos_clone,
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                device,
                &stream_config,
                shm,
                running_clone,
                peak_sender,
                channel_count,
                write_pos_clone,
            )?,
            _ => anyhow::bail!("Unsupported sample format: {:?}", sample_format),
        };

        stream.play().context("Failed to start audio stream")?;

        Ok(Self {
            stream: Some(stream),
            running,
            peak_receiver,
            channel_count,
            write_pos,
        })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &StreamConfig,
        mut shm: SharedAudioBuffer,
        running: Arc<AtomicBool>,
        peak_sender: Sender<[f32; MAX_CHANNELS]>,
        channel_count: u16,
        write_pos_atomic: Arc<AtomicU32>,
    ) -> Result<cpal::Stream>
    where
        T: cpal::Sample + cpal::SizedSample + Into<f32>,
    {
        let err_fn = |err| {
            // Note: This is an error callback, not the audio callback
            // Logging here is acceptable as errors are rare
            tracing::error!("Audio stream error: {}", err);
        };

        let channels = channel_count as usize;

        // Pre-allocate peak buffer (fixed-size array on stack, no heap allocation)
        let mut peaks = [0.0f32; MAX_CHANNELS];

        // Pre-allocate sample conversion buffer
        // Typical callback size is 256-1024 frames, we allocate for worst case
        let mut sample_buffer: Vec<f32> = Vec::with_capacity(4096 * channels);

        // Frame counter for peak sending
        let mut frame_counter: usize = 0;

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if !running.load(Ordering::Relaxed) {
                        return;
                    }

                    // Reuse pre-allocated buffer (clear + extend avoids reallocation)
                    sample_buffer.clear();
                    sample_buffer.extend(data.iter().map(|s| (*s).into()));

                    // Calculate peak levels per channel (no allocation)
                    for chunk in sample_buffer.chunks(channels) {
                        for (ch, &sample) in chunk.iter().enumerate() {
                            if ch < MAX_CHANNELS {
                                let abs = sample.abs();
                                if abs > peaks[ch] {
                                    peaks[ch] = abs;
                                }
                            }
                        }

                        frame_counter += 1;

                        // Send peaks every ~100 frames (fixed-size array, no clone allocation)
                        if frame_counter >= 100 {
                            let _ = peak_sender.try_send(peaks);
                            peaks = [0.0f32; MAX_CHANNELS];
                            frame_counter = 0;
                        }
                    }

                    // Write to shared memory (no mutex, callback owns shm)
                    // Error handling: silently ignore errors to avoid blocking
                    // The write_pos update will stall, which the driver handles gracefully
                    let _ = shm.write_samples(&sample_buffer);

                    // Update atomic write_pos for UI display
                    write_pos_atomic.store(shm.write_pos(), Ordering::Relaxed);
                },
                err_fn,
                None,
            )
            .context("Failed to build input stream")?;

        Ok(stream)
    }

    /// Get the peak level receiver
    pub fn peak_receiver(&self) -> &Receiver<[f32; MAX_CHANNELS]> {
        &self.peak_receiver
    }

    /// Get channel count
    pub fn channel_count(&self) -> u16 {
        self.channel_count
    }

    /// Get current write position (for UI display)
    pub fn write_pos(&self) -> u32 {
        self.write_pos.load(Ordering::Relaxed)
    }

    /// Stop capturing
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        tracing::info!("Audio capture stopped");
    }

    /// Check if capture is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convert linear amplitude to dB
pub fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        -60.0
    } else {
        20.0 * amplitude.log10()
    }
}

/// Convert dB to linear amplitude
pub fn db_to_amplitude(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_conversion() {
        assert!((amplitude_to_db(1.0) - 0.0).abs() < 0.001);
        assert!((amplitude_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((amplitude_to_db(0.0) - (-60.0)).abs() < 0.001);
    }
}
