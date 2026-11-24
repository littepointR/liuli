use core::fmt;
use liuli::prelude::*;

const TITLES: [&str; 12] = [
    "Harbour at dawn",
    "Rain on glass",
    "Night crossing",
    "Under the bridge",
    "Morning market",
    "Signal box",
    "Pines in wind",
    "Footsteps on tile",
    "Tide pools",
    "Last tram",
    "Roof garden",
    "Station hall",
];

#[derive(Clone, Copy)]
pub(super) struct Take {
    pub key: Key,
    pub number: u32,
    pub seconds: u32,
}
impl Take {
    fn archive(index: u32) -> Self {
        Self {
            key: Key(1000 + index),
            number: index + 1,
            seconds: 148 + (index * 47 % 600),
        }
    }
}
impl fmt::Display for Take {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.key.0 >= 2000 {
            write!(f, "Field take {:02}", self.number)
        } else {
            f.write_str(TITLES[(self.number as usize - 1) % TITLES.len()])
        }
    }
}

pub(super) struct Deck {
    pub loaded: Take,
    position_ms: u64,
    pub envelope: i16,
    fade: Tween,
    pub playing: Option<InstantMs>,
    pub recording: Option<InstantMs>,
    pub recording_seconds: u32,
    pub logged: u32,
    saved: [Option<Take>; 8],
}
impl Deck {
    pub fn new() -> Self {
        Self {
            loaded: Take::archive(0),
            position_ms: 0,
            envelope: 1000,
            fade: Tween::at(1000),
            playing: None,
            recording: None,
            recording_seconds: 0,
            logged: 0,
            saved: [None; 8],
        }
    }
    pub fn count(&self) -> u32 {
        1000 + self.logged.min(8)
    }
    pub fn row(&self, index: u32) -> Take {
        let n = self.logged.min(8);
        if index < n {
            self.saved[((self.logged - 1 - index) % 8) as usize].unwrap()
        } else {
            Take::archive(index - n)
        }
    }
    pub fn load(&mut self, key: Key) {
        let take = match key.0 {
            1000..=1999 => Take::archive(key.0 - 1000),
            2000..=2007 => match self.saved[(key.0 - 2000) as usize] {
                Some(take) => take,
                None => return,
            },
            _ => return,
        };
        self.loaded = take;
        self.position_ms = 0;
        self.envelope = 1000;
        self.playing = None;
    }
    pub fn elapsed(&self) -> u32 {
        (self.position_ms / 1000) as u32
    }
    pub fn play_pause(&mut self, now: InstantMs, fade_ms: u32) {
        let was_playing = self.playing.is_some();
        self.advance(now);
        if was_playing {
            self.playing = None;
        } else {
            if self.elapsed() >= self.loaded.seconds {
                self.position_ms = 0;
            }
            self.fade = Tween::at(0);
            self.envelope = 0;
            self.fade
                .animate_to(now, 1000, fade_ms, Easing::EaseOutCubic);
            self.playing = Some(now);
        }
    }
    pub fn record(&mut self, now: InstantMs) {
        self.playing = None;
        self.recording = Some(now);
        self.recording_seconds = 0;
    }
    pub fn save(&mut self, now: InstantMs) {
        if self.recording.is_none() {
            return;
        }
        self.advance(now);
        self.recording = None;
        let slot = (self.logged % 8) as usize;
        self.logged = self.logged.saturating_add(1);
        let take = Take {
            key: Key(2000 + slot as u32),
            number: self.logged,
            seconds: self.recording_seconds.max(1),
        };
        self.saved[slot] = Some(take);
        self.load(take.key);
    }
    pub fn advance(&mut self, now: InstantMs) -> bool {
        if let Some(start) = self.recording {
            let sec = seconds_since(start, now);
            return core::mem::replace(&mut self.recording_seconds, sec) != sec;
        }
        if let Some(start) = self.playing {
            let end = u64::from(self.loaded.seconds) * 1000;
            let before = self.elapsed();
            self.position_ms = self
                .position_ms
                .saturating_add(now.saturating_sub(start))
                .min(end);
            self.playing = (self.position_ms < end).then_some(now);
            let level = self.fade.value(now) as i16;
            let previous = core::mem::replace(&mut self.envelope, level);
            return self.elapsed() != before || level != previous;
        }
        false
    }
    pub fn deadline(&self, now: InstantMs) -> Option<InstantMs> {
        self.recording
            .map(|s| s.saturating_add((u64::from(self.recording_seconds) + 1) * 1000))
            .or_else(|| {
                self.playing.map(|s| {
                    let second = s.saturating_add(1000 - self.position_ms % 1000);
                    self.fade
                        .next_deadline(now, core::num::NonZeroU32::new(40).unwrap())
                        .unwrap_or(second)
                        .min(second)
                })
            })
    }
}
fn seconds_since(start: InstantMs, now: InstantMs) -> u32 {
    (now.0.saturating_sub(start.0) / 1000).min(u64::from(u32::MAX)) as u32
}
