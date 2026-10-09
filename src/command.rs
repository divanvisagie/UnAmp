//! What the user (or a remote control) can ask the player to do. Every
//! front end — the regular egui windows, classic skin windows, keyboard
//! shortcuts and MPRIS — expresses its input as these, and the app applies
//! them in one place, so behaviour can't drift between them (see ADR-0018).

use std::time::Duration;

use crate::config::Repeat;
use crate::eq::EqSnapshot;

/// The app's windows that commands can open, close or toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Player,
    Equalizer,
    Playlist,
    Library,
    Waveform,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Previous track, or restart the current one if it's past 3 seconds.
    Previous,
    /// Winamp's Play: resume if paused, restart if playing, else start.
    Play,
    TogglePause,
    Stop,
    Next,
    SeekTo(Duration),
    /// Relative seek in seconds; clamped to the track.
    SeekBy(f64),
    /// Sets the level and unmutes: changing the volume means you want sound.
    SetVolume(f32),
    ToggleMute,
    ToggleShuffle,
    SetShuffle(bool),
    CycleRepeat,
    SetRepeat(Repeat),
    SetEq(EqSnapshot),
    /// Time display: elapsed ⇄ remaining.
    ToggleRemaining,
    OpenWindow(Window),
    CloseWindow(Window),
    ToggleWindow(Window),
    Playlist(PlaylistAction),
    Minimize,
    Raise,
    Quit,
}

/// What can be done to a playlist entry, from any playlist view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaylistAction {
    Play(usize),
    PlayNext(usize),
    AddToQueue(usize),
    Remove(usize),
    /// Move entry `from` so it ends up at index `to`.
    Move { from: usize, to: usize },
}

/// A playlist entry's title with its Winamp-style up-next position, e.g.
/// `[2] Daft Punk - Da Funk`.
pub fn with_queue_marker(position: Option<usize>, title: String) -> String {
    match position {
        Some(n) => format!("[{n}] {title}"),
        None => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_marker_prefixes_only_queued_entries() {
        assert_eq!(with_queue_marker(Some(2), "Da Funk".into()), "[2] Da Funk");
        assert_eq!(with_queue_marker(None, "Da Funk".into()), "Da Funk");
    }
}
