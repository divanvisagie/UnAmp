//! 10-band graphic equalizer: one peaking biquad per band (RBJ audio EQ
//! cookbook) at Winamp's classic centre frequencies, plus a preamp. The UI
//! writes settings into atomics; the audio thread picks them up between
//! samples without locking.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};

pub const BANDS: usize = 10;
pub const FREQS: [f32; BANDS] = [
    60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0,
];
pub const LABELS: [&str; BANDS] = ["60", "170", "310", "600", "1K", "3K", "6K", "12K", "14K", "16K"];
/// Slider range for bands and preamp, in dB.
pub const MAX_DB: f32 = 12.0;
/// Bandwidth of each peak; wide enough that neighbouring bands blend.
const Q: f32 = 1.2;

pub const PRESETS: &[(&str, [f32; BANDS])] = &[
    ("Flat", [0.0; BANDS]),
    ("Rock", [8.0, 4.8, -5.6, -8.0, -3.2, 4.0, 8.8, 11.2, 11.2, 11.2]),
    ("Pop", [-1.6, 4.8, 7.2, 8.0, 5.6, 0.0, -2.4, -2.4, -1.6, -1.6]),
    ("Dance", [9.6, 7.2, 2.4, 0.0, 0.0, -5.6, -7.2, -7.2, 0.0, 0.0]),
    ("Classical", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -7.2, -7.2, -7.2, -9.6]),
    ("Full Bass", [9.6, 9.6, 9.6, 5.6, 1.6, -4.0, -8.0, -10.4, -11.2, -11.2]),
    ("Full Treble", [-9.6, -9.6, -9.6, -4.0, 2.4, 11.2, 12.0, 12.0, 12.0, 12.0]),
    ("Vocal", [-3.2, -4.0, -2.4, 1.6, 5.6, 5.6, 4.0, 1.6, 0.0, -3.2]),
];

/// Settings shared between the UI and the audio thread.
pub struct EqParams {
    enabled: AtomicBool,
    preamp: AtomicU32,
    gains: [AtomicU32; BANDS],
    /// Bumped on every change so the audio thread knows to recompute.
    version: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqSnapshot {
    pub enabled: bool,
    pub preamp: f32,
    pub gains: [f32; BANDS],
}

impl EqParams {
    pub fn new(enabled: bool, preamp: f32, gains: [f32; BANDS]) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(enabled),
            preamp: AtomicU32::new(preamp.to_bits()),
            gains: gains.map(|g| AtomicU32::new(g.to_bits())),
            version: AtomicU64::new(0),
        })
    }

    pub fn set(&self, snapshot: EqSnapshot) {
        self.enabled.store(snapshot.enabled, Ordering::Relaxed);
        self.preamp.store(snapshot.preamp.to_bits(), Ordering::Relaxed);
        for (slot, gain) in self.gains.iter().zip(snapshot.gains) {
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    pub fn snapshot(&self) -> EqSnapshot {
        EqSnapshot {
            enabled: self.enabled.load(Ordering::Relaxed),
            preamp: f32::from_bits(self.preamp.load(Ordering::Relaxed)),
            gains: std::array::from_fn(|i| f32::from_bits(self.gains[i].load(Ordering::Relaxed))),
        }
    }

    fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coeffs {
    /// RBJ peaking EQ, normalised so a0 = 1. `None` for a flat band or one
    /// too close to Nyquist to be stable at this sample rate.
    fn peaking(freq: f32, gain_db: f32, sample_rate: f32) -> Option<Self> {
        if gain_db.abs() < 0.05 || freq >= sample_rate * 0.45 {
            return None;
        }
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * Q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha / a;
        Some(Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        })
    }

    /// Magnitude response at `freq`, for drawing the curve.
    fn magnitude(&self, freq: f32, sample_rate: f32) -> f32 {
        let w = std::f32::consts::TAU * freq / sample_rate;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).sqrt()
    }
}

/// Transposed direct form II state for one band of one channel.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    z1: f32,
    z2: f32,
}

impl State {
    #[inline]
    fn process(&mut self, c: &Coeffs, x: f32) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Overall response in dB at `freq` for these settings, as the UI curve.
pub fn response_db(snapshot: &EqSnapshot, freq: f32, sample_rate: f32) -> f32 {
    let mut mag = 10f32.powf(snapshot.preamp / 20.0);
    for (band_freq, gain) in FREQS.iter().zip(snapshot.gains) {
        if let Some(c) = Coeffs::peaking(*band_freq, gain, sample_rate) {
            mag *= c.magnitude(freq, sample_rate);
        }
    }
    20.0 * mag.max(1e-6).log10()
}

/// A `Source` that runs its input through the equalizer.
pub struct Equalizer<S> {
    inner: S,
    params: Arc<EqParams>,
    seen_version: u64,
    active: bool,
    preamp: f32,
    /// Active bands only, so flat bands cost nothing.
    coeffs: Vec<Coeffs>,
    /// `states[channel][band]`, parallel to `coeffs`.
    states: Vec<Vec<State>>,
    channel: usize,
    channels: u16,
    sample_rate: u32,
}

impl<S: Source> Equalizer<S> {
    pub fn new(inner: S, params: Arc<EqParams>) -> Self {
        let mut eq = Self {
            channels: inner.channels().get(),
            sample_rate: inner.sample_rate().get(),
            inner,
            params,
            seen_version: u64::MAX,
            active: false,
            preamp: 1.0,
            coeffs: Vec::new(),
            states: Vec::new(),
            channel: 0,
        };
        eq.reconfigure();
        eq
    }

    fn reconfigure(&mut self) {
        self.seen_version = self.params.version();
        let snap = self.params.snapshot();
        self.channels = self.inner.channels().get();
        self.sample_rate = self.inner.sample_rate().get();
        self.active = snap.enabled;
        self.preamp = 10f32.powf(snap.preamp / 20.0);
        let new_coeffs: Vec<Coeffs> = FREQS
            .iter()
            .zip(snap.gains)
            .filter_map(|(f, g)| Coeffs::peaking(*f, g, self.sample_rate as f32))
            .collect();
        // Keep filter memory across small slider moves to avoid clicks;
        // reset only when the band layout or channel count changes.
        if new_coeffs.len() != self.coeffs.len() || self.states.len() != self.channels as usize {
            self.states = vec![vec![State::default(); new_coeffs.len()]; self.channels as usize];
        }
        self.coeffs = new_coeffs;
        self.channel = 0;
    }
}

impl<S: Source> Iterator for Equalizer<S> {
    type Item = rodio::Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Only pick up new settings on a frame boundary so channels stay aligned.
        if self.channel == 0
            && (self.params.version() != self.seen_version
                || self.inner.channels().get() != self.channels
                || self.inner.sample_rate().get() != self.sample_rate)
        {
            self.reconfigure();
        }
        let x = self.inner.next()?;
        let channel = self.channel;
        self.channel = (self.channel + 1) % self.channels as usize;
        if !self.active {
            return Some(x);
        }
        let mut y = x * self.preamp;
        if let Some(states) = self.states.get_mut(channel) {
            for (state, c) in states.iter_mut().zip(&self.coeffs) {
                y = state.process(c, y);
            }
        }
        Some(y)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for Equalizer<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        for states in &mut self.states {
            states.fill(State::default());
        }
        self.channel = 0;
        self.inner.try_seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44_100.0;

    fn snap(gains: [f32; BANDS]) -> EqSnapshot {
        EqSnapshot {
            enabled: true,
            preamp: 0.0,
            gains,
        }
    }

    #[test]
    fn flat_eq_is_unity() {
        let s = snap([0.0; BANDS]);
        for f in [50.0, 1_000.0, 10_000.0] {
            assert!(response_db(&s, f, SR).abs() < 0.01);
        }
    }

    #[test]
    fn boosted_band_peaks_at_its_centre() {
        let mut gains = [0.0; BANDS];
        gains[4] = 12.0; // 1 kHz
        let s = snap(gains);
        assert!((response_db(&s, 1_000.0, SR) - 12.0).abs() < 0.1);
        assert!(response_db(&s, 60.0, SR) < 1.0);
        assert!(response_db(&s, 16_000.0, SR) < 1.0);
    }

    #[test]
    fn preamp_shifts_whole_curve() {
        let s = EqSnapshot {
            enabled: true,
            preamp: -6.0,
            gains: [0.0; BANDS],
        };
        assert!((response_db(&s, 440.0, SR) + 6.0).abs() < 0.01);
    }

    #[test]
    fn bands_near_nyquist_are_skipped() {
        assert!(Coeffs::peaking(16_000.0, 6.0, 32_000.0).is_none());
        assert!(Coeffs::peaking(16_000.0, 6.0, 44_100.0).is_some());
    }

    #[test]
    fn filter_applies_gain_to_a_sine() {
        let c = Coeffs::peaking(1_000.0, 12.0, SR).unwrap();
        let mut st = State::default();
        let mut peak: f32 = 0.0;
        for i in 0..44_100 {
            let x = (std::f32::consts::TAU * 1_000.0 * i as f32 / SR).sin();
            let y = st.process(&c, x);
            if i > 4_410 {
                peak = peak.max(y.abs());
            }
        }
        // +12 dB ≈ ×3.98
        assert!((peak - 3.98).abs() < 0.05, "peak {peak}");
    }

    #[test]
    fn presets_stay_in_slider_range() {
        for (name, gains) in PRESETS {
            assert!(gains.iter().all(|g| g.abs() <= MAX_DB), "{name}");
        }
    }

    #[test]
    fn params_round_trip() {
        let p = EqParams::new(false, 0.0, [0.0; BANDS]);
        let s = EqSnapshot {
            enabled: true,
            preamp: -3.0,
            gains: std::array::from_fn(|i| i as f32),
        };
        p.set(s);
        assert_eq!(p.snapshot(), s);
    }
}
