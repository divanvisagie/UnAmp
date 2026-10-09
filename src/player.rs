//! Audio playback: one rodio `Player` on the default output device, fed by
//! a decoder that is opened on a worker thread so a slow or hung network
//! mount never blocks the UI. Decoded samples pass through the equalizer
//! and are then tapped on their way to the device for the visualizer.

use std::collections::VecDeque;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, MixerDeviceSink, Player, SampleRate, Source};

use crate::eq::{EqParams, Equalizer};

/// Mono samples kept for the visualizer — enough for one FFT window.
pub const TAP_CAPACITY: usize = 2048;
/// Samples the tap batches locally before taking the shared lock.
const TAP_BATCH: usize = 256;

/// Most recent decoded audio, downmixed to mono, for the visualizer.
#[derive(Default)]
pub struct TapBuffer {
    pub samples: VecDeque<f32>,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Loading,
    Playing,
    Paused,
}

struct Loaded {
    generation: u64,
    result: Result<Option<Duration>, String>,
}

/// Output-device handle plus playback state for the current track.
pub struct Engine {
    // Dropping the device sink stops all audio, so it lives as long as the engine.
    _device: Option<MixerDeviceSink>,
    player: Arc<Player>,
    tap: Arc<Mutex<TapBuffer>>,
    eq: Arc<EqParams>,
    /// Bumped on every new load; workers holding an older value discard their result.
    generation: Arc<AtomicU64>,
    /// Serializes clear+append across overlapping load workers.
    swap_lock: Arc<Mutex<()>>,
    tx: mpsc::Sender<Loaded>,
    rx: mpsc::Receiver<Loaded>,
    state: PlayState,
    current: Option<PathBuf>,
    duration: Option<Duration>,
    pub error: Option<String>,
    /// Set when a track plays through to its end; taken by the app to advance.
    finished: bool,
}

impl Engine {
    pub fn new(volume: f32, eq: Arc<EqParams>) -> Self {
        let (device, error) = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut device) => {
                device.log_on_drop(false);
                (Some(device), None)
            }
            Err(e) => (None, Some(format!("No audio output: {e}"))),
        };
        let player = match &device {
            Some(device) => Player::connect_new(device.mixer()),
            // No device: an unconnected player still accepts sources, it just never plays them.
            None => Player::new().0,
        };
        player.set_volume(volume);
        let (tx, rx) = mpsc::channel();
        Self {
            _device: device,
            player: Arc::new(player),
            tap: Arc::new(Mutex::new(TapBuffer::default())),
            eq,
            generation: Arc::new(AtomicU64::new(0)),
            swap_lock: Arc::new(Mutex::new(())),
            tx,
            rx,
            state: PlayState::Stopped,
            current: None,
            duration: None,
            error,
            finished: false,
        }
    }

    pub fn tap(&self) -> Arc<Mutex<TapBuffer>> {
        Arc::clone(&self.tap)
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// Track length from the decoder, or from the tags for formats whose
    /// decoder can't tell (e.g. VBR MP3 without a Xing header).
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn position(&self) -> Duration {
        match self.state {
            PlayState::Playing | PlayState::Paused => self.player.get_pos(),
            _ => Duration::ZERO,
        }
    }

    /// Starts playing `path`, replacing whatever is playing. The file is
    /// opened and probed off the UI thread; `duration_hint` (from tags)
    /// is used until the decoder reports its own length.
    pub fn play(&mut self, path: PathBuf, duration_hint: Option<Duration>, ctx: &egui::Context) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.state = PlayState::Loading;
        self.current = Some(path.clone());
        self.duration = duration_hint;
        self.error = None;
        self.finished = false;

        let player = Arc::clone(&self.player);
        let tap = Arc::clone(&self.tap);
        let eq = Arc::clone(&self.eq);
        let current_generation = Arc::clone(&self.generation);
        let swap_lock = Arc::clone(&self.swap_lock);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = open_decoder(&path).map(|decoder| {
                let duration = decoder.total_duration();
                let _guard = swap_lock.lock().unwrap_or_else(|e| e.into_inner());
                if current_generation.load(Ordering::SeqCst) == generation {
                    player.clear();
                    // EQ before the tap, so the visualizer shows what you hear.
                    player.append(Tap::new(Equalizer::new(decoder, eq), tap));
                    player.play();
                }
                duration
            });
            let _ = tx.send(Loaded { generation, result });
            ctx.request_repaint();
        });
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            PlayState::Playing => {
                self.player.pause();
                self.state = PlayState::Paused;
            }
            PlayState::Paused => {
                self.player.play();
                self.state = PlayState::Playing;
            }
            PlayState::Stopped | PlayState::Loading => {}
        }
    }

    pub fn stop(&mut self) {
        // Invalidate any in-flight load so it doesn't start playing afterwards.
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.player.clear();
        self.state = PlayState::Stopped;
        self.finished = false;
    }

    pub fn seek(&mut self, pos: Duration) {
        if !matches!(self.state, PlayState::Playing | PlayState::Paused) {
            return;
        }
        let pos = match self.duration {
            Some(d) => pos.min(d.saturating_sub(Duration::from_millis(250))),
            None => pos,
        };
        if let Err(e) = self.player.try_seek(pos) {
            if !matches!(e, SeekError::NotSupported { .. }) {
                self.error = Some(format!("Seek failed: {e}"));
            }
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }

    /// Applies finished loads and detects end of track. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok(Loaded { generation, result }) = self.rx.try_recv() {
            if generation != self.generation.load(Ordering::SeqCst) {
                continue;
            }
            match result {
                Ok(duration) => {
                    if duration.is_some() {
                        self.duration = duration;
                    }
                    self.state = PlayState::Playing;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.state = PlayState::Stopped;
                    // Let the app skip past an undecodable file.
                    self.finished = true;
                }
            }
        }
        if self.state == PlayState::Playing && self.player.empty() {
            self.state = PlayState::Stopped;
            self.finished = true;
        }
    }

    /// Whether the current track ended (or failed to load) since the last call.
    pub fn take_finished(&mut self) -> bool {
        std::mem::take(&mut self.finished)
    }
}

fn open_decoder(path: &Path) -> Result<Decoder<std::io::BufReader<File>>, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file = File::open(path).map_err(|e| format!("Cannot open {name}: {e}"))?;
    Decoder::try_from(file).map_err(|e| format!("Cannot play {name}: {e}"))
}

/// Passes samples through unchanged while copying a mono downmix into a
/// shared buffer for the visualizer.
struct Tap<S> {
    inner: S,
    shared: Arc<Mutex<TapBuffer>>,
    batch: Vec<f32>,
    frame_sum: f32,
    frame_pos: u16,
}

impl<S: Source> Tap<S> {
    fn new(inner: S, shared: Arc<Mutex<TapBuffer>>) -> Self {
        if let Ok(mut buf) = shared.lock() {
            buf.samples.clear();
        }
        Self {
            inner,
            shared,
            batch: Vec::with_capacity(TAP_BATCH),
            frame_sum: 0.0,
            frame_pos: 0,
        }
    }

    fn flush(&mut self) {
        // Never block the audio thread on the UI; drop this batch if contended.
        if let Ok(mut buf) = self.shared.try_lock() {
            buf.sample_rate = self.inner.sample_rate().get();
            buf.samples.extend(self.batch.iter().copied());
            let excess = buf.samples.len().saturating_sub(TAP_CAPACITY);
            buf.samples.drain(..excess);
        }
        self.batch.clear();
    }
}

impl<S: Source> Iterator for Tap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let channels = self.inner.channels().get();
        self.frame_sum += sample;
        self.frame_pos += 1;
        if self.frame_pos >= channels {
            self.batch.push(self.frame_sum / channels as f32);
            self.frame_sum = 0.0;
            self.frame_pos = 0;
            if self.batch.len() >= TAP_BATCH {
                self.flush();
            }
        }
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for Tap<S> {
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
        self.frame_sum = 0.0;
        self.frame_pos = 0;
        self.inner.try_seek(pos)
    }
}
