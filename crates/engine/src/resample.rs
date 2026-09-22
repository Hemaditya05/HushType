//! Streaming mono resampler to 16 kHz: windowed-sinc low-pass followed by
//! linear interpolation. Costs a few MFLOP/s, which is plenty for speech.

pub const TARGET_RATE: u32 = 16_000;
const TAPS: usize = 31;

pub struct Resampler {
    step: f64,
    /// Position of the next output sample, in `hist` index units.
    pos: f64,
    kernel: Vec<f32>,
    /// Filter history followed by unconsumed input.
    hist: Vec<f32>,
    passthrough: bool,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        let passthrough = input_rate == TARGET_RATE;
        let step = input_rate as f64 / TARGET_RATE as f64;
        // Cut-off just below the output Nyquist (8 kHz), relative to the input rate.
        let fc = (0.45 * TARGET_RATE as f64 / input_rate as f64).min(0.5);
        let m = (TAPS - 1) as f64;
        let pi = std::f64::consts::PI;
        let mut kernel: Vec<f32> = (0..TAPS)
            .map(|i| {
                let x = i as f64 - m / 2.0;
                let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * pi * fc * x).sin() / (pi * x) };
                let blackman = 0.42 - 0.5 * (2.0 * pi * i as f64 / m).cos() + 0.08 * (4.0 * pi * i as f64 / m).cos();
                (sinc * blackman) as f32
            })
            .collect();
        let sum: f32 = kernel.iter().sum();
        kernel.iter_mut().for_each(|k| *k /= sum);
        Resampler { step, pos: (TAPS - 1) as f64, kernel, hist: vec![0.0; TAPS - 1], passthrough }
    }

    fn filtered(&self, idx: usize) -> f32 {
        let start = idx + 1 - TAPS;
        self.hist[start..=idx].iter().zip(&self.kernel).map(|(a, b)| a * b).sum()
    }

    /// Feed input samples and append 16 kHz output to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.hist.extend_from_slice(input);
        while (self.pos as usize) + 1 < self.hist.len() {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = self.filtered(i);
            let b = self.filtered(i + 1);
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        // Drop consumed input but keep the filter history.
        let keep_from = (self.pos as usize).saturating_sub(TAPS - 1);
        if keep_from > 0 {
            self.hist.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_conversion_lengths() {
        for rate in [44_100u32, 48_000, 32_000, 22_050, 16_000] {
            let mut r = Resampler::new(rate);
            let mut out = Vec::new();
            let input = vec![0.1f32; rate as usize];
            for chunk in input.chunks(441) {
                r.process(chunk, &mut out);
            }
            let diff = (out.len() as i64 - 16_000).abs();
            assert!(diff < 40, "rate {rate}: {} samples", out.len());
        }
    }

    #[test]
    fn preserves_speech_band_tone() {
        let rate = 48_000;
        let mut r = Resampler::new(rate);
        let input: Vec<f32> =
            (0..rate).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin()).collect();
        let mut out = Vec::new();
        r.process(&input, &mut out);
        let tail = &out[1000..];
        let rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();
        assert!((rms - 0.707).abs() < 0.03, "rms {rms}");
    }

    #[test]
    fn attenuates_above_nyquist() {
        let rate = 48_000;
        let mut r = Resampler::new(rate);
        let input: Vec<f32> =
            (0..rate).map(|i| (2.0 * std::f32::consts::PI * 12_000.0 * i as f32 / rate as f32).sin()).collect();
        let mut out = Vec::new();
        r.process(&input, &mut out);
        let tail = &out[1000..];
        let rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();
        assert!(rms < 0.05, "aliasing rms {rms}");
    }
}
