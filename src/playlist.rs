//! The play queue: an ordered list of tracks, the current position, and
//! the shuffle/repeat rules for moving through it.

use crate::config::Repeat;
use crate::library::Track;

#[derive(Default)]
pub struct Playlist {
    pub tracks: Vec<Track>,
    pub current: Option<usize>,
    /// Indices played so far, so Previous retraces a shuffled order.
    history: Vec<usize>,
    rng: Rng,
}

impl Playlist {
    pub fn replace(&mut self, tracks: Vec<Track>, start: usize) {
        self.tracks = tracks;
        self.history.clear();
        self.current = (start < self.tracks.len()).then_some(start);
    }

    pub fn enqueue(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.tracks.extend(tracks);
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.history.clear();
        self.current = None;
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.tracks.remove(index);
        self.history.retain(|&i| i != index);
        for i in &mut self.history {
            if *i > index {
                *i -= 1;
            }
        }
        self.current = match self.current {
            Some(c) if c == index => None,
            Some(c) if c > index => Some(c - 1),
            other => other,
        };
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.tracks.get(self.current?)
    }

    pub fn jump(&mut self, index: usize) -> Option<&Track> {
        if index >= self.tracks.len() {
            return None;
        }
        if let Some(c) = self.current {
            self.history.push(c);
        }
        self.current = Some(index);
        self.tracks.get(index)
    }

    /// The track after the current one, per shuffle/repeat. `auto` is true
    /// when the current track ended by itself (Repeat One only applies then).
    pub fn advance(&mut self, shuffle: bool, repeat: Repeat, auto: bool) -> Option<&Track> {
        let len = self.tracks.len();
        if len == 0 {
            return None;
        }
        let next = match self.current {
            Some(c) if auto && repeat == Repeat::One => Some(c),
            Some(c) if shuffle && len > 1 => Some(self.rng.pick_other(len, c)),
            Some(_) if shuffle => (repeat != Repeat::Off).then_some(0),
            Some(c) if c + 1 < len => Some(c + 1),
            Some(_) => (repeat != Repeat::Off).then_some(0),
            None if shuffle => Some(self.rng.below(len)),
            None => Some(0),
        };
        let next = next?;
        if let Some(c) = self.current {
            if c != next {
                self.history.push(c);
            }
        }
        self.current = Some(next);
        self.tracks.get(next)
    }

    /// The previously played track (retracing shuffle), else the one above.
    pub fn back(&mut self) -> Option<&Track> {
        let prev = match self.history.pop() {
            Some(p) if p < self.tracks.len() => p,
            _ => self.current?.checked_sub(1)?,
        };
        self.current = Some(prev);
        self.tracks.get(prev)
    }
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
