//! Minimal WAV reading/writing (test fixtures and optional recordings).

use std::io::{self, Read, Write};
use std::path::Path;

pub struct Wav {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples in [-1, 1].
    pub samples: Vec<f32>,
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

pub fn read(path: &Path) -> io::Result<Wav> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut data)?;
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    let mut pos = 12;
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 0u16, 0u16, 0u16);
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        let end = (body + len).min(data.len());
        if id == b"fmt " && len >= 16 {
            format = u16::from_le_bytes([data[body], data[body + 1]]);
            channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
            rate = u32::from_le_bytes(data[body + 4..body + 8].try_into().unwrap());
            bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
        } else if id == b"data" {
            let pcm = &data[body..end];
            let samples: Vec<f32> = match (format, bits) {
                (1 | 0xFFFE, 16) => pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect(),
                (3, 32) => pcm.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect(),
                _ => return Err(bad("unsupported WAV encoding (use 16-bit PCM or 32-bit float)")),
            };
            if rate == 0 || channels == 0 {
                return Err(bad("missing fmt chunk"));
            }
            return Ok(Wav { sample_rate: rate, channels, samples });
        }
        pos = body + len + (len & 1);
    }
    Err(bad("no data chunk"))
}

/// Downmix + resample a WAV to 16 kHz mono.
pub fn to_16k_mono(w: &Wav) -> Vec<f32> {
    let ch = w.channels as usize;
    let mono: Vec<f32> = w.samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    let mut out = Vec::with_capacity(mono.len() * 16_000 / w.sample_rate as usize + 16);
    crate::resample::Resampler::new(w.sample_rate).process(&mono, &mut out);
    out
}

/// Write mono f32 samples as 16-bit PCM.
pub fn write_mono_16(path: &Path, sample_rate: u32, samples: &[f32]) -> io::Result<()> {
    let mut f = io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = (samples.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&(sample_rate * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    f.flush()
}
