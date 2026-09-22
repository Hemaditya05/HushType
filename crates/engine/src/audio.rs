//! Microphone capture. The stream exists only while recording: nothing is
//! opened, buffered or processed while the app is idle.
//!
//! The device callback only down-mixes to mono and hands small chunks to a
//! bounded channel; resampling and VAD run on the dictation thread.

use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde::Serialize;

use crate::error::EngineError;
use crate::resample::Resampler;

/// Bound on queued device chunks (~5 s at 10 ms chunks). If the consumer
/// stalls we drop audio rather than grow memory.
const QUEUE_CHUNKS: usize = 512;

#[derive(Debug, Clone, Serialize)]
pub struct InputDevice {
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDevice>, EngineError> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().map(|d| d.to_string());
    let devices = host.input_devices().map_err(|e| EngineError::MicFailed(e.to_string()))?;
    let mut out: Vec<InputDevice> = devices
        .map(|d| {
            let name = d.to_string();
            InputDevice { is_default: Some(&name) == default_name.as_ref(), name }
        })
        .collect();
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

fn map_err(e: cpal::Error) -> EngineError {
    use cpal::ErrorKind as K;
    let text = e.to_string();
    match e.kind() {
        K::PermissionDenied => EngineError::MicPermissionDenied,
        K::DeviceBusy => EngineError::MicBusy,
        K::DeviceNotAvailable => EngineError::MicNotFound,
        _ if text.contains("0x80070005") || text.to_lowercase().contains("access is denied") => {
            EngineError::MicPermissionDenied
        }
        _ if text.contains("0x88890004") => EngineError::MicNotFound, // AUDCLNT_E_DEVICE_INVALIDATED
        _ if text.contains("0x8889000A") => EngineError::MicBusy,     // AUDCLNT_E_DEVICE_IN_USE
        _ => EngineError::MicFailed(text),
    }
}

/// Running capture. Dropping it stops the microphone immediately.
pub struct Capture {
    _stream: Option<cpal::Stream>,
    rx: Receiver<Vec<f32>>,
    resampler: Resampler,
    stop_file: Option<Arc<AtomicBool>>,
    /// Time from open to the first delivered audio buffer.
    pub first_audio: Option<Duration>,
    opened_at: Instant,
    pub device_name: String,
    pub device_rate: u32,
    failed: Arc<AtomicBool>,
}

impl Capture {
    /// Open the named device (or the default one when `None` / not found).
    pub fn open(device: Option<&str>) -> Result<Capture, EngineError> {
        if let Ok(path) = std::env::var("HUSHTYPE_TEST_AUDIO") {
            if !path.is_empty() {
                return Capture::open_file(PathBuf::from(path));
            }
        }
        let opened_at = Instant::now();
        let host = cpal::default_host();
        let dev = match device.filter(|d| !d.is_empty()) {
            Some(want) => host
                .input_devices()
                .map_err(map_err)?
                .find(|d| d.to_string() == want)
                .or_else(|| {
                    log::warn!("configured microphone not found, using default");
                    host.default_input_device()
                }),
            None => host.default_input_device(),
        }
        .ok_or(EngineError::MicNotFound)?;
        let device_name = dev.to_string();
        let supported = dev.default_input_config().map_err(map_err)?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate();
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.config();
        let (tx, rx) = sync_channel::<Vec<f32>>(QUEUE_CHUNKS);
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&dev, config, channels, tx, failed.clone()),
            SampleFormat::I16 => build::<i16>(&dev, config, channels, tx, failed.clone()),
            SampleFormat::I32 => build::<i32>(&dev, config, channels, tx, failed.clone()),
            SampleFormat::U16 => build::<u16>(&dev, config, channels, tx, failed.clone()),
            SampleFormat::U8 => build::<u8>(&dev, config, channels, tx, failed.clone()),
            SampleFormat::F64 => build::<f64>(&dev, config, channels, tx, failed.clone()),
            other => return Err(EngineError::MicFailed(format!("unsupported sample format {other:?}"))),
        }?;
        stream.play().map_err(map_err)?;
        log::info!("microphone open: {channels} ch @ {rate} Hz ({format:?})");
        Ok(Capture {
            _stream: Some(stream),
            rx,
            resampler: Resampler::new(rate),
            stop_file: None,
            first_audio: None,
            opened_at,
            device_name,
            device_rate: rate,
            failed,
        })
    }

    /// Test/benchmark source: plays a WAV file in real time, then silence.
    fn open_file(path: PathBuf) -> Result<Capture, EngineError> {
        let opened_at = Instant::now();
        let wav = crate::wav::read(&path).map_err(|e| EngineError::MicFailed(format!("test audio: {e}")))?;
        let (tx, rx) = sync_channel::<Vec<f32>>(QUEUE_CHUNKS);
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let rate = wav.sample_rate;
        std::thread::Builder::new()
            .name("test-audio".into())
            .spawn(move || {
                let ch = wav.channels as usize;
                let per_chunk = (rate as usize / 100) * ch; // 10 ms
                let start = Instant::now();
                let mut sent = 0usize;
                let mut iter = wav.samples.chunks(per_chunk);
                while !stop2.load(Ordering::Relaxed) {
                    let chunk: Vec<f32> = match iter.next() {
                        Some(c) => c.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect(),
                        None => vec![0.0; rate as usize / 100],
                    };
                    if tx.send(chunk).is_err() {
                        break;
                    }
                    sent += 1;
                    let due = start + Duration::from_millis(sent as u64 * 10);
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                }
            })
            .map_err(|e| EngineError::MicFailed(e.to_string()))?;
        Ok(Capture {
            _stream: None,
            rx,
            resampler: Resampler::new(rate),
            stop_file: Some(stop),
            first_audio: None,
            opened_at,
            device_name: format!("Test audio: {}", path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
            device_rate: rate,
            failed: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Wait up to `timeout` for audio; appends 16 kHz mono samples to `out`.
    /// Returns Err if the device failed (unplugged etc.).
    pub fn read(&mut self, out: &mut Vec<f32>, timeout: Duration) -> Result<usize, EngineError> {
        if self.failed.load(Ordering::Relaxed) {
            return Err(EngineError::MicFailed("device stopped delivering audio".into()));
        }
        let before = out.len();
        match self.rx.recv_timeout(timeout) {
            Ok(chunk) => {
                if self.first_audio.is_none() {
                    self.first_audio = Some(self.opened_at.elapsed());
                }
                self.resampler.process(&chunk, out);
                while let Ok(more) = self.rx.try_recv() {
                    self.resampler.process(&more, out);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(EngineError::MicFailed("audio stream closed".into()))
            }
        }
        Ok(out.len() - before)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop_file {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

fn build<T>(
    dev: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    tx: SyncSender<Vec<f32>>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let mut dropped = 0u64;
    dev.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            let mono: Vec<f32> = if channels == 1 {
                data.iter().map(|s| f32::from_sample_(*s)).collect()
            } else {
                data.chunks(channels)
                    .map(|f| f.iter().map(|s| f32::from_sample_(*s)).sum::<f32>() / channels as f32)
                    .collect()
            };
            match tx.try_send(mono) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    dropped += 1;
                    if dropped % 100 == 1 {
                        log::warn!("audio queue full, dropped {dropped} chunks");
                    }
                }
                Err(TrySendError::Disconnected(_)) => {}
            }
        },
        move |e| {
            use cpal::ErrorKind as K;
            match e.kind() {
                // Glitches and automatic re-routing are recoverable.
                K::Xrun | K::DeviceChanged | K::RealtimeDenied => log::debug!("audio stream notice: {e}"),
                _ => {
                    log::error!("audio stream error: {e}");
                    failed.store(true, Ordering::Relaxed);
                }
            }
        },
        Some(Duration::from_secs(3)),
    )
    .map_err(map_err)
}
