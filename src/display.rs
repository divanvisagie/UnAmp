//! Display rules shared by the regular and classic-skin player windows, so
//! the two can only differ in how they draw, not in what they show.

use std::time::Duration;

use crate::config::Repeat;
use crate::player::PlayState;

/// The player state every front end shows, gathered once per frame so the
/// regular windows, classic skins and MPRIS can't disagree about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerStatus {
    pub state: PlayState,
    pub position: Duration,
    pub duration: Option<Duration>,
    /// The volume as heard: 0 while muted.
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub show_remaining: bool,
}

/// What the big time display shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeDisplay {
    /// Nothing loaded (stopped, or between tracks).
    Blank,
    /// Paused, in the "off" half of the blink.
    BlinkOff,
    /// Elapsed time, or remaining time with a minus sign.
    Shown { negative: bool, time: Duration },
}

/// The time display at `now` (seconds, for the paused blink). Remaining
/// time needs a known length; without one it falls back to elapsed.
pub fn time_display(
    state: PlayState,
    position: Duration,
    duration: Option<Duration>,
    show_remaining: bool,
    now: f64,
) -> TimeDisplay {
    match state {
        PlayState::Stopped | PlayState::Loading => TimeDisplay::Blank,
        PlayState::Paused if (now * 2.0) as i64 % 2 == 1 => TimeDisplay::BlinkOff,
        _ => match (show_remaining, duration) {
            (true, Some(total)) => TimeDisplay::Shown { negative: true, time: total.saturating_sub(position) },
            _ => TimeDisplay::Shown { negative: false, time: position },
        },
    }
}

/// Seek-bar interaction: while the bar is held, `drag` previews the
/// fraction under the pointer; on release (or a click) the preview is
/// returned as the fraction to seek to.
pub fn seek_drag(resp: &egui::Response, pointer_fraction: Option<f32>, drag: &mut Option<f32>) -> Option<f32> {
    if resp.is_pointer_button_down_on() {
        if let Some(f) = pointer_fraction {
            *drag = Some(f.clamp(0.0, 1.0));
        }
    }
    if resp.drag_stopped() || resp.clicked() {
        return drag.take().or(pointer_fraction.map(|f| f.clamp(0.0, 1.0)));
    }
    None
}

/// How far through the track the seek bar's handle sits.
pub fn seek_fraction(drag: Option<f32>, position: Duration, duration: Option<Duration>) -> f32 {
    match (drag, duration) {
        (Some(f), _) => f,
        (None, Some(total)) if !total.is_zero() => (position.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    #[test]
    fn stopped_and_loading_are_blank() {
        for state in [PlayState::Stopped, PlayState::Loading] {
            assert_eq!(time_display(state, MIN, Some(MIN * 3), false, 0.0), TimeDisplay::Blank);
        }
    }

    #[test]
    fn remaining_needs_a_length() {
        assert_eq!(
            time_display(PlayState::Playing, MIN, Some(MIN * 3), true, 0.0),
            TimeDisplay::Shown { negative: true, time: MIN * 2 }
        );
        assert_eq!(
            time_display(PlayState::Playing, MIN, None, true, 0.0),
            TimeDisplay::Shown { negative: false, time: MIN }
        );
    }

    #[test]
    fn paused_blinks_twice_a_second() {
        let at = |now| time_display(PlayState::Paused, MIN, None, false, now);
        assert_eq!(at(0.2), TimeDisplay::Shown { negative: false, time: MIN });
        assert_eq!(at(0.7), TimeDisplay::BlinkOff);
        assert_eq!(at(1.2), TimeDisplay::Shown { negative: false, time: MIN });
    }

    #[test]
    fn seek_fraction_prefers_the_drag_preview() {
        assert_eq!(seek_fraction(Some(0.3), MIN, Some(MIN * 2)), 0.3);
        assert_eq!(seek_fraction(None, MIN, Some(MIN * 2)), 0.5);
        assert_eq!(seek_fraction(None, MIN, None), 0.0);
        assert_eq!(seek_fraction(None, MIN, Some(Duration::ZERO)), 0.0);
    }
}
