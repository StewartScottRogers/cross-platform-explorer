//! Media playlist / queue model (CPE-943, epic CPE-720): the pure navigation core behind the audio/video
//! player pane — an ordered list of tracks with a current position, and next/previous that honour a
//! **repeat** mode and an optional **shuffle** order. No decoding or playback here; the player pane drives
//! this and renders whatever `current()` points at. Shuffle is seeded so it's deterministic + testable.

/// How playback loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    /// Stop at the ends (`next()` past the last track returns `None`).
    Off,
    /// Repeat the current track (`next()`/`prev()` stay put).
    One,
    /// Wrap around at the ends.
    All,
}

/// An ordered playlist with a cursor. `order` is the play sequence (indices into `items`); it's the
/// identity until shuffled.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Playlist {
    items: Vec<String>,
    order: Vec<usize>,
    pos: usize,
    repeat: RepeatMode,
    shuffled: bool,
}

impl Playlist {
    pub fn new(items: Vec<String>) -> Self {
        let order = (0..items.len()).collect();
        Self { items, order, pos: 0, repeat: RepeatMode::Off, shuffled: false }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }
    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    /// The track at the cursor, or `None` for an empty playlist.
    pub fn current(&self) -> Option<&str> {
        self.order.get(self.pos).and_then(|&i| self.items.get(i)).map(|s| s.as_str())
    }

    /// Advance per the repeat mode; returns the new current track (or `None` at a hard end with `Off`).
    /// (Not an `Iterator`: it returns the *current* track and honours repeat/shuffle, never consuming.)
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&str> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => {}
            RepeatMode::Off => {
                if self.pos + 1 >= self.order.len() {
                    return None;
                }
                self.pos += 1;
            }
            RepeatMode::All => self.pos = (self.pos + 1) % self.order.len(),
        }
        self.current()
    }

    /// Step back per the repeat mode; returns the new current track (or `None` before the start with `Off`).
    pub fn prev(&mut self) -> Option<&str> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => {}
            RepeatMode::Off => {
                if self.pos == 0 {
                    return None;
                }
                self.pos -= 1;
            }
            RepeatMode::All => self.pos = (self.pos + self.order.len() - 1) % self.order.len(),
        }
        self.current()
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    /// Jump the cursor to the track at `item_index` (an index into the original items). No-op if out of
    /// range.
    pub fn select(&mut self, item_index: usize) {
        if let Some(p) = self.order.iter().position(|&i| i == item_index) {
            self.pos = p;
        }
    }

    /// Toggle shuffle. Rebuilds the play order (deterministically from `seed` when turning it on, identity
    /// when off) and keeps the **current track** under the cursor across the change.
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        let current_item = self.order.get(self.pos).copied();
        self.order = if on { shuffled_order(self.items.len(), seed) } else { (0..self.items.len()).collect() };
        self.shuffled = on;
        self.pos = current_item
            .and_then(|it| self.order.iter().position(|&i| i == it))
            .unwrap_or(0);
    }
}

/// A deterministic Fisher-Yates shuffle of `0..n` driven by a small LCG — a permutation, reproducible for
/// a given seed so playback order is testable (the real UI can seed from the clock).
fn shuffled_order(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    for i in (1..n).rev() {
        // xorshift-ish step for a spread of bits.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(n: usize) -> Playlist {
        Playlist::new((0..n).map(|i| format!("t{i}")).collect())
    }

    #[test]
    fn off_stops_at_the_ends() {
        let mut p = pl(3);
        assert_eq!(p.current(), Some("t0"));
        assert_eq!(p.next(), Some("t1"));
        assert_eq!(p.next(), Some("t2"));
        assert_eq!(p.next(), None); // hard stop
        assert_eq!(p.current(), Some("t2")); // cursor unmoved past the end
        assert_eq!(p.prev(), Some("t1"));
        p.select(0);
        assert_eq!(p.prev(), None);
    }

    #[test]
    fn all_wraps_both_ways() {
        let mut p = pl(3);
        p.set_repeat(RepeatMode::All);
        assert_eq!(p.next(), Some("t1"));
        assert_eq!(p.next(), Some("t2"));
        assert_eq!(p.next(), Some("t0")); // wrap
        assert_eq!(p.prev(), Some("t2")); // wrap back
    }

    #[test]
    fn one_repeats_the_current_track() {
        let mut p = pl(3);
        p.select(1);
        p.set_repeat(RepeatMode::One);
        assert_eq!(p.next(), Some("t1"));
        assert_eq!(p.next(), Some("t1"));
        assert_eq!(p.prev(), Some("t1"));
    }

    #[test]
    fn shuffle_is_a_permutation_and_keeps_the_current_track() {
        let mut p = pl(6);
        p.select(3);
        assert_eq!(p.current(), Some("t3"));
        p.set_shuffle(true, 42);
        assert!(p.is_shuffled());
        assert_eq!(p.current(), Some("t3")); // cursor followed the track
        // Every track is still reachable exactly once.
        p.set_repeat(RepeatMode::All);
        let mut seen: Vec<String> = vec![p.current().unwrap().to_string()];
        for _ in 0..5 {
            seen.push(p.next().unwrap().to_string());
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 6);
        // Turning shuffle off restores the original order but keeps whatever track is current now.
        p.select(2);
        let cur = p.current().unwrap().to_string();
        p.set_shuffle(false, 0);
        assert_eq!(p.current().map(str::to_string), Some(cur));
        assert!(!p.is_shuffled());
    }

    #[test]
    fn empty_playlist_is_safe() {
        let mut p = Playlist::new(vec![]);
        assert!(p.is_empty());
        assert_eq!(p.current(), None);
        assert_eq!(p.next(), None);
        assert_eq!(p.prev(), None);
    }
}
