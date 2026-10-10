//! MPRIS: exposes UnAmp on the session D-Bus as
//! `org.mpris.MediaPlayer2.unamp`, which is how GNOME's media controls,
//! media keys, headset buttons and `playerctl` find and drive it
//! (see ADR-0016).
//!
//! D-Bus lives entirely on its own thread. The app sends it state
//! snapshots and reads commands from a channel, so a slow or missing bus
//! never blocks the UI. With no session bus the thread logs once and ends,
//! and UnAmp runs as before.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::command::Command as PlayerCommand;
use crate::config::Repeat;
use crate::player::PlayState;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.unamp";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// What a D-Bus client asked UnAmp to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    /// Relative seek in microseconds.
    Seek(i64),
    /// Absolute position in microseconds, for the given track id.
    SetPosition(String, i64),
    Raise,
    Quit,
    SetVolume(f64),
    SetShuffle(bool),
    SetRepeat(Repeat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Playing => "Playing",
            Status::Paused => "Paused",
            Status::Stopped => "Stopped",
        }
    }
}

/// Everything MPRIS reports, except the position (see `Clock`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    pub status: Status,
    /// D-Bus object path identifying the track; changes with every new track.
    pub track_id: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub length_us: Option<i64>,
    pub art_url: Option<String>,
    pub volume: f64,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play: bool,
    pub can_seek: bool,
}

impl State {
    fn loop_status(&self) -> &'static str {
        match self.repeat {
            Repeat::Off => "None",
            Repeat::One => "Track",
            Repeat::All => "Playlist",
        }
    }

    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        let mut put = |key: &str, value: Value<'_>| {
            if let Ok(owned) = OwnedValue::try_from(value) {
                map.insert(key.to_string(), owned);
            }
        };
        let id = self.track_id.as_deref().unwrap_or(NO_TRACK);
        if let Ok(path) = ObjectPath::try_from(id) {
            put("mpris:trackid", Value::from(path));
        }
        if self.track_id.is_none() {
            return map;
        }
        if let Some(t) = &self.title {
            put("xesam:title", Value::from(t.as_str()));
        }
        if let Some(a) = &self.artist {
            put("xesam:artist", Value::from(vec![a.as_str()]));
        }
        if let Some(a) = &self.album {
            put("xesam:album", Value::from(a.as_str()));
        }
        if let Some(l) = self.length_us {
            put("mpris:length", Value::from(l));
        }
        if let Some(u) = &self.art_url {
            put("mpris:artUrl", Value::from(u.as_str()));
        }
        map
    }
}

/// Playback position that keeps advancing between app frames, so a client
/// reading `Position` while UnAmp is hidden still gets the right answer.
#[derive(Debug, Clone, Copy)]
struct Clock {
    base_us: i64,
    at: Instant,
    running: bool,
}

impl Clock {
    fn now_us(&self) -> i64 {
        if self.running {
            self.base_us + self.at.elapsed().as_micros() as i64
        } else {
            self.base_us
        }
    }
}

struct Shared {
    state: State,
    clock: Clock,
}

type SharedRef = Arc<Mutex<Shared>>;

fn lock(shared: &SharedRef) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sends a command to the app and wakes it.
#[derive(Clone)]
struct Remote {
    tx: mpsc::Sender<Command>,
    ctx: egui::Context,
}

impl Remote {
    fn send(&self, command: Command) {
        let _ = self.tx.send(command);
        self.ctx.request_repaint();
    }
}

struct Root {
    remote: Remote,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        self.remote.send(Command::Raise);
    }

    fn quit(&self) {
        self.remote.send(Command::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "UnAmp".into()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "unamp".into()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    remote: Remote,
    shared: SharedRef,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.remote.send(Command::Next);
    }

    fn previous(&self) {
        self.remote.send(Command::Previous);
    }

    fn pause(&self) {
        self.remote.send(Command::Pause);
    }

    fn play_pause(&self) {
        self.remote.send(Command::PlayPause);
    }

    fn stop(&self) {
        self.remote.send(Command::Stop);
    }

    fn play(&self) {
        self.remote.send(Command::Play);
    }

    fn seek(&self, offset: i64) {
        self.remote.send(Command::Seek(offset));
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        self.remote.send(Command::SetPosition(track_id.to_string(), position));
    }

    fn open_uri(&self, _uri: &str) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported("UnAmp doesn't open URIs".into()))
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        lock(&self.shared).state.status.as_str().into()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        lock(&self.shared).state.loop_status().into()
    }

    #[zbus(property)]
    fn set_loop_status(&mut self, value: String) {
        let repeat = match value.as_str() {
            "Track" => Repeat::One,
            "Playlist" => Repeat::All,
            _ => Repeat::Off,
        };
        self.remote.send(Command::SetRepeat(repeat));
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn set_rate(&mut self, _value: f64) {}

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        lock(&self.shared).state.shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&mut self, value: bool) {
        self.remote.send(Command::SetShuffle(value));
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        lock(&self.shared).state.metadata()
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        lock(&self.shared).state.volume
    }

    #[zbus(property)]
    fn set_volume(&mut self, value: f64) {
        self.remote.send(Command::SetVolume(value.clamp(0.0, 1.0)));
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        lock(&self.shared).clock.now_us()
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        lock(&self.shared).state.can_go_next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        lock(&self.shared).state.can_go_previous
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        lock(&self.shared).state.can_play
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        lock(&self.shared).state.track_id.is_some()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        lock(&self.shared).state.can_seek
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

enum Update {
    State(State),
    Seeked(i64),
}

/// The app's handle on the MPRIS thread.
pub struct Mpris {
    updates: mpsc::Sender<Update>,
    commands: mpsc::Receiver<Command>,
    shared: SharedRef,
    last: Option<State>,
    /// Last reported position and when, to notice seeks.
    last_position: Option<(i64, Instant)>,
}

impl Mpris {
    /// Starts the D-Bus thread. Never fails: without a session bus the
    /// thread just ends and the handle's sends go nowhere. With `on_bus`
    /// off it doesn't connect at all (screenshot mode).
    pub fn start(ctx: &egui::Context, on_bus: bool) -> Self {
        let (updates_tx, updates_rx) = mpsc::channel();
        let (commands_tx, commands_rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            state: State::default(),
            clock: Clock { base_us: 0, at: Instant::now(), running: false },
        }));
        let remote = Remote { tx: commands_tx, ctx: ctx.clone() };
        let thread_shared = Arc::clone(&shared);
        if on_bus {
            let _ = std::thread::Builder::new()
                .name("mpris".into())
                .spawn(move || run(remote, thread_shared, updates_rx));
        }
        Self {
            updates: updates_tx,
            commands: commands_rx,
            shared,
            last: None,
            last_position: None,
        }
    }

    /// Pending commands from D-Bus clients.
    pub fn commands(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }

    /// Reports the current state; only changes are sent on to D-Bus.
    pub fn update(&mut self, state: State, position_us: i64) {
        let playing = state.status == Status::Playing;
        {
            let mut shared = lock(&self.shared);
            shared.clock = Clock { base_us: position_us, at: Instant::now(), running: playing };
        }
        // A jump the clock didn't predict is a seek, whoever caused it.
        let same_track = self.last.as_ref().is_some_and(|l| l.track_id == state.track_id);
        if let Some((prev, at)) = self.last_position {
            let expected = if playing { prev + at.elapsed().as_micros() as i64 } else { prev };
            if same_track && (position_us - expected).abs() > 1_000_000 {
                let _ = self.updates.send(Update::Seeked(position_us));
            }
        }
        self.last_position = Some((position_us, Instant::now()));

        if self.last.as_ref() != Some(&state) {
            self.last = Some(state.clone());
            let _ = self.updates.send(Update::State(state));
        }
    }
}

fn connect(name: &str, remote: &Remote, shared: &SharedRef) -> zbus::Result<zbus::blocking::Connection> {
    zbus::blocking::connection::Builder::session()?
        .name(name.to_string())?
        .serve_at(OBJECT_PATH, Root { remote: remote.clone() })?
        .serve_at(OBJECT_PATH, Player { remote: remote.clone(), shared: Arc::clone(shared) })?
        .build()
}

fn run(remote: Remote, shared: SharedRef, updates: mpsc::Receiver<Update>) {
    // A second UnAmp registers a per-instance name, as the spec suggests.
    let connection = connect(BUS_NAME, &remote, &shared).or_else(|_| {
        let instance = format!("{BUS_NAME}.instance{}", std::process::id());
        connect(&instance, &remote, &shared)
    });
    let connection = match connection {
        Ok(c) => c,
        Err(e) => {
            eprintln!("unamp: media controls (MPRIS) unavailable: {e}");
            return;
        }
    };

    let mut previous = State::default();
    for update in updates {
        match update {
            Update::State(state) => {
                let changed = changed_properties(&previous, &state);
                lock(&shared).state = state.clone();
                previous = state;
                if !changed.is_empty() {
                    let body = (PLAYER_IFACE, changed, Vec::<String>::new());
                    let _ = connection.emit_signal(
                        None::<&str>,
                        OBJECT_PATH,
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &body,
                    );
                }
            }
            Update::Seeked(position) => {
                let _ = connection.emit_signal(None::<&str>, OBJECT_PATH, PLAYER_IFACE, "Seeked", &(position,));
            }
        }
    }
}

/// The Player properties that differ between two states, as D-Bus values.
fn changed_properties(old: &State, new: &State) -> HashMap<String, OwnedValue> {
    let mut out = HashMap::new();
    let mut put = |key: &str, value: Value<'_>| {
        if let Ok(owned) = OwnedValue::try_from(value) {
            out.insert(key.to_string(), owned);
        }
    };
    if old.status != new.status {
        put("PlaybackStatus", Value::from(new.status.as_str()));
    }
    if old.metadata() != new.metadata() {
        put("Metadata", Value::from(new.metadata()));
    }
    if old.volume != new.volume {
        put("Volume", Value::from(new.volume));
    }
    if old.shuffle != new.shuffle {
        put("Shuffle", Value::from(new.shuffle));
    }
    if old.repeat != new.repeat {
        put("LoopStatus", Value::from(new.loop_status()));
    }
    if old.can_go_next != new.can_go_next {
        put("CanGoNext", Value::from(new.can_go_next));
    }
    if old.can_go_previous != new.can_go_previous {
        put("CanGoPrevious", Value::from(new.can_go_previous));
    }
    if old.can_play != new.can_play {
        put("CanPlay", Value::from(new.can_play));
    }
    if old.track_id.is_some() != new.track_id.is_some() {
        put("CanPause", Value::from(new.track_id.is_some()));
    }
    if old.can_seek != new.can_seek {
        put("CanSeek", Value::from(new.can_seek));
    }
    out
}

/// What `to_player_command` needs to know about playback.
pub struct Playback<'a> {
    pub state: PlayState,
    pub position: std::time::Duration,
    pub duration: Option<std::time::Duration>,
    pub track_id: Option<&'a str>,
}

/// Translates an MPRIS request into the app's command, applying the spec's
/// state-dependent rules: Play does nothing while playing, Pause only
/// pauses, PlayPause starts from stopped, seeking past the end goes to the
/// next track, and SetPosition is ignored for a stale track id.
pub fn to_player_command(command: Command, now: &Playback<'_>) -> Option<PlayerCommand> {
    use PlayState as S;
    use PlayerCommand as P;
    Some(match command {
        Command::PlayPause => match now.state {
            S::Playing | S::Paused => P::TogglePause,
            S::Stopped => P::Play,
            S::Loading => return None,
        },
        Command::Play => match now.state {
            S::Paused => P::TogglePause,
            S::Stopped => P::Play,
            S::Playing | S::Loading => return None,
        },
        Command::Pause if now.state == S::Playing => P::TogglePause,
        Command::Pause => return None,
        Command::Stop => P::Stop,
        Command::Next => P::Next,
        Command::Previous => P::Previous,
        Command::Seek(offset_us) => {
            let target = now.position.as_micros() as i64 + offset_us;
            match now.duration {
                Some(d) if target > d.as_micros() as i64 => P::Next,
                _ => P::SeekTo(std::time::Duration::from_micros(target.max(0) as u64)),
            }
        }
        Command::SetPosition(track_id, position_us) => {
            let in_range = now.duration.is_some_and(|d| position_us <= d.as_micros() as i64);
            if now.track_id != Some(track_id.as_str()) || position_us < 0 || !in_range {
                return None;
            }
            P::SeekTo(std::time::Duration::from_micros(position_us as u64))
        }
        Command::Raise => P::Raise,
        Command::Quit => P::Quit,
        Command::SetVolume(v) => P::SetVolume(v as f32),
        Command::SetShuffle(on) => P::SetShuffle(on),
        Command::SetRepeat(r) => P::SetRepeat(r),
    })
}

/// A `file://` URL for a local path, percent-encoding anything that isn't
/// a plain path character.
pub fn file_url(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut url = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => url.push(b as char),
            _ => url.push_str(&format!("%{b:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> State {
        State {
            status: Status::Playing,
            track_id: Some("/org/unamp/track/1".into()),
            title: Some("Da Funk".into()),
            artist: Some("Daft Punk".into()),
            album: Some("Homework".into()),
            length_us: Some(328_000_000),
            art_url: Some("file:///tmp/a.jpg".into()),
            volume: 0.8,
            can_go_next: true,
            can_play: true,
            can_seek: true,
            ..Default::default()
        }
    }

    #[test]
    fn metadata_has_the_mpris_keys() {
        let md = playing().metadata();
        for key in ["mpris:trackid", "xesam:title", "xesam:artist", "xesam:album", "mpris:length", "mpris:artUrl"] {
            assert!(md.contains_key(key), "{key}");
        }
    }

    #[test]
    fn no_track_reports_only_the_no_track_id() {
        let md = State::default().metadata();
        assert_eq!(md.len(), 1);
        let id: ObjectPath<'_> = md["mpris:trackid"].downcast_ref().unwrap();
        assert_eq!(id.as_str(), NO_TRACK);
    }

    #[test]
    fn only_changed_properties_are_announced() {
        let a = playing();
        assert!(changed_properties(&a, &a).is_empty());
        let mut b = a.clone();
        b.status = Status::Paused;
        b.volume = 0.5;
        let changed = changed_properties(&a, &b);
        let mut keys: Vec<_> = changed.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["PlaybackStatus", "Volume"]);
    }

    #[test]
    fn repeat_maps_to_loop_status() {
        let mut s = State::default();
        for (repeat, status) in [(Repeat::Off, "None"), (Repeat::One, "Track"), (Repeat::All, "Playlist")] {
            s.repeat = repeat;
            assert_eq!(s.loop_status(), status);
        }
    }

    #[test]
    fn clock_advances_only_while_running() {
        let start = Instant::now() - std::time::Duration::from_secs(2);
        let paused = Clock { base_us: 5_000_000, at: start, running: false };
        assert_eq!(paused.now_us(), 5_000_000);
        let running = Clock { running: true, ..paused };
        assert!(running.now_us() >= 7_000_000);
    }

    fn at(state: PlayState, position_s: u64) -> Playback<'static> {
        Playback {
            state,
            position: std::time::Duration::from_secs(position_s),
            duration: Some(std::time::Duration::from_secs(200)),
            track_id: Some("/org/unamp/track/7"),
        }
    }

    #[test]
    fn play_and_pause_follow_the_spec() {
        use PlayerCommand as P;
        assert_eq!(to_player_command(Command::Play, &at(PlayState::Playing, 0)), None);
        assert_eq!(to_player_command(Command::Play, &at(PlayState::Paused, 0)), Some(P::TogglePause));
        assert_eq!(to_player_command(Command::Play, &at(PlayState::Stopped, 0)), Some(P::Play));
        assert_eq!(to_player_command(Command::Pause, &at(PlayState::Paused, 0)), None);
        assert_eq!(to_player_command(Command::Pause, &at(PlayState::Playing, 0)), Some(P::TogglePause));
        assert_eq!(to_player_command(Command::PlayPause, &at(PlayState::Stopped, 0)), Some(P::Play));
        assert_eq!(to_player_command(Command::PlayPause, &at(PlayState::Loading, 0)), None);
    }

    #[test]
    fn seeking_is_relative_and_past_the_end_skips() {
        use std::time::Duration;
        let now = at(PlayState::Playing, 100);
        assert_eq!(to_player_command(Command::Seek(5_000_000), &now), Some(PlayerCommand::SeekTo(Duration::from_secs(105))));
        assert_eq!(to_player_command(Command::Seek(-500_000_000), &now), Some(PlayerCommand::SeekTo(Duration::ZERO)));
        assert_eq!(to_player_command(Command::Seek(150_000_000), &now), Some(PlayerCommand::Next));
    }

    #[test]
    fn set_position_ignores_stale_tracks_and_out_of_range() {
        let now = at(PlayState::Playing, 10);
        let set = |id: &str, us| Command::SetPosition(id.into(), us);
        assert_eq!(
            to_player_command(set("/org/unamp/track/7", 60_000_000), &now),
            Some(PlayerCommand::SeekTo(std::time::Duration::from_secs(60)))
        );
        assert_eq!(to_player_command(set("/org/unamp/track/6", 60_000_000), &now), None);
        assert_eq!(to_player_command(set("/org/unamp/track/7", 300_000_000), &now), None);
        assert_eq!(to_player_command(set("/org/unamp/track/7", -1), &now), None);
    }

    #[test]
    fn file_url_percent_encodes() {
        assert_eq!(
            file_url(std::path::Path::new("/home/u/.cache/unamp/art/Sigur Rós.jpg")),
            "file:///home/u/.cache/unamp/art/Sigur%20R%C3%B3s.jpg"
        );
    }
}
