//! Short, quiet feedback tones synthesized in memory (no audio assets).

use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

use crate::Sound;

const RATE: u32 = 22_050;

fn tone_wav(notes: &[(f32, f32)], volume: f32) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    for (freq, ms) in notes {
        let n = (RATE as f32 * ms / 1000.0) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            // Soft attack/release envelope to avoid clicks.
            let env = ((i as f32 / (RATE as f32 * 0.008)).min(1.0)) * (((n - i) as f32 / (RATE as f32 * 0.03)).min(1.0));
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.8 + (4.0 * std::f32::consts::PI * freq * t).sin() * 0.2;
            samples.push((s * env * volume * i16::MAX as f32) as i16);
        }
    }
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

fn wav(sound: Sound) -> &'static [u8] {
    static START: OnceLock<Vec<u8>> = OnceLock::new();
    static STOP: OnceLock<Vec<u8>> = OnceLock::new();
    static ERROR: OnceLock<Vec<u8>> = OnceLock::new();
    match sound {
        Sound::Start => START.get_or_init(|| tone_wav(&[(784.0, 55.0), (1175.0, 80.0)], 0.18)),
        Sound::Stop => STOP.get_or_init(|| tone_wav(&[(1175.0, 55.0), (784.0, 80.0)], 0.15)),
        Sound::Error => ERROR.get_or_init(|| tone_wav(&[(330.0, 90.0), (262.0, 140.0)], 0.2)),
    }
}

/// Play asynchronously; returns immediately.
pub fn play(sound: Sound) {
    let data = wav(sound);
    unsafe {
        let _ = PlaySoundW(PCWSTR(data.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
