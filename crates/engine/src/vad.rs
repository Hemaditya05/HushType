//! Lightweight energy-based voice activity detection.
//!
//! Works on 20 ms frames of 16 kHz audio. The noise floor is a low percentile
//! of the last ~3 s of frame energies, so it adapts to steady background noise
//! (fans, hum) while speech — which is bursty — stays well above it. Only runs
//! while recording; costs well under 0.1% CPU.

pub const FRAME: usize = 320; // 20 ms @ 16 kHz
const FRAME_MS: u32 = 20;
const HISTORY: usize = 150; // 3 s of frames
const ONSET_FRAMES: u32 = 3; // 60 ms above threshold to start speech
const HANGOVER_FRAMES: u32 = 15; // 300 ms to end speech

pub struct Vad {
    ratio: f32,
    abs_min: f32,
    history: Vec<f32>,
    hist_pos: usize,
    pending: Vec<f32>,
    above: u32,
    below: u32,
    in_speech: bool,
    pub speech_ms: u32,
    /// Milliseconds of non-speech since the last speech frame (0 before any speech).
    pub silence_ms: u32,
    pub ever_speech: bool,
    /// Last frame level in dBFS (for meters).
    pub level_db: f32,
    /// Sample index (in the stream) where speech was last seen ending.
    pub samples_seen: usize,
    pub first_speech_sample: Option<usize>,
    pub last_speech_sample: usize,
}

impl Vad {
    /// `sensitivity` in [0, 1]: higher picks up quieter speech.
    pub fn new(sensitivity: f32) -> Self {
        let s = sensitivity.clamp(0.0, 1.0);
        // Threshold relative to the noise floor: +16 dB (low) .. +6 dB (high).
        let ratio_db = 16.0 - 10.0 * s;
        // Absolute floor: -40 dBFS (low) .. -62 dBFS (high).
        let abs_db = -40.0 - 22.0 * s;
        Vad {
            ratio: 10f32.powf(ratio_db / 20.0),
            abs_min: 10f32.powf(abs_db / 20.0),
            history: Vec::with_capacity(HISTORY),
            hist_pos: 0,
            pending: Vec::with_capacity(FRAME),
            above: 0,
            below: 0,
            in_speech: false,
            speech_ms: 0,
            silence_ms: 0,
            ever_speech: false,
            level_db: -100.0,
            samples_seen: 0,
            first_speech_sample: None,
            last_speech_sample: 0,
        }
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    fn noise_floor(&self) -> f32 {
        if self.history.len() < 10 {
            return 10f32.powf(-60.0 / 20.0);
        }
        let mut v = self.history.clone();
        let k = v.len() / 7; // ~15th percentile
        let (_, x, _) = v.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
        *x
    }

    fn frame(&mut self, f: &[f32]) {
        let rms = (f.iter().map(|x| x * x).sum::<f32>() / f.len() as f32).sqrt().max(1e-7);
        self.level_db = 20.0 * rms.log10();
        let floor = self.noise_floor();
        if self.history.len() < HISTORY {
            self.history.push(rms);
        } else {
            self.history[self.hist_pos] = rms;
            self.hist_pos = (self.hist_pos + 1) % HISTORY;
        }
        let threshold = (floor * self.ratio).max(self.abs_min);
        let loud = rms > threshold;
        if loud {
            self.above += 1;
            self.below = 0;
        } else {
            self.below += 1;
            self.above = 0;
        }
        if !self.in_speech && self.above >= ONSET_FRAMES {
            self.in_speech = true;
            if self.first_speech_sample.is_none() {
                self.first_speech_sample = Some(self.samples_seen.saturating_sub(ONSET_FRAMES as usize * FRAME));
            }
            self.ever_speech = true;
        } else if self.in_speech && self.below >= HANGOVER_FRAMES {
            self.in_speech = false;
        }
        self.samples_seen += f.len();
        if self.in_speech {
            self.speech_ms += FRAME_MS;
            self.silence_ms = 0;
            self.last_speech_sample = self.samples_seen;
        } else if self.ever_speech {
            self.silence_ms += FRAME_MS;
        }
    }

    /// Feed 16 kHz mono samples of any length.
    pub fn push(&mut self, samples: &[f32]) {
        let mut s = samples;
        if !self.pending.is_empty() {
            let need = FRAME - self.pending.len();
            let take = need.min(s.len());
            self.pending.extend_from_slice(&s[..take]);
            s = &s[take..];
            if self.pending.len() == FRAME {
                let p = std::mem::take(&mut self.pending);
                self.frame(&p);
                self.pending = p;
                self.pending.clear();
            }
        }
        let mut chunks = s.chunks_exact(FRAME);
        for f in &mut chunks {
            self.frame(f);
        }
        self.pending.extend_from_slice(chunks.remainder());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(ms: usize, amp: f32) -> Vec<f32> {
        (0..ms * 16).map(|i| amp * (i as f32 * 0.2).sin() * (1.0 + 0.5 * (i as f32 * 0.003).sin())).collect()
    }

    fn noise(ms: usize, amp: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..ms * 16)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                amp * ((x >> 8) as f32 / (1 << 24) as f32 - 0.5)
            })
            .collect()
    }

    #[test]
    fn detects_speech_and_silence() {
        let mut v = Vad::new(0.5);
        v.push(&noise(1000, 0.002));
        assert!(!v.ever_speech);
        v.push(&tone(800, 0.2));
        assert!(v.in_speech());
        assert!(v.speech_ms >= 600);
        v.push(&noise(1200, 0.002));
        assert!(!v.in_speech());
        assert!(v.silence_ms >= 800);
    }

    #[test]
    fn adapts_to_steady_noise() {
        let mut v = Vad::new(0.5);
        // Loud steady fan noise shouldn't count as speech forever.
        v.push(&noise(4000, 0.05));
        v.push(&noise(1000, 0.05));
        assert!(!v.in_speech());
        v.push(&tone(600, 0.5));
        assert!(v.in_speech());
    }
}
