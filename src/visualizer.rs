//! Classic bar spectrum analyzer: an FFT of the newest tapped samples,
//! grouped into log-spaced bands, with bars that fall smoothly and peak
//! caps that hang briefly before dropping.

use std::sync::{Arc, Mutex};

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::player::TapBuffer;
use crate::skin::Palette;

pub const BANDS: usize = 19;
const FFT_SIZE: usize = 1024;
const MIN_FREQ: f32 = 40.0;
const MAX_FREQ: f32 = 16_000.0;
/// Level range mapped onto the bar height.
const FLOOR_DB: f32 = -60.0;
/// How far bars and peaks fall per second, as a fraction of full height.
const BAR_FALL: f32 = 2.5;
const PEAK_FALL: f32 = 0.6;
const PEAK_HOLD_SECS: f32 = 0.4;

pub struct Visualizer {
    tap: Arc<Mutex<TapBuffer>>,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    bars: [f32; BANDS],
    peaks: [f32; BANDS],
    peak_age: [f32; BANDS],
}

impl Visualizer {
    pub fn new(tap: Arc<Mutex<TapBuffer>>) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        // Hann window to keep loud bands from smearing into their neighbours.
        let window = (0..FFT_SIZE)
            .map(|i| {
                let x = i as f32 / (FFT_SIZE - 1) as f32;
                0.5 - 0.5 * (std::f32::consts::TAU * x).cos()
            })
            .collect();
        Self {
            tap,
            fft,
            window,
            scratch: vec![Complex::default(); FFT_SIZE],
            bars: [0.0; BANDS],
            peaks: [0.0; BANDS],
            peak_age: [0.0; BANDS],
        }
    }

    /// Advances the animation by `dt` seconds. While `active`, bars rise to
    /// the current spectrum; otherwise they fall back to silence.
    pub fn update(&mut self, dt: f32, active: bool) {
        let target = if active {
            self.spectrum()
        } else {
            None
        }
        .unwrap_or([0.0; BANDS]);

        for i in 0..BANDS {
            self.bars[i] = if target[i] >= self.bars[i] {
                target[i]
            } else {
                (self.bars[i] - BAR_FALL * dt).max(target[i])
            };
            if self.bars[i] >= self.peaks[i] {
                self.peaks[i] = self.bars[i];
                self.peak_age[i] = 0.0;
            } else {
                self.peak_age[i] += dt;
                if self.peak_age[i] > PEAK_HOLD_SECS {
                    self.peaks[i] = (self.peaks[i] - PEAK_FALL * dt).max(self.bars[i]);
                }
            }
        }
    }

    /// Current bar heights and peak positions, 0–1, for other renderers.
    pub fn levels(&self) -> (&[f32; BANDS], &[f32; BANDS]) {
        (&self.bars, &self.peaks)
    }

    /// Whether anything is still moving (so the caller keeps repainting).
    pub fn is_animating(&self) -> bool {
        self.peaks.iter().any(|&p| p > 0.001)
    }

    fn spectrum(&mut self) -> Option<[f32; BANDS]> {
        let sample_rate = {
            let tap = self.tap.lock().ok()?;
            if tap.samples.len() < FFT_SIZE || tap.sample_rate == 0 {
                return None;
            }
            let start = tap.samples.len() - FFT_SIZE;
            for (i, (slot, sample)) in self
                .scratch
                .iter_mut()
                .zip(tap.samples.range(start..))
                .enumerate()
            {
                *slot = Complex::new(sample * self.window[i], 0.0);
            }
            tap.sample_rate as f32
        };
        self.fft.process(&mut self.scratch);

        let bin_hz = sample_rate / FFT_SIZE as f32;
        let nyquist_bin = FFT_SIZE / 2;
        let mut out = [0.0; BANDS];
        for (band, level) in out.iter_mut().enumerate() {
            let (lo, hi) = band_edges(band);
            let lo_bin = ((lo / bin_hz) as usize).clamp(1, nyquist_bin - 1);
            let hi_bin = ((hi / bin_hz) as usize).clamp(lo_bin + 1, nyquist_bin);
            let peak = self.scratch[lo_bin..hi_bin]
                .iter()
                .map(|c| c.norm())
                .fold(0.0f32, f32::max);
            // Normalise so a full-scale sine reads ~0 dB (Hann window halves the gain).
            let magnitude = peak / (FFT_SIZE as f32 / 4.0);
            let db = 20.0 * magnitude.max(1e-9).log10();
            *level = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
        }
        Some(out)
    }

    pub fn show(&self, ui: &mut egui::Ui, size: egui::Vec2, palette: &Palette) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, palette.radius, palette.display);

        let inner = rect.shrink(3.0);
        let gap = 1.0;
        let bar_w = (inner.width() - gap * (BANDS - 1) as f32) / BANDS as f32;
        // Bars are drawn as stacked segments so the skin's low→mid→high
        // gradient stays fixed to height rather than stretching with each bar.
        let segments = 16;
        let seg_h = inner.height() / segments as f32;
        for i in 0..BANDS {
            let x = inner.left() + i as f32 * (bar_w + gap);
            let lit = (self.bars[i] * segments as f32).round() as usize;
            for s in 0..lit {
                let y1 = inner.bottom() - s as f32 * seg_h;
                let seg = egui::Rect::from_min_max(
                    egui::pos2(x, y1 - seg_h + 1.0),
                    egui::pos2(x + bar_w, y1),
                );
                painter.rect_filled(seg, 0.0, palette.spectrum(s as f32 / (segments - 1) as f32));
            }
            if self.peaks[i] > 0.001 {
                let y = inner.bottom() - self.peaks[i] * inner.height();
                let cap = egui::Rect::from_min_max(
                    egui::pos2(x, y - 2.0),
                    egui::pos2(x + bar_w, y),
                );
                painter.rect_filled(cap, 0.0, palette.spectrum_peak);
            }
        }
        response
    }
}

/// Frequency range of a band on a log scale between `MIN_FREQ` and `MAX_FREQ`.
fn band_edges(band: usize) -> (f32, f32) {
    let ratio = MAX_FREQ / MIN_FREQ;
    let lo = MIN_FREQ * ratio.powf(band as f32 / BANDS as f32);
    let hi = MIN_FREQ * ratio.powf((band + 1) as f32 / BANDS as f32);
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_cover_range_contiguously() {
        assert!((band_edges(0).0 - MIN_FREQ).abs() < 0.01);
        assert!((band_edges(BANDS - 1).1 - MAX_FREQ).abs() < 1.0);
        for b in 1..BANDS {
            assert!((band_edges(b).0 - band_edges(b - 1).1).abs() < 0.01);
        }
    }

    #[test]
    fn sine_lights_its_own_band_most() {
        let tap = Arc::new(Mutex::new(TapBuffer::default()));
        {
            let mut t = tap.lock().unwrap();
            t.sample_rate = 44_100;
            t.samples = (0..FFT_SIZE)
                .map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / 44_100.0).sin())
                .collect();
        }
        let mut vis = Visualizer::new(tap);
        let spectrum = vis.spectrum().unwrap();
        let loudest = (0..BANDS)
            .max_by(|&a, &b| spectrum[a].total_cmp(&spectrum[b]))
            .unwrap();
        let (lo, hi) = band_edges(loudest);
        assert!(lo <= 1000.0 && 1000.0 < hi, "1 kHz landed in {lo}..{hi}");
        assert!(spectrum[loudest] > 0.9);
    }

    #[test]
    fn bars_fall_back_when_inactive() {
        let mut vis = Visualizer::new(Arc::new(Mutex::new(TapBuffer::default())));
        vis.bars = [1.0; BANDS];
        vis.peaks = [1.0; BANDS];
        for _ in 0..200 {
            vis.update(0.05, false);
        }
        assert!(!vis.is_animating());
    }
}
