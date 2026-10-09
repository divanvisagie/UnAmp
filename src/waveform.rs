//! The Waveform window: the whole track drawn the way a DAW shows it —
//! per-channel lanes of peak and RMS levels, the played part highlighted,
//! a playhead, and click-or-drag to seek (see ADR-0017).
//!
//! The track is decoded on a worker thread into one min/max/RMS summary
//! per `BLOCK_FRAMES` frames, which is drawn as it arrives. Summaries for
//! the last few tracks are kept so replaying doesn't decode again.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Color32, Stroke};
use rodio::{Decoder, Source};

/// Frames summarised per block (~11.6 ms at 44.1 kHz).
pub const BLOCK_FRAMES: usize = 512;
/// Tracks whose waveforms are kept in memory.
const CACHE_TRACKS: usize = 8;
/// Blocks gathered before publishing them to the UI.
const PUBLISH_EVERY: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

impl Peak {
    fn merge(self, other: Peak) -> Peak {
        Peak {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
            rms: self.rms.max(other.rms),
        }
    }
}

/// A track's waveform summary, filled in progressively by the decoder.
#[derive(Debug, Default)]
pub struct Waveform {
    /// 1 for mono, 2 for stereo (further channels aren't drawn).
    pub lanes: usize,
    pub sample_rate: u32,
    /// One entry per block; `[left, right]` (only `[0]` used when mono).
    pub blocks: Vec<[Peak; 2]>,
    pub done: bool,
    pub error: Option<String>,
}

impl Waveform {
    pub fn decoded_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        (self.blocks.len() * BLOCK_FRAMES) as f64 / self.sample_rate as f64
    }

    /// Blocks `[from, to)` merged into one peak per lane, or `None` if none
    /// of that range has been decoded yet.
    fn column(&self, from: usize, to: usize) -> Option<[Peak; 2]> {
        let slice = self.blocks.get(from..to.min(self.blocks.len()))?;
        let (first, rest) = slice.split_first()?;
        Some(rest.iter().fold(*first, |acc, b| [acc[0].merge(b[0]), acc[1].merge(b[1])]))
    }
}

struct Entry {
    path: PathBuf,
    waveform: Arc<Mutex<Waveform>>,
    cancel: Arc<AtomicBool>,
}

/// Starts decodes on demand and keeps the last few results.
#[derive(Default)]
pub struct Waveforms {
    cache: VecDeque<Entry>,
}

impl Waveforms {
    /// The waveform for `path`, starting a decode if it isn't known yet.
    /// Unfinished decodes of other tracks are cancelled and dropped.
    pub fn get(&mut self, path: &Path, ctx: &egui::Context) -> Arc<Mutex<Waveform>> {
        if let Some(i) = self.cache.iter().position(|e| e.path == path) {
            let entry = self.cache.remove(i).expect("index from position");
            let waveform = Arc::clone(&entry.waveform);
            self.cache.push_front(entry);
            return waveform;
        }
        self.cache.retain(|e| {
            let done = lock(&e.waveform).done;
            if !done {
                e.cancel.store(true, Ordering::Relaxed);
            }
            done
        });
        self.cache.truncate(CACHE_TRACKS - 1);

        let waveform = Arc::new(Mutex::new(Waveform::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (out, stop, ctx, file) = (Arc::clone(&waveform), Arc::clone(&cancel), ctx.clone(), path.to_path_buf());
        std::thread::spawn(move || {
            let result = analyze(&file, &out, || stop.load(Ordering::Relaxed), || ctx.request_repaint());
            let mut wf = lock(&out);
            match result {
                Ok(()) => wf.done = true,
                Err(e) => wf.error = Some(e),
            }
            drop(wf);
            ctx.request_repaint();
        });
        self.cache.push_front(Entry { path: path.to_path_buf(), waveform: Arc::clone(&waveform), cancel });
        waveform
    }
}

fn lock(w: &Mutex<Waveform>) -> std::sync::MutexGuard<'_, Waveform> {
    w.lock().unwrap_or_else(|e| e.into_inner())
}

/// Decodes `path` into `out`, publishing blocks as it goes. Stops early
/// (leaving `done` false) when `cancelled()` turns true.
fn analyze(
    path: &Path,
    out: &Mutex<Waveform>,
    cancelled: impl Fn() -> bool,
    published: impl Fn(),
) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = Decoder::try_from(file).map_err(|e| e.to_string())?;
    let channels = decoder.channels().get() as usize;
    {
        let mut wf = lock(out);
        wf.lanes = channels.min(2);
        wf.sample_rate = decoder.sample_rate().get();
    }

    let mut batch = Vec::with_capacity(PUBLISH_EVERY);
    let mut acc = [Accumulator::default(); 2];
    let mut frames = 0;
    'decode: loop {
        for ch in 0..channels {
            let Some(sample) = decoder.next() else {
                break 'decode;
            };
            if ch < 2 {
                acc[ch].add(sample);
            }
        }
        frames += 1;
        if frames == BLOCK_FRAMES {
            batch.push([acc[0].finish(), acc[1].finish()]);
            acc = Default::default();
            frames = 0;
            if batch.len() == PUBLISH_EVERY {
                if cancelled() {
                    return Ok(());
                }
                lock(out).blocks.append(&mut batch);
                published();
            }
        }
    }
    if frames > 0 {
        batch.push([acc[0].finish(), acc[1].finish()]);
    }
    lock(out).blocks.append(&mut batch);
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Accumulator {
    min: f32,
    max: f32,
    sum_sq: f64,
    count: usize,
}

impl Accumulator {
    fn add(&mut self, s: f32) {
        self.min = self.min.min(s);
        self.max = self.max.max(s);
        self.sum_sq += (s as f64) * (s as f64);
        self.count += 1;
    }

    fn finish(&self) -> Peak {
        let rms = if self.count == 0 { 0.0 } else { (self.sum_sq / self.count as f64).sqrt() as f32 };
        Peak { min: self.min, max: self.max, rms }
    }
}

pub struct WaveColors {
    pub background: Color32,
    pub center: Color32,
    pub played: Color32,
    pub unplayed: Color32,
    pub playhead: Color32,
}

/// Draws `wf` across the available space. `total_secs` sets the time scale
/// (the track length, known before decoding finishes); `position` is the
/// playhead. Returns a seek target (seconds) when the user clicks or
/// finishes dragging.
pub fn show(
    ui: &mut egui::Ui,
    wf: &Waveform,
    total_secs: f64,
    position: f64,
    colors: &WaveColors,
) -> Option<f64> {
    let size = ui.available_size().max(egui::vec2(120.0, 60.0));
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, colors.background);
    let total = if total_secs > 0.0 { total_secs } else { wf.decoded_secs().max(1.0) };
    let secs_at = |x: f32| (((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64) * total;

    // While dragging, the playhead follows the pointer; the seek happens on release.
    let drag_id = resp.id.with("drag");
    let mut seek = None;
    let pointer = resp.interact_pointer_pos().map(|p| secs_at(p.x));
    if resp.dragged() {
        ui.data_mut(|d| d.insert_temp(drag_id, pointer));
    }
    let dragging: Option<f64> = ui.data(|d| d.get_temp(drag_id)).flatten();
    if resp.drag_stopped() || resp.clicked() {
        seek = pointer.or(dragging);
        ui.data_mut(|d| d.remove::<Option<f64>>(drag_id));
    }
    let playhead_secs = dragging.unwrap_or(position);
    let playhead_x = rect.left() + (playhead_secs / total).clamp(0.0, 1.0) as f32 * rect.width();

    let lanes = wf.lanes.max(1);
    let gap = 4.0;
    let lane_h = (rect.height() - gap * (lanes - 1) as f32) / lanes as f32;
    let blocks_total = (total * wf.sample_rate as f64 / BLOCK_FRAMES as f64).max(1.0);
    let columns = rect.width().floor() as usize;
    for lane in 0..lanes {
        let top = rect.top() + lane as f32 * (lane_h + gap);
        let mid = top + lane_h / 2.0;
        let half = lane_h / 2.0 - 1.0;
        painter.hline(rect.x_range(), mid, Stroke::new(1.0, colors.center));
        for col in 0..columns {
            let from = (col as f64 / columns as f64 * blocks_total) as usize;
            let to = (((col + 1) as f64 / columns as f64 * blocks_total) as usize).max(from + 1);
            let Some(peaks) = wf.column(from, to) else {
                break;
            };
            let p = peaks[lane];
            let x = rect.left() + col as f32 + 0.5;
            let base = if x < playhead_x { colors.played } else { colors.unplayed };
            let y = |v: f32| mid - v.clamp(-1.0, 1.0) * half;
            // Peak envelope, then the RMS body in a brighter shade on top.
            painter.vline(x, (y(p.max)).min(mid - 0.5)..=(y(p.min)).max(mid + 0.5), Stroke::new(1.0, base.gamma_multiply(0.55)));
            painter.vline(x, y(p.rms)..=y(-p.rms), Stroke::new(1.0, base));
        }
    }

    painter.vline(playhead_x, rect.y_range(), Stroke::new(2.0, colors.playhead));
    if let Some(hover) = resp.hover_pos() {
        painter.vline(hover.x, rect.y_range(), Stroke::new(1.0, colors.playhead.gamma_multiply(0.4)));
        resp.on_hover_text_at_pointer(crate::metadata::format_duration(std::time::Duration::from_secs_f64(secs_at(hover.x))));
    }
    seek
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit PCM WAV: left channel a sine at `amp`, right channel silent.
    fn stereo_wav(frames: usize, amp: f32) -> Vec<u8> {
        let mut data = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let l = ((std::f32::consts::TAU * 440.0 * i as f32 / 44_100.0).sin() * amp * 32767.0) as i16;
            data.extend_from_slice(&l.to_le_bytes());
            data.extend_from_slice(&0i16.to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&2u16.to_le_bytes()); // channels
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        wav
    }

    fn analyzed(name: &str, frames: usize) -> Waveform {
        let path = std::env::temp_dir().join(format!("unamp-wave-{name}-{}.wav", std::process::id()));
        std::fs::write(&path, stereo_wav(frames, 0.5)).unwrap();
        let out = Mutex::new(Waveform::default());
        analyze(&path, &out, || false, || {}).unwrap();
        let _ = std::fs::remove_file(&path);
        out.into_inner().unwrap()
    }

    #[test]
    fn stereo_file_gives_two_lanes_with_their_own_levels() {
        let wf = analyzed("lanes", 44_100);
        assert_eq!(wf.lanes, 2);
        assert_eq!(wf.sample_rate, 44_100);
        assert_eq!(wf.blocks.len(), 44_100usize.div_ceil(BLOCK_FRAMES));
        let whole = wf.column(0, wf.blocks.len()).unwrap();
        assert!((whole[0].max - 0.5).abs() < 0.01, "left peak {}", whole[0].max);
        assert!((whole[0].min + 0.5).abs() < 0.01, "left trough {}", whole[0].min);
        // A sine's RMS is its peak / √2.
        assert!((whole[0].rms - 0.5 / 2f32.sqrt()).abs() < 0.02, "left rms {}", whole[0].rms);
        assert_eq!(whole[1], Peak::default(), "right is silent");
        assert!((wf.decoded_secs() - 1.0).abs() < 0.02);
    }

    #[test]
    fn cancelled_decode_stops_early() {
        let path = std::env::temp_dir().join(format!("unamp-wave-cancel-{}.wav", std::process::id()));
        std::fs::write(&path, stereo_wav(44_100 * 5, 0.5)).unwrap();
        let out = Mutex::new(Waveform::default());
        analyze(&path, &out, || true, || {}).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(out.into_inner().unwrap().blocks.is_empty());
    }

    #[test]
    fn undecodable_file_is_an_error() {
        let path = std::env::temp_dir().join(format!("unamp-wave-bad-{}.mp3", std::process::id()));
        std::fs::write(&path, b"not audio").unwrap();
        let out = Mutex::new(Waveform::default());
        assert!(analyze(&path, &out, || false, || {}).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn columns_beyond_the_decoded_part_are_empty() {
        let wf = Waveform {
            lanes: 1,
            sample_rate: 44_100,
            blocks: vec![[Peak { min: -0.2, max: 0.3, rms: 0.1 }, Peak::default()]; 4],
            ..Default::default()
        };
        assert_eq!(wf.column(1, 3).unwrap()[0].max, 0.3);
        assert!(wf.column(4, 6).is_none());
    }
}
