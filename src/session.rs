//! Saves and restores the playlist and up-next queue as extended M3U
//! files in UnAmp's data directory, so they survive restarts and can be
//! opened by other players (see ADR-0013).
//!
//! UnAmp marks where you were with comment lines other players ignore:
//! `#UNAMP-CURRENT` before the current playlist entry, and
//! `#UNAMP-PLAYING` before a queued track that was playing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::library::Track;
use crate::metadata::TrackInfo;
use crate::playlist::Session;

pub const PLAYLIST_FILE: &str = "playlist.m3u8";
pub const QUEUE_FILE: &str = "queue.m3u8";
const CURRENT: &str = "#UNAMP-CURRENT";
const PLAYING: &str = "#UNAMP-PLAYING";

/// `~/.local/share/unamp` (or `$XDG_DATA_HOME/unamp`).
pub fn dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("unamp"))
}

/// Loads the saved session; missing or unreadable files give an empty one.
/// Paths aren't checked: a share that's offline now may be back by the
/// time a track comes up, and stat-ing it here could hang startup.
pub fn load(dir: &Path) -> Session {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    let mut session = Session::default();
    for (i, entry) in parse(&read(PLAYLIST_FILE), dir).into_iter().enumerate() {
        if entry.marker == Some(CURRENT) {
            session.current = Some(i);
        }
        session.tracks.push(entry.track);
    }
    for entry in parse(&read(QUEUE_FILE), dir) {
        if entry.marker == Some(PLAYING) && session.playing_queued.is_none() {
            session.playing_queued = Some(entry.track);
        } else {
            session.queue.push(entry.track);
        }
    }
    session
}

/// Writes both files, each via a temporary file and a rename so a crash
/// mid-write never leaves a truncated playlist.
pub fn save(dir: &Path, session: &Session) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let playlist = session
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t, (session.current == Some(i)).then_some(CURRENT)));
    let queue = session
        .playing_queued
        .iter()
        .map(|t| (t, Some(PLAYING)))
        .chain(session.queue.iter().map(|t| (t, None)));
    write_atomic(&dir.join(PLAYLIST_FILE), &render(playlist))?;
    write_atomic(&dir.join(QUEUE_FILE), &render(queue))
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("m3u8.tmp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn render<'a>(entries: impl Iterator<Item = (&'a Track, Option<&'static str>)>) -> String {
    let mut out = String::from("#EXTM3U\n");
    for (track, marker) in entries {
        let path = track.path.to_string_lossy();
        // A newline in a path would split the entry; such files can't be
        // represented in M3U, so they're left out.
        if path.contains(['\n', '\r']) {
            continue;
        }
        if let Some(marker) = marker {
            out.push_str(marker);
            out.push('\n');
        }
        let secs = track.duration().map(|d| d.as_secs() as i64).unwrap_or(-1);
        // Only real tag info is written, so it isn't mistaken for tags on reload.
        let title = track
            .info
            .as_ref()
            .map(|i| i.display_title(&track.path).replace(['\n', '\r'], " "))
            .unwrap_or_default();
        out.push_str(&format!("#EXTINF:{secs},{title}\n{path}\n"));
    }
    out
}

struct Entry {
    track: Track,
    marker: Option<&'static str>,
}

/// Parses extended or plain M3U. Relative paths resolve against `base`.
/// `#EXTINF` titles and durations become the track's provisional info, so
/// restored lists show names and lengths without re-reading tags.
fn parse(text: &str, base: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut extinf: Option<(Option<Duration>, Option<String>)> = None;
    let mut marker = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (secs, title) = rest.split_once(',').unwrap_or((rest, ""));
            let duration = secs
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|&s| s > 0)
                .map(|s| Duration::from_secs(s as u64));
            let title = Some(title.trim().to_string()).filter(|t| !t.is_empty());
            extinf = Some((duration, title));
            continue;
        }
        if line == CURRENT {
            marker = Some(CURRENT);
            continue;
        }
        if line == PLAYING {
            marker = Some(PLAYING);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let path = Path::new(line);
        let path = if path.is_absolute() { path.to_path_buf() } else { base.join(path) };
        let mut track = Track::new(path);
        if let Some((duration, title)) = extinf.take() {
            if duration.is_some() || title.is_some() {
                track.info = Some(TrackInfo {
                    title,
                    duration,
                    ..Default::default()
                });
            }
        }
        entries.push(Entry {
            track,
            marker: marker.take(),
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str, title: Option<&str>, secs: Option<u64>) -> Track {
        let mut t = Track::new(PathBuf::from(path));
        if title.is_some() || secs.is_some() {
            t.info = Some(TrackInfo {
                title: title.map(String::from),
                duration: secs.map(Duration::from_secs),
                ..Default::default()
            });
        }
        t
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("unamp-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn session_round_trips_through_m3u_files() {
        let dir = temp_dir("roundtrip");
        let session = Session {
            tracks: vec![
                track("/m/01 Intro.flac", Some("Band - Intro"), Some(65)),
                track("/m/02 Song.flac", None, None),
            ],
            current: Some(1),
            queue: vec![track("/nas/q.mp3", Some("Q"), Some(200))],
            playing_queued: Some(track("/nas/now.mp3", Some("Now"), Some(180))),
        };
        save(&dir, &session).unwrap();
        assert_eq!(load(&dir), session);
        // No temp files left behind.
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_load_as_empty() {
        assert_eq!(load(&temp_dir("missing")), Session::default());
    }

    #[test]
    fn written_playlist_is_plain_extended_m3u() {
        let text = render(
            [(&track("/m/a.mp3", Some("A - B"), Some(61)), Some(CURRENT))].into_iter(),
        );
        assert_eq!(text, "#EXTM3U\n#UNAMP-CURRENT\n#EXTINF:61,A - B\n/m/a.mp3\n");
    }

    #[test]
    fn parses_foreign_m3u_with_relative_paths_crlf_and_comments() {
        let text = "#EXTM3U\r\n# made by Winamp\r\n#EXTINF:-1,\r\nAlbum/01.mp3\r\n\r\n/abs/02.ogg\r\n";
        let entries = parse(text, Path::new("/home/u/Music"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].track.path, PathBuf::from("/home/u/Music/Album/01.mp3"));
        assert_eq!(entries[0].track.info, None);
        assert_eq!(entries[1].track.path, PathBuf::from("/abs/02.ogg"));
        assert!(entries.iter().all(|e| e.marker.is_none()));
    }

    #[test]
    fn paths_with_newlines_are_skipped() {
        let text = render([(&track("/m/bad\nname.mp3", None, None), None)].into_iter());
        assert_eq!(text, "#EXTM3U\n");
    }
}
