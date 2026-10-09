//! What plays next: the playlist (an ordered list moved through with
//! shuffle/repeat) and the "up next" queue, which plays before the
//! playlist continues (see ADR-0012).

use std::collections::VecDeque;
use std::path::Path;

use crate::config::Repeat;
use crate::library::Track;

/// Something that was playing, for Previous to return to.
#[derive(Debug, Clone)]
enum Played {
    Index(usize),
    Queued(Track),
}

/// Everything needed to restore the playlist and queue after a restart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Session {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
    pub queue: Vec<Track>,
    /// A queued track that was playing (it's no longer in `queue`).
    pub playing_queued: Option<Track>,
}

/// Fields are private so every change goes through a method that bumps
/// `revision`, which is how the app knows to save.
#[derive(Default)]
pub struct Playlist {
    tracks: Vec<Track>,
    /// Position in `tracks`: the playlist track playing, or, while a queued
    /// track plays, the one the playlist resumes after.
    current: Option<usize>,
    /// Up next. Plays front to back before the playlist continues.
    queue: VecDeque<Track>,
    /// The queued track that's playing, if the queue is what's playing.
    playing_queued: Option<Track>,
    /// Everything played so far, so Previous retraces shuffle and the queue.
    history: Vec<Played>,
    rng: Rng,
    revision: u64,
}

impl Playlist {
    /// Rebuilds a playlist from a saved session. History starts empty.
    pub fn restore(session: Session) -> Self {
        let current = session.current.filter(|&c| c < session.tracks.len());
        Self {
            tracks: session.tracks,
            current,
            queue: session.queue.into(),
            playing_queued: session.playing_queued,
            ..Default::default()
        }
    }

    pub fn session(&self) -> Session {
        Session {
            tracks: self.tracks.clone(),
            current: self.current,
            queue: self.queue.iter().cloned().collect(),
            playing_queued: self.playing_queued.clone(),
        }
    }

    /// Changes whenever the playlist, queue or position may have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn queue(&self) -> &VecDeque<Track> {
        &self.queue
    }

    /// Starts a new playlist at `start`. The queue is kept: it's the
    /// listener's "up next", independent of what they're playing through.
    pub fn replace(&mut self, tracks: Vec<Track>, start: usize) {
        self.touch();
        self.tracks = tracks;
        self.history.clear();
        self.playing_queued = None;
        self.current = (start < self.tracks.len()).then_some(start);
    }

    /// Appends to the end of the playlist.
    pub fn append(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.touch();
        self.tracks.extend(tracks);
    }

    /// Empties the playlist. The queue, and a queued track that's playing, stay.
    pub fn clear(&mut self) {
        self.touch();
        self.tracks.clear();
        self.history.retain(|p| matches!(p, Played::Queued(_)));
        self.current = None;
    }

    pub fn remove(&mut self, index: usize) {
        self.touch();
        if index >= self.tracks.len() {
            return;
        }
        self.tracks.remove(index);
        self.history.retain(|p| !matches!(p, Played::Index(i) if *i == index));
        for p in &mut self.history {
            if let Played::Index(i) = p {
                if *i > index {
                    *i -= 1;
                }
            }
        }
        self.current = match self.current {
            Some(c) if c == index => None,
            Some(c) if c > index => Some(c - 1),
            other => other,
        };
    }

    /// Moves playlist entry `from` so it ends up at index `to`. The current
    /// position and Previous history keep pointing at the same tracks.
    pub fn move_track(&mut self, from: usize, to: usize) {
        self.touch();
        let len = self.tracks.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        let remap = |i: usize| moved_index(i, from, to);
        self.current = self.current.map(remap);
        for p in &mut self.history {
            if let Played::Index(i) = p {
                *i = remap(*i);
            }
        }
    }

    /// Moves queue entry `from` so it ends up at index `to`.
    pub fn move_in_queue(&mut self, from: usize, to: usize) {
        self.touch();
        if from < self.queue.len() && to < self.queue.len() && from != to {
            if let Some(track) = self.queue.remove(from) {
                self.queue.insert(to, track);
            }
        }
    }

    /// The playlist track at `current`, whether or not it's what's playing.
    pub fn current_track(&self) -> Option<&Track> {
        self.tracks.get(self.current?)
    }

    /// What's playing (or would resume): a queued track, else the playlist's.
    pub fn now_playing(&self) -> Option<&Track> {
        self.playing_queued.as_ref().or_else(|| self.current_track())
    }

    /// The playlist position that's playing, or `None` while the queue plays.
    pub fn playing_index(&self) -> Option<usize> {
        if self.playing_queued.is_some() { None } else { self.current }
    }

    /// Puts `track` at the front of the queue.
    pub fn play_next(&mut self, track: Track) {
        self.touch();
        self.queue.push_front(track);
    }

    /// Puts tracks at the back of the queue.
    pub fn add_to_queue(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.touch();
        self.queue.extend(tracks);
    }

    pub fn remove_from_queue(&mut self, index: usize) {
        self.touch();
        self.queue.remove(index);
    }

    /// Moves queue entry `index` to the front of the queue.
    pub fn move_to_front(&mut self, index: usize) {
        self.touch();
        if let Some(track) = self.queue.remove(index) {
            self.queue.push_front(track);
        }
    }

    pub fn clear_queue(&mut self) {
        self.touch();
        self.queue.clear();
    }

    /// 1-based queue position of the first queued copy of `path`, for the
    /// Winamp-style `[n]` markers next to playlist entries.
    pub fn queue_position(&self, path: &Path) -> Option<usize> {
        self.queue.iter().position(|t| t.path == path).map(|i| i + 1)
    }

    fn remember_now_playing(&mut self, next: Option<usize>) {
        match (&self.playing_queued, self.current) {
            (Some(track), _) => self.history.push(Played::Queued(track.clone())),
            (None, Some(c)) if Some(c) != next => self.history.push(Played::Index(c)),
            _ => {}
        }
    }

    /// Plays playlist entry `index` now.
    pub fn jump(&mut self, index: usize) -> Option<&Track> {
        self.touch();
        if index >= self.tracks.len() {
            return None;
        }
        self.remember_now_playing(Some(index));
        self.playing_queued = None;
        self.current = Some(index);
        self.tracks.get(index)
    }

    /// Takes queue entry `index` out of the queue and plays it now.
    pub fn play_from_queue(&mut self, index: usize) -> Option<&Track> {
        self.touch();
        let track = self.queue.remove(index)?;
        self.remember_now_playing(None);
        self.playing_queued = Some(track);
        self.playing_queued.as_ref()
    }

    /// What plays after the current track: the queue first, then the
    /// playlist per shuffle/repeat. `auto` is true when the current track
    /// ended by itself (Repeat One only applies then, to whatever played).
    pub fn advance(&mut self, shuffle: bool, repeat: Repeat, auto: bool) -> Option<&Track> {
        self.touch();
        if auto && repeat == Repeat::One && self.now_playing().is_some() {
            return self.now_playing();
        }
        if let Some(track) = self.queue.pop_front() {
            self.remember_now_playing(None);
            self.playing_queued = Some(track);
            return self.playing_queued.as_ref();
        }

        let len = self.tracks.len();
        if len == 0 {
            if self.playing_queued.is_some() {
                self.remember_now_playing(None);
                self.playing_queued = None;
            }
            return None;
        }
        // While the queue played, `current` stayed on the playlist track
        // before it, so this resumes right after that one.
        let next = match self.current {
            Some(c) if shuffle && len > 1 => Some(self.rng.pick_other(len, c)),
            Some(_) if shuffle => (repeat != Repeat::Off).then_some(0),
            Some(c) if c + 1 < len => Some(c + 1),
            Some(_) => (repeat != Repeat::Off).then_some(0),
            None if shuffle => Some(self.rng.below(len)),
            None => Some(0),
        };
        let next = next?;
        self.remember_now_playing(Some(next));
        self.playing_queued = None;
        self.current = Some(next);
        self.tracks.get(next)
    }

    /// The previously played track, queued or not (retracing shuffle),
    /// else the playlist entry above. Going back to a queued track doesn't
    /// put it back in the queue.
    pub fn back(&mut self) -> Option<&Track> {
        self.touch();
        loop {
            match self.history.pop() {
                Some(Played::Index(p)) if p < self.tracks.len() => {
                    self.playing_queued = None;
                    self.current = Some(p);
                    return self.tracks.get(p);
                }
                Some(Played::Index(_)) => continue,
                Some(Played::Queued(track)) => {
                    self.playing_queued = Some(track);
                    return self.playing_queued.as_ref();
                }
                None => break,
            }
        }
        let prev = if self.playing_queued.is_some() { self.current? } else { self.current?.checked_sub(1)? };
        self.playing_queued = None;
        self.current = Some(prev);
        self.tracks.get(prev)
    }
}

/// Where index `i` ends up after the item at `from` moves to `to`.
fn moved_index(i: usize, from: usize, to: usize) -> usize {
    if i == from {
        to
    } else if from < to && i > from && i <= to {
        i - 1
    } else if from > to && i >= to && i < from {
        i + 1
    } else {
        i
    }
}

/// Converts a drop position — the gap before row `gap`, from 0 to len — into
/// the final index of the row being dragged from `from`.
pub fn drop_index(from: usize, gap: usize) -> usize {
    if gap > from { gap - 1 } else { gap }
}

/// Tiny xorshift generator — shuffle doesn't need a crypto-grade dependency.
struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        Rng(seed | 1)
    }
}

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// A random index in `0..n` other than `avoid` (requires `n > 1`).
    fn pick_other(&mut self, n: usize, avoid: usize) -> usize {
        let i = self.below(n - 1);
        if i >= avoid { i + 1 } else { i }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn playlist(n: usize) -> Playlist {
        let mut p = Playlist::default();
        p.replace(
            (0..n).map(|i| Track::new(PathBuf::from(format!("{i}.mp3")))).collect(),
            0,
        );
        p
    }

    #[test]
    fn advance_stops_at_end_without_repeat() {
        let mut p = playlist(2);
        assert!(p.advance(false, Repeat::Off, true).is_some());
        assert_eq!(p.current, Some(1));
        assert!(p.advance(false, Repeat::Off, true).is_none());
    }

    #[test]
    fn advance_wraps_with_repeat_all() {
        let mut p = playlist(2);
        p.advance(false, Repeat::All, true);
        p.advance(false, Repeat::All, true);
        assert_eq!(p.current, Some(0));
    }

    #[test]
    fn repeat_one_repeats_only_on_auto_advance() {
        let mut p = playlist(3);
        p.advance(false, Repeat::One, true);
        assert_eq!(p.current, Some(0));
        p.advance(false, Repeat::One, false);
        assert_eq!(p.current, Some(1));
    }

    #[test]
    fn shuffle_never_repeats_current_and_back_retraces() {
        let mut p = playlist(5);
        let mut order = vec![0];
        for _ in 0..20 {
            let before = p.current.unwrap();
            p.advance(true, Repeat::All, true);
            assert_ne!(p.current.unwrap(), before);
            order.push(p.current.unwrap());
        }
        for expected in order.iter().rev().skip(1).take(5) {
            p.back();
            assert_eq!(p.current, Some(*expected));
        }
    }

    fn track(name: &str) -> Track {
        Track::new(PathBuf::from(format!("{name}.mp3")))
    }

    fn playing(p: &Playlist) -> String {
        p.now_playing().map(|t| t.name.clone()).unwrap_or_default()
    }

    #[test]
    fn queue_plays_before_playlist_then_resumes_after_current() {
        let mut p = playlist(4);
        p.add_to_queue([track("q1"), track("q2")]);
        assert_eq!(p.advance(false, Repeat::Off, true).unwrap().name, "q1");
        assert_eq!(p.playing_index(), None);
        assert_eq!(p.playing_index(), None);
        assert_eq!(p.advance(false, Repeat::Off, true).unwrap().name, "q2");
        // Back to the playlist, right after the track that was playing.
        assert_eq!(p.advance(false, Repeat::Off, true).unwrap().name, "1");
        assert_eq!(p.playing_index(), Some(1));
    }

    #[test]
    fn play_next_jumps_the_queue() {
        let mut p = playlist(3);
        p.add_to_queue([track("later")]);
        p.play_next(track("soon"));
        assert_eq!(p.advance(false, Repeat::Off, false).unwrap().name, "soon");
        assert_eq!(p.advance(false, Repeat::Off, false).unwrap().name, "later");
    }

    #[test]
    fn previous_retraces_through_queued_tracks() {
        let mut p = playlist(3);
        p.play_next(track("q"));
        p.advance(false, Repeat::Off, true); // q
        p.advance(false, Repeat::Off, true); // 1
        assert_eq!(p.back().unwrap().name, "q");
        assert_eq!(p.playing_index(), None);
        assert_eq!(p.back().unwrap().name, "0");
        assert!(p.playing_index().is_some());
        // Going back didn't put q back in the queue.
        assert!(p.queue.is_empty());
    }

    #[test]
    fn repeat_one_repeats_a_queued_track_but_skip_moves_on() {
        let mut p = playlist(2);
        p.add_to_queue([track("q")]);
        p.advance(false, Repeat::One, false);
        assert_eq!(playing(&p), "q");
        p.advance(false, Repeat::One, true);
        assert_eq!(playing(&p), "q");
        p.advance(false, Repeat::One, false);
        assert_eq!(playing(&p), "1");
    }

    #[test]
    fn queue_survives_a_new_playlist_and_clearing() {
        let mut p = playlist(2);
        p.add_to_queue([track("q")]);
        p.replace(vec![track("a"), track("b")], 0);
        p.clear();
        assert_eq!(p.queue.len(), 1);
        assert_eq!(p.advance(false, Repeat::Off, true).unwrap().name, "q");
        // Nothing left after the queue: playback stops.
        assert!(p.advance(false, Repeat::Off, true).is_none());
    }

    #[test]
    fn play_from_queue_takes_it_out_and_plays_it() {
        let mut p = playlist(2);
        p.add_to_queue([track("a"), track("b"), track("c")]);
        assert_eq!(p.play_from_queue(1).unwrap().name, "b");
        let left: Vec<_> = p.queue.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(left, ["a", "c"]);
        assert_eq!(p.back().unwrap().name, "0");
    }

    #[test]
    fn queue_positions_mark_playlist_entries() {
        let mut p = playlist(3);
        p.add_to_queue([p.tracks[2].clone(), p.tracks[0].clone()]);
        assert_eq!(p.queue_position(&p.tracks[2].path), Some(1));
        assert_eq!(p.queue_position(&p.tracks[0].path), Some(2));
        assert_eq!(p.queue_position(&p.tracks[1].path), None);
    }

    #[test]
    fn session_round_trips_and_every_change_bumps_revision() {
        let mut p = playlist(3);
        let r0 = p.revision();
        p.add_to_queue([track("q1"), track("q2")]);
        assert_ne!(p.revision(), r0);
        p.advance(false, Repeat::Off, true); // q1 playing
        let r1 = p.revision();
        p.move_to_front(1);
        assert_ne!(p.revision(), r1);

        let restored = Playlist::restore(p.session());
        assert_eq!(restored.session(), p.session());
        assert_eq!(playing(&restored), "q1");
        assert_eq!(restored.queue().len(), 1);
    }

    #[test]
    fn restore_drops_an_out_of_range_current() {
        let p = Playlist::restore(Session {
            tracks: vec![track("a")],
            current: Some(5),
            ..Default::default()
        });
        assert_eq!(p.current, None);
    }

    fn names(p: &Playlist) -> Vec<String> {
        p.tracks().iter().map(|t| t.name.clone()).collect()
    }

    #[test]
    fn moving_a_track_keeps_current_on_the_same_track() {
        let mut p = playlist(5);
        p.jump(3);
        p.move_track(0, 4); // 1 2 3 4 0
        assert_eq!(names(&p), ["1", "2", "3", "4", "0"]);
        assert_eq!(p.current_track().unwrap().name, "3");
        p.move_track(4, 0); // 0 1 2 3 4
        assert_eq!(p.current_track().unwrap().name, "3");
        p.move_track(3, 1); // 0 3 1 2 4: the current track itself moves
        assert_eq!(names(&p), ["0", "3", "1", "2", "4"]);
        assert_eq!(p.current, Some(1));
        assert_eq!(p.current_track().unwrap().name, "3");
    }

    #[test]
    fn moving_keeps_previous_history_on_the_same_tracks() {
        let mut p = playlist(4);
        p.jump(2);
        p.jump(3); // history: 0, 2
        p.move_track(0, 3); // 1 2 3 0
        assert_eq!(p.back().unwrap().name, "2");
        assert_eq!(p.back().unwrap().name, "0");
    }

    #[test]
    fn queue_entries_can_be_reordered() {
        let mut p = playlist(1);
        p.add_to_queue([track("a"), track("b"), track("c")]);
        p.move_in_queue(2, 0);
        let order: Vec<_> = p.queue().iter().map(|t| t.name.as_str()).collect();
        assert_eq!(order, ["c", "a", "b"]);
    }

    #[test]
    fn drop_gaps_map_to_final_indices() {
        // Dragging row 1 of 4: dropping in gap 0 puts it first, gaps 1 and 2
        // leave it where it is, gap 4 (after the last row) puts it last.
        assert_eq!(drop_index(1, 0), 0);
        assert_eq!(drop_index(1, 1), 1);
        assert_eq!(drop_index(1, 2), 1);
        assert_eq!(drop_index(1, 4), 3);
    }

    #[test]
    fn remove_keeps_current_pointing_at_same_track() {
        let mut p = playlist(4);
        p.jump(2);
        p.remove(0);
        assert_eq!(p.current, Some(1));
        assert_eq!(p.current_track().unwrap().name, "2");
        p.remove(1);
        assert_eq!(p.current, None);
    }
}
