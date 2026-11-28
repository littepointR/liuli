use liuli::prelude::*;
use liuli::text::Lf4Font;

use crate::Theme;
use crate::pinyin_dict::{DEFAULT_PHRASES, DEFAULT_SYLLABLES};

const TEXT_BOX_PAD_X: i32 = 8;
use liuli::core::capacity::IME_DISPLAY_BYTES as MAX_DISPLAY_BYTES;
const MAX_PREEDIT_BYTES: usize = liuli::core::capacity::IME_PREEDIT_BYTES;
const MAX_CODE_BYTES: usize = 16;
const MAX_CANDS_SHOWN: usize = liuli::core::capacity::IME_CANDIDATES;

// ---------------------------------------------------------------------------
// 1. FixedTextEditor<CAP> — Zero-heap UTF-8 text + preedit + cursor state
// ---------------------------------------------------------------------------

/// Fixed-capacity UTF-8 text buffer with independent IME preedit composition and caret tracking.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct FixedTextEditor<const CAP: usize> {
    buf: [u8; CAP],
    len: usize,
    cursor: usize,
    preedit: [u8; MAX_PREEDIT_BYTES],
    preedit_len: usize,
    password: bool,
    rev: u32,
}

impl<const CAP: usize> FixedTextEditor<CAP> {
    pub const fn new() -> Self {
        Self {
            buf: [0; CAP],
            len: 0,
            cursor: 0,
            preedit: [0; MAX_PREEDIT_BYTES],
            preedit_len: 0,
            password: false,
            rev: 1,
        }
    }

    pub fn from_text(initial: &str) -> Self {
        let mut ed = Self::new();
        let _ = ed.apply(TextInput::Commit(initial));
        ed
    }

    #[inline]
    pub const fn with_password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    #[inline]
    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    #[inline]
    pub fn preedit(&self) -> &str {
        core::str::from_utf8(&self.preedit[..self.preedit_len]).unwrap_or("")
    }

    #[inline]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub const fn revision(&self) -> u32 {
        self.rev
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0 && self.preedit_len == 0
    }

    #[inline]
    pub const fn is_password(&self) -> bool {
        self.password
    }

    pub fn clear(&mut self) {
        if self.len > 0 || self.preedit_len > 0 {
            self.len = 0;
            self.cursor = 0;
            self.preedit_len = 0;
            self.rev = self.rev.wrapping_add(1).max(1);
        }
    }

    fn prev_char_boundary(s: &str, pos: usize) -> usize {
        let mut p = pos.min(s.len());
        if p == 0 {
            return 0;
        }
        p -= 1;
        while p > 0 && !s.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    fn next_char_boundary(s: &str, pos: usize) -> usize {
        let p = pos.min(s.len());
        if p >= s.len() {
            return s.len();
        }
        s[p..]
            .chars()
            .next()
            .map_or(s.len(), |ch| p + ch.len_utf8())
    }

    /// Apply a `TextInput` event. Returns `true` when editor contents, preedit, or cursor changed.
    pub fn apply(&mut self, input: TextInput<'_>) -> bool {
        match input {
            TextInput::Compose(s) => {
                let take = utf8_prefix_len(s, MAX_PREEDIT_BYTES);
                if self.preedit_len == take && self.preedit[..take] == s.as_bytes()[..take] {
                    return false;
                }
                self.preedit[..take].copy_from_slice(&s.as_bytes()[..take]);
                self.preedit_len = take;
                self.rev = self.rev.wrapping_add(1).max(1);
                true
            }
            TextInput::Commit(s) => {
                let had_preedit = self.preedit_len > 0;
                self.preedit_len = 0;
                let mut inserted = 0usize;
                for ch in s.chars() {
                    if matches!(ch, '\r' | '\n') {
                        continue;
                    }
                    let cw = ch.len_utf8();
                    if self.len + cw > CAP {
                        break;
                    }
                    self.buf
                        .copy_within(self.cursor..self.len, self.cursor + cw);
                    let mut tmp = [0u8; 4];
                    let encoded = ch.encode_utf8(&mut tmp);
                    self.buf[self.cursor..self.cursor + cw].copy_from_slice(encoded.as_bytes());
                    self.cursor += cw;
                    self.len += cw;
                    inserted += cw;
                }
                if had_preedit || inserted > 0 {
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else {
                    false
                }
            }
            TextInput::Cancel => {
                if self.preedit_len > 0 {
                    self.preedit_len = 0;
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else {
                    false
                }
            }
            TextInput::Backspace => {
                if self.preedit_len > 0 {
                    self.preedit_len = 0;
                    self.rev = self.rev.wrapping_add(1).max(1);
                    return true;
                }
                if self.cursor == 0 {
                    return false;
                }
                let prev = Self::prev_char_boundary(self.text(), self.cursor);
                let rem = self.cursor - prev;
                self.buf.copy_within(self.cursor..self.len, prev);
                self.cursor = prev;
                self.len -= rem;
                self.rev = self.rev.wrapping_add(1).max(1);
                true
            }
            TextInput::Left => {
                let cleared = self.preedit_len > 0;
                self.preedit_len = 0;
                if self.cursor > 0 {
                    self.cursor = Self::prev_char_boundary(self.text(), self.cursor);
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else if cleared {
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else {
                    false
                }
            }
            TextInput::Right => {
                let cleared = self.preedit_len > 0;
                self.preedit_len = 0;
                if self.cursor < self.len {
                    self.cursor = Self::next_char_boundary(self.text(), self.cursor);
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else if cleared {
                    self.rev = self.rev.wrapping_add(1).max(1);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Apply an `ImeEvent` produced by `SoftKeyboard`. Returns `true` if contents changed.
    pub fn apply_ime(&mut self, ev: ImeEvent<'_>) -> bool {
        match ev {
            ImeEvent::Text(ti) => self.apply(ti),
            ImeEvent::CommitCompose { commit, compose } => {
                self.apply(TextInput::Commit(commit)) | self.apply(TextInput::Compose(compose))
            }
            ImeEvent::None | ImeEvent::Done => false,
        }
    }

    /// Format display string (inserting active preedit at `cursor`) into `out`.
    /// Returns `(display_str, pre_start, pre_end, caret_byte)`.
    pub fn format_display<'b>(&self, out: &'b mut [u8]) -> (&'b str, usize, usize, usize) {
        let mut n = 0usize;
        let text = self.text();
        let cur = self.cursor.min(text.len());
        let pre = self.preedit();

        let push_masked_or_raw = |slice: &str, out: &mut [u8], n: &mut usize, password: bool| {
            if !password {
                let take = utf8_prefix_len(slice, out.len().saturating_sub(*n));
                out[*n..*n + take].copy_from_slice(&slice.as_bytes()[..take]);
                *n += take;
            } else {
                for _ in slice.chars() {
                    if *n >= out.len() {
                        break;
                    }
                    out[*n] = b'*';
                    *n += 1;
                }
            }
        };

        push_masked_or_raw(&text[..cur], out, &mut n, self.password);
        let pre_start = n;
        let pre_take = utf8_prefix_len(pre, out.len().saturating_sub(n));
        out[n..n + pre_take].copy_from_slice(&pre.as_bytes()[..pre_take]);
        n += pre_take;
        let pre_end = n;
        let caret_byte = pre_end;
        push_masked_or_raw(&text[cur..], out, &mut n, self.password);

        let s = core::str::from_utf8(&out[..n]).unwrap_or("");
        (s, pre_start, pre_end, caret_byte)
    }

    /// Move committed cursor to the boundary closest to widget-local `local_x`.
    pub fn set_cursor_from_x(
        &mut self,
        local_x: i32,
        scroll_x: i32,
        font: Option<&Lf4Font<'_>>,
    ) -> bool {
        let mut tmp = [0u8; MAX_DISPLAY_BYTES];
        let (display, start, end, _) = self.format_display(&mut tmp);
        self.place_display_cursor(local_x, scroll_x, font, display, start..end)
    }

    fn place_display_cursor(
        &mut self,
        local_x: i32,
        scroll_x: i32,
        font: Option<&Lf4Font<'_>>,
        display: &str,
        preedit: core::ops::Range<usize>,
    ) -> bool {
        let target_x = local_x
            .saturating_sub(TEXT_BOX_PAD_X)
            .saturating_add(scroll_x)
            .max(0);
        let mut best_pos = 0usize;
        let mut best_dist = target_x.abs();
        let mut pen_q2 = 0i64;
        let mut prev: Option<char> = None;
        let mut committed = self.text().chars();
        let mut pos = 0;
        for (idx, ch) in display.char_indices() {
            pen_q2 += char_adv_q2(ch, prev, font);
            prev = Some(ch);
            if !preedit.contains(&idx)
                && let Some(original) = committed.next()
            {
                pos += original.len_utf8();
            }
            let boundary_x = ((pen_q2 + 2) >> 2) as i32;
            let dist = (target_x - boundary_x).abs();
            if dist < best_dist || (idx + ch.len_utf8() == display.len() && dist == best_dist) {
                best_dist = dist;
                best_pos = pos;
            }
        }

        let changed = self.cursor != best_pos || self.preedit_len > 0;
        self.cursor = best_pos;
        self.preedit_len = 0;
        if changed {
            self.rev = self.rev.wrapping_add(1).max(1);
        }
        changed
    }
}

impl<const CAP: usize> Default for FixedTextEditor<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAP: usize> core::str::FromStr for FixedTextEditor<CAP> {
    type Err = core::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_text(s))
    }
}

#[inline]
fn utf8_prefix_len(s: &str, max_bytes: usize) -> usize {
    let mut take = 0usize;
    for (idx, ch) in s.char_indices() {
        let end = idx + ch.len_utf8();
        if end <= max_bytes {
            take = end;
        } else {
            break;
        }
    }
    take
}

fn char_adv_q2(ch: char, prev: Option<char>, font: Option<&Lf4Font<'_>>) -> i64 {
    if let Some(f) = font {
        let (g, from_fb) = f.lookup_or_fallback(ch);
        let base = if f.advance_is_q2() {
            i64::from(g.metrics.advance)
        } else {
            i64::from(g.metrics.advance) * 4
        };
        let kern = if let Some(p) = prev
            && !from_fb
            && f.has_kerning()
        {
            let k = i64::from(f.kerning(p, ch));
            if f.advance_is_q2() { k } else { k * 4 }
        } else {
            0
        };
        (base + kern).max(0)
    } else if ch.is_ascii() {
        8 * 4
    } else {
        16 * 4
    }
}

fn measure_prefix_px(text: &str, byte_end: usize, font: Option<&Lf4Font<'_>>, masked: bool) -> i32 {
    let end = byte_end.min(text.len());
    let mut pen_q2 = 0i64;
    let mut prev: Option<char> = None;
    for ch in text[..end].chars() {
        let c = if masked { '*' } else { ch };
        pen_q2 += char_adv_q2(c, prev, font);
        prev = Some(c);
    }
    ((pen_q2 + 2) >> 2) as i32
}

// ---------------------------------------------------------------------------
// 2. Pluggable ImeDict & Built-in Pinyin / T9 Table
// ---------------------------------------------------------------------------

/// Pluggable candidate lookup interface for QWERTY Pinyin (`t9 = false`) and 9-key (`t9 = true`).
pub trait ImeDict {
    /// Fill `out` with up to `out.len()` candidates starting at `offset`.
    /// Returns `(filled_count, has_more)`.
    fn query<'a>(
        &'a self,
        query: &str,
        t9: bool,
        offset: usize,
        out: &mut [&'a str],
    ) -> (usize, bool);

    /// Query candidates together with the prefix length in bytes of `query` matched.
    /// Defaults to consuming the full `query` length.
    fn query_with_prefix<'a>(
        &'a self,
        query: &str,
        t9: bool,
        offset: usize,
        out: &mut [&'a str],
    ) -> (usize, bool, usize) {
        let (n, more) = self.query(query, t9, offset, out);
        (n, more, query.len())
    }
}

/// Zero-heap static `.rodata` Pinyin/T9 dictionary combining syllable entries and phrases.
#[derive(Copy, Clone)]
pub struct StaticPinyinDict {
    phrases: &'static [(&'static str, &'static str)],
    syllables: &'static [(&'static str, &'static str)],
}

impl StaticPinyinDict {
    /// Wrap a flat phrase/entry slice (`pinyin -> candidate`) without a separate syllable table.
    pub const fn new(entries: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            phrases: entries,
            syllables: &[],
        }
    }

    /// Construct a dictionary from separate `phrases` (`ni'hao -> "你好"`) and `syllables` (`ni -> "你拟尼..."`).
    pub const fn with_syllables(
        phrases: &'static [(&'static str, &'static str)],
        syllables: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { phrases, syllables }
    }

    fn exact_only_query<'a>(
        &'a self,
        query: &str,
        t9: bool,
        offset: usize,
        out: &mut [&'a str],
    ) -> (usize, bool) {
        if query.is_empty() || out.is_empty() {
            return (0, false);
        }
        let mut matched_idx = 0usize;
        let mut filled = 0usize;

        macro_rules! push_cand {
            ($cand:expr) => {{
                if matched_idx < offset {
                    matched_idx += 1;
                } else if filled < out.len() {
                    out[filled] = $cand;
                    filled += 1;
                    matched_idx += 1;
                } else {
                    return (filled, true);
                }
            }};
        }

        // Exact phrases
        for (i, &(py, word)) in self.phrases.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            if self.phrases[..i]
                .iter()
                .any(|&(p2, w2)| w2 == word && pinyin_matches(p2, query, t9, true))
            {
                continue;
            }
            push_cand!(word);
        }

        // Stage 1: first char of each exact syllable
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            if let Some(ch) = chars.chars().next() {
                let word = &chars[..ch.len_utf8()];
                if self.exact_phrase_has(query, t9, word)
                    || self.syllables[..i]
                        .iter()
                        .any(|&(p2, c2)| pinyin_matches(p2, query, t9, true) && c2.starts_with(ch))
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        // Stage 2: remaining chars of each exact syllable
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            for (b, ch) in chars.char_indices().skip(1) {
                if chars[..b].contains(ch) {
                    continue;
                }
                let word = &chars[b..b + ch.len_utf8()];
                if self.exact_phrase_has(query, t9, word)
                    || self.exact_first_char_has(query, t9, ch)
                    || self.syllables[..i].iter().any(|&(p2, c2)| {
                        pinyin_matches(p2, query, t9, true) && tail_contains(c2, ch)
                    })
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        (filled, false)
    }

    fn exact_phrase_has(&self, query: &str, t9: bool, word: &str) -> bool {
        self.phrases
            .iter()
            .any(|&(py, w)| w == word && pinyin_matches(py, query, t9, true))
    }

    fn exact_first_char_has(&self, query: &str, t9: bool, ch: char) -> bool {
        self.syllables
            .iter()
            .any(|&(py, chars)| pinyin_matches(py, query, t9, true) && chars.starts_with(ch))
    }

    fn exact_any_char_has(&self, query: &str, t9: bool, ch: char) -> bool {
        self.syllables
            .iter()
            .any(|&(py, chars)| pinyin_matches(py, query, t9, true) && chars.contains(ch))
    }

    fn prefix_first_char_has(&self, query: &str, t9: bool, ch: char) -> bool {
        self.syllables
            .iter()
            .any(|&(py, chars)| pinyin_matches(py, query, t9, false) && chars.starts_with(ch))
    }

    fn prefix_phrase_has(&self, query: &str, t9: bool, word: &str, limit: usize) -> bool {
        let mut seen = 0usize;
        for boundary in [true, false] {
            for (i, &(py, w)) in self.phrases.iter().enumerate() {
                if !pinyin_matches(py, query, t9, false)
                    || (pinyin_first_syllable_exact(py, query, t9) != boundary)
                {
                    continue;
                }
                if self.exact_phrase_has(query, t9, w)
                    || (w.chars().count() == 1
                        && self.exact_first_char_has(query, t9, w.chars().next().unwrap()))
                    || (!boundary
                        && self.phrases.iter().any(|&(pa, wa)| {
                            wa == w
                                && pinyin_matches(pa, query, t9, false)
                                && pinyin_first_syllable_exact(pa, query, t9)
                        }))
                    || self.phrases[..i].iter().any(|&(p2, w2)| {
                        w2 == w
                            && pinyin_matches(p2, query, t9, false)
                            && (pinyin_first_syllable_exact(p2, query, t9) == boundary)
                    })
                {
                    continue;
                }
                if w == word {
                    return true;
                }
                seen += 1;
                if seen >= limit {
                    return false;
                }
            }
        }
        false
    }
}

fn pinyin_first_syllable_exact(pinyin: &str, query: &str, t9: bool) -> bool {
    let pb = pinyin.as_bytes();
    let qb = query.as_bytes();
    if qb.is_empty() || pb.len() <= qb.len() || pb[qb.len()] != b'\'' {
        return false;
    }
    for i in 0..qb.len() {
        let lhs = if t9 { ascii_to_t9_digit(pb[i]) } else { pb[i] };
        if lhs != qb[i] {
            return false;
        }
    }
    true
}

#[inline]
fn tail_contains(chars: &str, ch: char) -> bool {
    let mut it = chars.chars();
    let _ = it.next();
    it.any(|c| c == ch)
}

#[inline]
const fn ascii_to_t9_digit(b: u8) -> u8 {
    match b {
        b'a'..=b'c' => b'2',
        b'd'..=b'f' => b'3',
        b'g'..=b'i' => b'4',
        b'j'..=b'l' => b'5',
        b'm'..=b'o' => b'6',
        b'p'..=b's' => b'7',
        b't'..=b'v' => b'8',
        b'w'..=b'z' => b'9',
        _ => b'?',
    }
}

fn pinyin_matches(pinyin: &str, query: &str, t9: bool, exact: bool) -> bool {
    let qb = query.as_bytes();
    if qb.is_empty() {
        return false;
    }
    let mut qi = 0usize;
    let mut has_trailing = false;
    for &b in pinyin.as_bytes() {
        if b == b'\'' {
            continue;
        }
        if qi < qb.len() {
            let lhs = if t9 { ascii_to_t9_digit(b) } else { b };
            if lhs != qb[qi] {
                return false;
            }
            qi += 1;
        } else {
            has_trailing = true;
            break;
        }
    }
    if qi < qb.len() {
        return false;
    }
    if exact { !has_trailing } else { has_trailing }
}

fn initials_match(py: &str, query: &str, t9: bool) -> bool {
    let qb = query.as_bytes();
    if qb.len() < 2 {
        return false;
    }
    let pb = py.as_bytes();
    if !pb.contains(&b'\'') {
        return false;
    }
    let mut qi = 0usize;
    let mut take_next = true;
    for &b in pb {
        if b == b'\'' {
            take_next = true;
            continue;
        }
        if take_next {
            take_next = false;
            let lhs = if t9 { ascii_to_t9_digit(b) } else { b };
            if lhs != qb[qi] {
                return false;
            }
            qi += 1;
            if qi == qb.len() {
                return true;
            }
        }
    }
    false
}

impl ImeDict for StaticPinyinDict {
    fn query<'a>(
        &'a self,
        query: &str,
        t9: bool,
        offset: usize,
        out: &mut [&'a str],
    ) -> (usize, bool) {
        if query.is_empty() || out.is_empty() {
            return (0, false);
        }
        let mut matched_idx = 0usize;
        let mut filled = 0usize;

        macro_rules! push_cand {
            ($cand:expr) => {{
                if matched_idx < offset {
                    matched_idx += 1;
                } else if filled < out.len() {
                    out[filled] = $cand;
                    filled += 1;
                    matched_idx += 1;
                } else {
                    return (filled, true);
                }
            }};
        }

        // Pass 1: exact phrases
        for (i, &(py, word)) in self.phrases.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            if self.phrases[..i]
                .iter()
                .any(|&(p2, w2)| w2 == word && pinyin_matches(p2, query, t9, true))
            {
                continue;
            }
            push_cand!(word);
        }

        // Pass 2: first char of each exact syllable
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            if let Some(ch) = chars.chars().next() {
                let word = &chars[..ch.len_utf8()];
                if self.exact_phrase_has(query, t9, word)
                    || self.syllables[..i]
                        .iter()
                        .any(|&(p2, c2)| pinyin_matches(p2, query, t9, true) && c2.starts_with(ch))
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        // Pass 3: up to 2 strict-prefix phrases (prioritizing exact first-syllable boundary matches)
        let mut prefix_phrase_emitted = 0usize;
        let mut a_resume = self.phrases.len();
        let mut b_resume = 0usize;
        for (i, &(py, word)) in self.phrases.iter().enumerate() {
            if !pinyin_first_syllable_exact(py, query, t9) {
                continue;
            }
            if self.exact_phrase_has(query, t9, word)
                || (word.chars().count() == 1
                    && self.exact_first_char_has(query, t9, word.chars().next().unwrap()))
                || self.phrases[..i]
                    .iter()
                    .any(|&(p2, w2)| w2 == word && pinyin_first_syllable_exact(p2, query, t9))
            {
                continue;
            }
            push_cand!(word);
            prefix_phrase_emitted += 1;
            if prefix_phrase_emitted == 2 {
                a_resume = i + 1;
                break;
            }
        }
        if prefix_phrase_emitted < 2 {
            b_resume = self.phrases.len();
            for (i, &(py, word)) in self.phrases.iter().enumerate() {
                if !pinyin_matches(py, query, t9, false)
                    || pinyin_first_syllable_exact(py, query, t9)
                {
                    continue;
                }
                if self.exact_phrase_has(query, t9, word)
                    || (word.chars().count() == 1
                        && self.exact_first_char_has(query, t9, word.chars().next().unwrap()))
                    || self
                        .phrases
                        .iter()
                        .any(|&(pa, wa)| wa == word && pinyin_first_syllable_exact(pa, query, t9))
                    || self.phrases[..i].iter().any(|&(p2, w2)| {
                        w2 == word
                            && pinyin_matches(p2, query, t9, false)
                            && !pinyin_first_syllable_exact(p2, query, t9)
                    })
                {
                    continue;
                }
                push_cand!(word);
                prefix_phrase_emitted += 1;
                if prefix_phrase_emitted == 2 {
                    b_resume = i + 1;
                    break;
                }
            }
        }

        // Pass 4: remaining chars of exact syllables
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, true) {
                continue;
            }
            for (b, ch) in chars.char_indices().skip(1) {
                if chars[..b].contains(ch) {
                    continue;
                }
                let word = &chars[b..b + ch.len_utf8()];
                if self.exact_phrase_has(query, t9, word)
                    || self.exact_first_char_has(query, t9, ch)
                    || self.prefix_phrase_has(query, t9, word, 2)
                    || self.syllables[..i].iter().any(|&(p2, c2)| {
                        pinyin_matches(p2, query, t9, true) && tail_contains(c2, ch)
                    })
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        // Pass 5: remaining strict-prefix phrases (remaining exact-boundary first, then partial-boundary)
        for (i, &(py, word)) in self.phrases.iter().enumerate().skip(a_resume) {
            if !pinyin_first_syllable_exact(py, query, t9) {
                continue;
            }
            if self.exact_phrase_has(query, t9, word)
                || (word.chars().count() == 1
                    && self.exact_any_char_has(query, t9, word.chars().next().unwrap()))
                || self.phrases[..i]
                    .iter()
                    .any(|&(p2, w2)| w2 == word && pinyin_first_syllable_exact(p2, query, t9))
            {
                continue;
            }
            push_cand!(word);
        }
        for (i, &(py, word)) in self.phrases.iter().enumerate().skip(b_resume) {
            if !pinyin_matches(py, query, t9, false) || pinyin_first_syllable_exact(py, query, t9) {
                continue;
            }
            if self.exact_phrase_has(query, t9, word)
                || (word.chars().count() == 1
                    && self.exact_any_char_has(query, t9, word.chars().next().unwrap()))
                || self
                    .phrases
                    .iter()
                    .any(|&(pa, wa)| wa == word && pinyin_first_syllable_exact(pa, query, t9))
                || self.phrases[..i].iter().any(|&(p2, w2)| {
                    w2 == word
                        && pinyin_matches(p2, query, t9, false)
                        && !pinyin_first_syllable_exact(p2, query, t9)
                })
            {
                continue;
            }
            push_cand!(word);
        }

        // Pass 6: first char of each strict-prefix syllable
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, false) {
                continue;
            }
            if let Some(ch) = chars.chars().next() {
                let word = &chars[..ch.len_utf8()];
                if self.exact_any_char_has(query, t9, ch)
                    || self.exact_phrase_has(query, t9, word)
                    || self.prefix_phrase_has(query, t9, word, usize::MAX)
                    || self.syllables[..i]
                        .iter()
                        .any(|&(p2, c2)| pinyin_matches(p2, query, t9, false) && c2.starts_with(ch))
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        // Pass 7: remaining chars of strict-prefix syllables
        for (i, &(py, chars)) in self.syllables.iter().enumerate() {
            if !pinyin_matches(py, query, t9, false) {
                continue;
            }
            for (b, ch) in chars.char_indices().skip(1) {
                if chars[..b].contains(ch) {
                    continue;
                }
                let word = &chars[b..b + ch.len_utf8()];
                if self.exact_any_char_has(query, t9, ch)
                    || self.prefix_first_char_has(query, t9, ch)
                    || self.exact_phrase_has(query, t9, word)
                    || self.prefix_phrase_has(query, t9, word, usize::MAX)
                    || self.syllables[..i].iter().any(|&(p2, c2)| {
                        pinyin_matches(p2, query, t9, false) && tail_contains(c2, ch)
                    })
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        // Pass 8: phrase initials (简拼), e.g. "nh" -> "你好", "sb" -> "设备"
        if matched_idx == 0 && query.len() >= 2 {
            for (i, &(py, word)) in self.phrases.iter().enumerate() {
                if !initials_match(py, query, t9) {
                    continue;
                }
                if self.phrases[..i]
                    .iter()
                    .any(|&(p2, w2)| w2 == word && initials_match(p2, query, t9))
                {
                    continue;
                }
                push_cand!(word);
            }
        }

        (filled, false)
    }

    fn query_with_prefix<'a>(
        &'a self,
        query: &str,
        t9: bool,
        offset: usize,
        out: &mut [&'a str],
    ) -> (usize, bool, usize) {
        if query.is_empty() || out.is_empty() {
            return (0, false, query.len());
        }
        let (n, more) = self.query(query, t9, offset, out);
        if n > 0 {
            return (n, more, query.len());
        }
        if offset > 0 {
            let mut probe = [""; 1];
            if self.query(query, t9, 0, &mut probe).0 > 0 {
                return (0, false, query.len());
            }
        }
        let mut plen = query.len().saturating_sub(1);
        while plen >= 1 {
            let sub = &query[..plen];
            let (sn, smore) = self.exact_only_query(sub, t9, offset, out);
            if sn > 0 {
                return (sn, smore, plen);
            }
            if offset > 0 {
                let mut probe = [""; 1];
                if self.exact_only_query(sub, t9, 0, &mut probe).0 > 0 {
                    return (0, false, plen);
                }
            }
            plen -= 1;
        }
        (0, false, query.len())
    }
}

/// Built-in 411-syllable Mandarin Pinyin/T9 dictionary + common phrases in `.rodata`.
pub const DEFAULT_PINYIN_DICT: StaticPinyinDict =
    StaticPinyinDict::with_syllables(DEFAULT_PHRASES, DEFAULT_SYLLABLES);

// ---------------------------------------------------------------------------
// 3. ImeState — Multi-mode keyboard state machine
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum InputMode {
    #[default]
    ZhPinyin = 0,
    ZhT9 = 1,
    EnLower = 2,
    EnUpper = 3,
    Numeric = 4,
    Symbols = 5,
}

impl InputMode {
    #[inline]
    pub const fn is_chinese(self) -> bool {
        matches!(self, Self::ZhPinyin | Self::ZhT9)
    }

    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ZhPinyin => "Pin",
            Self::ZhT9 => "T9",
            Self::EnLower => "en",
            Self::EnUpper => "EN",
            Self::Numeric => "123",
            Self::Symbols => "Sym",
        }
    }
}

/// Logical output event produced by `ImeState` or `SoftKeyboard`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImeEvent<'a> {
    None,
    Text(TextInput<'a>),
    /// Commit chosen prefix candidate while leaving remaining Pinyin/T9 preedit active.
    CommitCompose {
        commit: &'a str,
        compose: &'a str,
    },
    Done,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum HitAction {
    Mode(InputMode),
    Candidate(u8),
    PrevPage,
    NextPage,
    Char(&'static str),
    Shift,
    Backspace,
    Space,
    ClearCode,
    CursorLeft,
    ToggleZhEn,
    Done,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct HitButton<'a> {
    rect: Rect,
    action: HitAction,
    label: &'a str,
    accent: bool,
    secondary: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ImeState {
    mode: InputMode,
    code: [u8; MAX_CODE_BYTES],
    code_len: usize,
    page: u8,
    shift_once: bool,
    pressed_action: Option<HitAction>,
    rev: u32,
}

impl ImeState {
    pub const fn new(mode: InputMode) -> Self {
        Self {
            mode,
            code: [0; MAX_CODE_BYTES],
            code_len: 0,
            page: 0,
            shift_once: false,
            pressed_action: None,
            rev: 1,
        }
    }

    #[inline]
    pub const fn mode(&self) -> InputMode {
        self.mode
    }

    #[inline]
    pub fn code(&self) -> &str {
        core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("")
    }

    #[inline]
    pub const fn revision(&self) -> u32 {
        self.rev
    }

    #[inline]
    fn bump(&mut self) {
        self.rev = self.rev.wrapping_add(1).max(1);
    }

    pub fn set_mode(&mut self, mode: InputMode) -> ImeEvent<'static> {
        let cancel = self.code_len > 0;
        self.code_len = 0;
        self.page = 0;
        self.mode = mode;
        self.shift_once = false;
        self.bump();
        if cancel {
            ImeEvent::Text(TextInput::Cancel)
        } else {
            ImeEvent::None
        }
    }

    pub fn clear_composing(&mut self) -> ImeEvent<'static> {
        if self.code_len > 0 {
            self.code_len = 0;
            self.page = 0;
            self.bump();
            ImeEvent::Text(TextInput::Cancel)
        } else {
            ImeEvent::None
        }
    }

    pub fn press_char(&mut self, s: &'static str) -> ImeEvent<'_> {
        if self.mode == InputMode::ZhPinyin && s.len() == 1 && s.as_bytes()[0].is_ascii_lowercase()
        {
            if self.code_len < MAX_CODE_BYTES {
                self.code[self.code_len] = s.as_bytes()[0];
                self.code_len += 1;
                self.page = 0;
                self.bump();
            }
            let code_str = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
            return ImeEvent::Text(TextInput::Compose(code_str));
        }

        let cancel_first = self.code_len > 0;
        if cancel_first {
            self.code_len = 0;
            self.page = 0;
        }
        if self.shift_once && self.mode == InputMode::EnUpper {
            self.mode = InputMode::EnLower;
            self.shift_once = false;
        }
        self.bump();
        ImeEvent::Text(TextInput::Commit(s))
    }

    pub fn press_t9_digit(&mut self, digit: u8) -> ImeEvent<'_> {
        if self.mode == InputMode::ZhT9 && (b'2'..=b'9').contains(&digit) {
            if self.code_len < MAX_CODE_BYTES {
                self.code[self.code_len] = digit;
                self.code_len += 1;
                self.page = 0;
                self.bump();
            }
            let code_str = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
            return ImeEvent::Text(TextInput::Compose(code_str));
        }
        ImeEvent::None
    }

    pub fn press_backspace(&mut self) -> ImeEvent<'_> {
        if self.code_len > 0 {
            self.code_len -= 1;
            self.page = 0;
            self.bump();
            if self.code_len == 0 {
                return ImeEvent::Text(TextInput::Cancel);
            }
            let code_str = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
            return ImeEvent::Text(TextInput::Compose(code_str));
        }
        self.bump();
        ImeEvent::Text(TextInput::Backspace)
    }

    pub fn select_candidate<'a, D: ImeDict>(
        &'a mut self,
        slot_idx: usize,
        dict: &'a D,
    ) -> ImeEvent<'a> {
        if self.code_len == 0 || !self.mode.is_chinese() {
            return ImeEvent::None;
        }
        let mut cands = [""; MAX_CANDS_SHOWN];
        let offset = usize::from(self.page) * MAX_CANDS_SHOWN;
        let query = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
        let t9 = self.mode == InputMode::ZhT9;
        let (n, _, consumed) = dict.query_with_prefix(query, t9, offset, &mut cands);
        let Some(&chosen) = cands[..n].get(slot_idx) else {
            return ImeEvent::None;
        };
        let consumed = consumed.min(self.code_len);
        if consumed < self.code_len {
            let rem = self.code_len - consumed;
            self.code.copy_within(consumed..self.code_len, 0);
            self.code_len = rem;
            self.page = 0;
            self.bump();
            let compose = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
            ImeEvent::CommitCompose {
                commit: chosen,
                compose,
            }
        } else {
            self.code_len = 0;
            self.page = 0;
            self.bump();
            ImeEvent::Text(TextInput::Commit(chosen))
        }
    }

    pub fn press_space<'a, D: ImeDict>(&'a mut self, dict: &'a D) -> ImeEvent<'a> {
        if self.code_len > 0 && self.mode.is_chinese() {
            let mut probe = [""; 1];
            let offset = usize::from(self.page) * MAX_CANDS_SHOWN;
            let query = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
            let t9 = self.mode == InputMode::ZhT9;
            if dict.query_with_prefix(query, t9, offset, &mut probe).0 > 0 {
                return self.select_candidate(0, dict);
            }
            self.code_len = 0;
            self.page = 0;
            self.bump();
            return ImeEvent::Text(TextInput::Cancel);
        }
        self.bump();
        ImeEvent::Text(TextInput::Commit(" "))
    }

    pub fn prev_page(&mut self) -> bool {
        if self.page > 0 {
            self.page -= 1;
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn next_page<D: ImeDict>(&mut self, dict: &D) -> bool {
        if self.code_len == 0 || !self.mode.is_chinese() {
            return false;
        }
        let mut cands = [""; MAX_CANDS_SHOWN];
        let offset = usize::from(self.page) * MAX_CANDS_SHOWN;
        let query = core::str::from_utf8(&self.code[..self.code_len]).unwrap_or("");
        let t9 = self.mode == InputMode::ZhT9;
        let (_, more, _) = dict.query_with_prefix(query, t9, offset, &mut cands);
        if more {
            self.page = self.page.saturating_add(1);
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn toggle_shift(&mut self) {
        self.mode = match self.mode {
            InputMode::EnLower => {
                self.shift_once = true;
                InputMode::EnUpper
            }
            InputMode::EnUpper => {
                self.shift_once = false;
                InputMode::EnLower
            }
            _ => self.mode,
        };
        self.bump();
    }

    pub fn toggle_zh_en(&mut self) -> ImeEvent<'static> {
        let next = match self.mode {
            InputMode::ZhPinyin | InputMode::ZhT9 => InputMode::EnLower,
            _ => InputMode::ZhPinyin,
        };
        self.set_mode(next)
    }

    fn apply_action<'a, D: ImeDict>(&'a mut self, act: HitAction, dict: &'a D) -> ImeEvent<'a> {
        match act {
            HitAction::Mode(m) => self.set_mode(m),
            HitAction::Candidate(idx) => self.select_candidate(usize::from(idx), dict),
            HitAction::PrevPage => {
                self.prev_page();
                ImeEvent::None
            }
            HitAction::NextPage => {
                self.next_page(dict);
                ImeEvent::None
            }
            HitAction::Char(s) => {
                if self.mode == InputMode::ZhT9
                    && s.len() == 1
                    && (b'2'..=b'9').contains(&s.as_bytes()[0])
                {
                    return self.press_t9_digit(s.as_bytes()[0]);
                }
                self.press_char(s)
            }
            HitAction::Shift => {
                self.toggle_shift();
                ImeEvent::None
            }
            HitAction::Backspace => self.press_backspace(),
            HitAction::Space => self.press_space(dict),
            HitAction::ClearCode => self.clear_composing(),
            HitAction::CursorLeft => {
                self.code_len = 0;
                self.page = 0;
                self.bump();
                ImeEvent::Text(TextInput::Left)
            }
            HitAction::ToggleZhEn => self.toggle_zh_en(),
            HitAction::Done => ImeEvent::Done,
        }
    }

    /// Lookup the widget-local rectangle of the keycap or candidate chip matching `label`.
    pub fn key_rect(&self, dict: &impl ImeDict, size: Size, label: &str) -> Option<Rect> {
        let mut found = None;
        let _ = emit_keyboard_layout(self, dict, size, |btn| {
            if found.is_none() && btn.label == label {
                found = Some(btn.rect);
            }
            Ok(())
        });
        found
    }
}

impl Default for ImeState {
    fn default() -> Self {
        Self::new(InputMode::ZhPinyin)
    }
}

// ---------------------------------------------------------------------------
// 4. Single-Node TextBox & SoftKeyboard Widgets
// ---------------------------------------------------------------------------

/// Declare a single-node focusable, scrollable, touch-editable UTF-8 `TextBox` (caret active).
#[track_caller]
pub fn text_box<const CAP: usize>(
    ui: &mut Children<'_, '_>,
    key: Key,
    editor: &FixedTextEditor<CAP>,
    placeholder: &str,
    theme: Theme,
) -> Result<(), BuildError> {
    text_box_with_focus(ui, key, editor, placeholder, true, theme)
}

/// Declare a single-node UTF-8 `TextBox` with explicit active/caret visibility for multi-field forms.
#[track_caller]
pub fn text_box_with_focus<const CAP: usize>(
    ui: &mut Children<'_, '_>,
    key: Key,
    editor: &FixedTextEditor<CAP>,
    placeholder: &str,
    active: bool,
    theme: Theme,
) -> Result<(), BuildError> {
    let text_ref = if editor.is_empty() {
        ui.text_dyn(placeholder)?
    } else {
        let mut tmp = [0u8; MAX_DISPLAY_BYTES];
        let (disp, _, _, _) = editor.format_display(&mut tmp);
        ui.text_dyn(disp)?
    };
    let h = u32::from(theme.density.control_height).max(28);
    ui.custom(
        key,
        NodeProps {
            text: text_ref,
            bg: Some(theme.surface),
            fg: theme.text,
            accent: theme.accent,
            focus_color: theme.focus,
            focus_ring: theme.interaction.focus,
            border_color: Some(theme.border),
            border_width: theme.shape.border_width,
            radius: theme.shape.radius_md,
            font: FontId::Body16,
            width: Length::Fill(1),
            height: Length::Px(h),
            pointer_capture: PointerCaptureMode::Free,
            checked: active,
            clickable: true,
            focusable: true,
            style_source: theme.source.declared(),
            ..NodeProps::new()
        },
    )?;
    ui.set_custom_rev(key, editor.revision())
}

/// Paint a `TextBox` (border, scrolled display text, preedit underline, and 2 px focus caret).
pub fn paint_text_box<const CAP: usize>(
    editor: &FixedTextEditor<CAP>,
    _placeholder: &str,
    props: &StoredProps,
    state: PaintState,
    font: Option<&Lf4Font<'_>>,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    if sz.width < 8 || sz.height < 10 {
        return Ok(());
    }
    let active = state.focused || state.checked;
    let border_col = if active {
        props.focus_color
    } else {
        props.border_color().unwrap_or(props.accent)
    };
    canvas.rounded_rect(
        Rect::new(0, 0, sz.width, sz.height),
        props.bg().unwrap_or(Rgb565::new(3, 5, 8)),
        props.radius,
        Some(border_col),
        props.border_width.max(1),
    )?;

    let inner = Rect::new(
        TEXT_BOX_PAD_X,
        2,
        sz.width.saturating_sub((TEXT_BOX_PAD_X * 2) as u32),
        sz.height.saturating_sub(4),
    );
    if inner.w == 0 || inner.h == 0 {
        return Ok(());
    }

    let line_h = font.map_or(16, |f| f.line_height().max(12).min(inner.h));
    let text_y = inner.y + ((inner.h.saturating_sub(line_h)) / 2) as i32;

    canvas.push_clip(inner);
    if editor.is_empty() {
        let placeholder_col = blend_muted(props.fg, props.bg().unwrap_or(Rgb565::new(3, 5, 8)));
        canvas.node_text(
            Rect::new(inner.x, text_y, inner.w, line_h),
            props,
            placeholder_col,
            TextAlign::Start,
        )?;
        if active {
            canvas.fill(Rect::new(inner.x, text_y, 2, line_h.min(18)), props.accent)?;
        }
    } else {
        let mut tmp = [0u8; MAX_DISPLAY_BYTES];
        let (disp, pre_start, pre_end, caret_byte) = editor.format_display(&mut tmp);
        let masked = false; // `format_display` already masked committed chars as '*'
        let total_w = measure_prefix_px(disp, disp.len(), font, masked);
        let caret_x = measure_prefix_px(disp, caret_byte, font, masked);
        let scroll_x = (caret_x + 3 - inner.w as i32).max(0);

        let draw_w = (total_w.max(0) as u32).saturating_add(16).max(inner.w);
        canvas.node_text(
            Rect::new(inner.x - scroll_x, text_y, draw_w, line_h),
            props,
            props.fg,
            TextAlign::Start,
        )?;

        if pre_end > pre_start {
            let x0 = measure_prefix_px(disp, pre_start, font, masked) - scroll_x;
            let x1 = measure_prefix_px(disp, pre_end, font, masked) - scroll_x;
            let uy = (text_y + line_h as i32).min(inner.y + inner.h as i32 - 1);
            if x1 > x0 {
                canvas.line(
                    Point::new(inner.x + x0, uy),
                    Point::new(inner.x + x1, uy),
                    props.accent,
                    1,
                    false,
                )?;
            }
        }
        if active {
            let w = inner.w.min(2);
            let cx = (inner.x + caret_x - scroll_x).clamp(inner.x, inner.x + (inner.w - w) as i32);
            canvas.fill(Rect::new(cx, text_y, w, line_h.min(18)), props.accent)?;
        }
    }
    canvas.pop_clip();
    Ok(())
}

/// Handle touch press on `TextBox` to reposition cursor.
pub fn text_box_on_pointer<const CAP: usize>(
    editor: &mut FixedTextEditor<CAP>,
    ev: PointerEvent,
    font: Option<&Lf4Font<'_>>,
) -> bool {
    if ev.phase != PointerPhase::Down {
        return false;
    }
    let inner_w = ev.size.width.saturating_sub((TEXT_BOX_PAD_X * 2) as u32) as i32;
    let mut tmp = [0u8; MAX_DISPLAY_BYTES];
    let (disp, start, end, caret_byte) = editor.format_display(&mut tmp);
    let caret_x = measure_prefix_px(disp, caret_byte, font, false);
    let scroll_x = (caret_x + 3 - inner_w).max(0);
    editor.place_display_cursor(ev.local.x, scroll_x, font, disp, start..end)
}

#[inline]
fn blend_muted(fg: Rgb565, bg: Rgb565) -> Rgb565 {
    use liuli::core::geometry::{IntoStorage, RawU16};
    let f = fg.into_storage();
    let b = bg.into_storage();
    let r = (((f >> 11) + (b >> 11)) / 2) & 0x1F;
    let g = ((((f >> 5) & 0x3F) + ((b >> 5) & 0x3F)) / 2) & 0x3F;
    let bl = (((f & 0x1F) + (b & 0x1F)) / 2) & 0x1F;
    Rgb565::from(RawU16::new((r << 11) | (g << 5) | bl))
}

/// Declare the single-node `SoftKeyboard` widget (candidate bar + multi-mode keycap grid).
#[track_caller]
pub fn soft_keyboard(
    ui: &mut Children<'_, '_>,
    key: Key,
    state: &ImeState,
    height: u16,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.custom(
        key,
        NodeProps {
            bg: Some(theme.background),
            fg: theme.text,
            accent: theme.accent,
            focus_color: theme.focus,
            border_color: Some(theme.border),
            radius: theme.shape.radius_md,
            font: FontId::Small12,
            width: Length::Fill(1),
            height: Length::Px(height.into()),
            pointer_capture: PointerCaptureMode::Free,
            clickable: true,
            focusable: false,
            style_source: theme.source.declared(),
            ..NodeProps::new()
        },
    )?;
    ui.set_custom_rev(key, state.revision())
}

fn emit_key_row<'a>(
    w: i32,
    pad: i32,
    gap: i32,
    ry: i32,
    row_h: i32,
    specs: &[(&'a str, HitAction, u8, bool, bool)],
    emit: &mut impl FnMut(HitButton<'a>) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let count = specs.len() as i32;
    let total_units: i32 = specs.iter().map(|s| i32::from(s.2)).sum();
    let avail = (w - pad * 2 - gap * (count - 1)).max(count);
    let mut x = pad;
    for (idx, &(lbl, act, units, accent, secondary)) in specs.iter().enumerate() {
        let kw = if idx + 1 == specs.len() {
            (w - pad - x).max(4)
        } else {
            (avail * i32::from(units) / total_units.max(1)).max(4)
        };
        emit(HitButton {
            rect: Rect::new(x, ry, kw as u32, row_h as u32),
            action: act,
            label: lbl,
            accent,
            secondary,
        })?;
        x += kw + gap;
    }
    Ok(())
}

fn emit_keyboard_layout<'a, D: ImeDict>(
    state: &'a ImeState,
    dict: &'a D,
    size: Size,
    mut emit: impl FnMut(HitButton<'a>) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let w = size.width as i32;
    let h = size.height as i32;
    if w < 120 || h < 80 {
        return Ok(());
    }
    let pad = 3i32;
    let gap = 2i32;
    let bar_h = ((h / 5).clamp(22, 30)) - 2;
    let bar_y = pad;
    let grid_y = bar_y + bar_h + gap + 1;
    let grid_h = (h - grid_y - pad).max(40);
    let row_h = ((grid_h - gap * 3) / 4).max(8);

    // Top bar: candidate strip when Chinese composing, otherwise quick mode switcher + OK.
    let mut cands = [""; MAX_CANDS_SHOWN];
    let query = state.code();
    let has_cands = if !query.is_empty() && state.mode.is_chinese() {
        let offset = usize::from(state.page) * MAX_CANDS_SHOWN;
        let (cnt, has_more, _) =
            dict.query_with_prefix(query, state.mode == InputMode::ZhT9, offset, &mut cands);
        let inner_w = w - pad * 2;
        let pager_w = 20i32;
        let code_w = ((query.len() as i32) * 8 + 12).clamp(32, inner_w / 3);
        emit(HitButton {
            rect: Rect::new(pad, bar_y, code_w as u32, bar_h as u32),
            action: HitAction::ClearCode,
            label: query,
            accent: false,
            secondary: true,
        })?;
        let prev_x = pad + code_w + gap;
        emit(HitButton {
            rect: Rect::new(prev_x, bar_y, pager_w as u32, bar_h as u32),
            action: HitAction::PrevPage,
            label: "<",
            accent: false,
            secondary: true,
        })?;
        let next_x = w - pad - pager_w;
        emit(HitButton {
            rect: Rect::new(next_x, bar_y, pager_w as u32, bar_h as u32),
            action: HitAction::NextPage,
            label: if has_more { ">" } else { "." },
            accent: false,
            secondary: true,
        })?;
        let area_x = prev_x + pager_w + gap;
        let area_w = (next_x - gap - area_x).max(0);
        if cnt > 0 && area_w > 10 {
            let slot_w = (area_w - gap * (cnt as i32 - 1)) / (cnt as i32);
            for (i, &cand) in cands[..cnt].iter().enumerate() {
                let cx = area_x + (i as i32) * (slot_w + gap);
                emit(HitButton {
                    rect: Rect::new(cx, bar_y, slot_w.max(1) as u32, bar_h as u32),
                    action: HitAction::Candidate(i as u8),
                    label: cand,
                    accent: i == 0,
                    secondary: false,
                })?;
            }
        }
        true
    } else {
        false
    };

    if !has_cands {
        let tabs: [(InputMode, &str); 5] = [
            (InputMode::ZhPinyin, "Pin"),
            (InputMode::ZhT9, "T9"),
            (InputMode::EnLower, "En"),
            (InputMode::Numeric, "123"),
            (InputMode::Symbols, "Sym"),
        ];
        let cols = 6i32;
        let avail = w - pad * 2 - gap * (cols - 1);
        let cw = (avail / cols).max(8);
        for (i, &(m, lbl)) in tabs.iter().enumerate() {
            let active = match (m, state.mode) {
                (InputMode::EnLower, InputMode::EnLower | InputMode::EnUpper) => true,
                _ => m == state.mode,
            };
            let x = pad + (i as i32) * (cw + gap);
            emit(HitButton {
                rect: Rect::new(x, bar_y, cw as u32, bar_h as u32),
                action: HitAction::Mode(m),
                label: lbl,
                accent: active,
                secondary: true,
            })?;
        }
        let ok_x = pad + 5 * (cw + gap);
        let ok_w = (w - pad - ok_x).max(8);
        emit(HitButton {
            rect: Rect::new(ok_x, bar_y, ok_w as u32, bar_h as u32),
            action: HitAction::Done,
            label: "OK",
            accent: true,
            secondary: false,
        })?;
    }

    let row_y = |r: i32| grid_y + r * (row_h + gap);

    match state.mode {
        InputMode::ZhPinyin | InputMode::EnLower | InputMode::EnUpper => {
            let upper = state.mode == InputMode::EnUpper;
            let r0 = if upper {
                [
                    ("Q", HitAction::Char("Q"), 2, false, false),
                    ("W", HitAction::Char("W"), 2, false, false),
                    ("E", HitAction::Char("E"), 2, false, false),
                    ("R", HitAction::Char("R"), 2, false, false),
                    ("T", HitAction::Char("T"), 2, false, false),
                    ("Y", HitAction::Char("Y"), 2, false, false),
                    ("U", HitAction::Char("U"), 2, false, false),
                    ("I", HitAction::Char("I"), 2, false, false),
                    ("O", HitAction::Char("O"), 2, false, false),
                    ("P", HitAction::Char("P"), 2, false, false),
                ]
            } else {
                [
                    ("q", HitAction::Char("q"), 2, false, false),
                    ("w", HitAction::Char("w"), 2, false, false),
                    ("e", HitAction::Char("e"), 2, false, false),
                    ("r", HitAction::Char("r"), 2, false, false),
                    ("t", HitAction::Char("t"), 2, false, false),
                    ("y", HitAction::Char("y"), 2, false, false),
                    ("u", HitAction::Char("u"), 2, false, false),
                    ("i", HitAction::Char("i"), 2, false, false),
                    ("o", HitAction::Char("o"), 2, false, false),
                    ("p", HitAction::Char("p"), 2, false, false),
                ]
            };
            emit_key_row(w, pad, gap, row_y(0), row_h, &r0, &mut emit)?;

            let r1 = if upper {
                [
                    ("A", HitAction::Char("A"), 2, false, false),
                    ("S", HitAction::Char("S"), 2, false, false),
                    ("D", HitAction::Char("D"), 2, false, false),
                    ("F", HitAction::Char("F"), 2, false, false),
                    ("G", HitAction::Char("G"), 2, false, false),
                    ("H", HitAction::Char("H"), 2, false, false),
                    ("J", HitAction::Char("J"), 2, false, false),
                    ("K", HitAction::Char("K"), 2, false, false),
                    ("L", HitAction::Char("L"), 2, false, false),
                ]
            } else {
                [
                    ("a", HitAction::Char("a"), 2, false, false),
                    ("s", HitAction::Char("s"), 2, false, false),
                    ("d", HitAction::Char("d"), 2, false, false),
                    ("f", HitAction::Char("f"), 2, false, false),
                    ("g", HitAction::Char("g"), 2, false, false),
                    ("h", HitAction::Char("h"), 2, false, false),
                    ("j", HitAction::Char("j"), 2, false, false),
                    ("k", HitAction::Char("k"), 2, false, false),
                    ("l", HitAction::Char("l"), 2, false, false),
                ]
            };
            // Half-key horizontal inset for staggered middle QWERTY row.
            let mid_inset = (w / 22).max(4);
            let ry1 = row_y(1);
            let avail1 = (w - (pad + mid_inset) * 2 - gap * 8).max(9);
            let mut x1 = pad + mid_inset;
            let kw1 = avail1 / 9;
            for (idx, &(lbl, act, _, accent, secondary)) in r1.iter().enumerate() {
                let wk = if idx == 8 {
                    w - pad - mid_inset - x1
                } else {
                    kw1
                };
                emit(HitButton {
                    rect: Rect::new(x1, ry1, wk.max(4) as u32, row_h as u32),
                    action: act,
                    label: lbl,
                    accent,
                    secondary,
                })?;
                x1 += wk + gap;
            }

            let r2 = if upper {
                [
                    ("cap", HitAction::Shift, 3, true, true),
                    ("Z", HitAction::Char("Z"), 2, false, false),
                    ("X", HitAction::Char("X"), 2, false, false),
                    ("C", HitAction::Char("C"), 2, false, false),
                    ("V", HitAction::Char("V"), 2, false, false),
                    ("B", HitAction::Char("B"), 2, false, false),
                    ("N", HitAction::Char("N"), 2, false, false),
                    ("M", HitAction::Char("M"), 2, false, false),
                    ("Del", HitAction::Backspace, 3, false, true),
                ]
            } else {
                let first_lbl = if state.mode == InputMode::ZhPinyin {
                    "T9"
                } else {
                    "Cap"
                };
                let first_act = if state.mode == InputMode::ZhPinyin {
                    HitAction::Mode(InputMode::ZhT9)
                } else {
                    HitAction::Shift
                };
                [
                    (first_lbl, first_act, 3, false, true),
                    ("z", HitAction::Char("z"), 2, false, false),
                    ("x", HitAction::Char("x"), 2, false, false),
                    ("c", HitAction::Char("c"), 2, false, false),
                    ("v", HitAction::Char("v"), 2, false, false),
                    ("b", HitAction::Char("b"), 2, false, false),
                    ("n", HitAction::Char("n"), 2, false, false),
                    ("m", HitAction::Char("m"), 2, false, false),
                    ("Del", HitAction::Backspace, 3, false, true),
                ]
            };
            emit_key_row(w, pad, gap, row_y(2), row_h, &r2, &mut emit)?;

            let r3 = [
                ("123", HitAction::Mode(InputMode::Numeric), 3, false, true),
                (
                    if state.mode == InputMode::ZhPinyin {
                        "Zh/En"
                    } else {
                        "En/Zh"
                    },
                    HitAction::ToggleZhEn,
                    3,
                    false,
                    true,
                ),
                ("Space", HitAction::Space, 8, false, false),
                ("Sym", HitAction::Mode(InputMode::Symbols), 3, false, true),
                ("OK", HitAction::Done, 3, true, false),
            ];
            emit_key_row(w, pad, gap, row_y(3), row_h, &r3, &mut emit)?;
        }
        InputMode::ZhT9 => {
            emit_key_row(
                w,
                pad,
                gap,
                row_y(0),
                row_h,
                &[
                    ("1 ,.", HitAction::Char(","), 2, false, true),
                    ("2 ABC", HitAction::Char("2"), 2, false, false),
                    ("3 DEF", HitAction::Char("3"), 2, false, false),
                    ("Del", HitAction::Backspace, 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(1),
                row_h,
                &[
                    ("4 GHI", HitAction::Char("4"), 2, false, false),
                    ("5 JKL", HitAction::Char("5"), 2, false, false),
                    ("6 MNO", HitAction::Char("6"), 2, false, false),
                    ("Clr", HitAction::ClearCode, 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(2),
                row_h,
                &[
                    ("7 PQRS", HitAction::Char("7"), 2, false, false),
                    ("8 TUV", HitAction::Char("8"), 2, false, false),
                    ("9 WXYZ", HitAction::Char("9"), 2, false, false),
                    ("Pin", HitAction::Mode(InputMode::ZhPinyin), 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(3),
                row_h,
                &[
                    ("123", HitAction::Mode(InputMode::Numeric), 2, false, true),
                    ("Space", HitAction::Space, 2, false, false),
                    ("<", HitAction::CursorLeft, 2, false, true),
                    ("OK", HitAction::Done, 2, true, false),
                ],
                &mut emit,
            )?;
        }
        InputMode::Numeric => {
            emit_key_row(
                w,
                pad,
                gap,
                row_y(0),
                row_h,
                &[
                    ("1", HitAction::Char("1"), 2, false, false),
                    ("2", HitAction::Char("2"), 2, false, false),
                    ("3", HitAction::Char("3"), 2, false, false),
                    ("Del", HitAction::Backspace, 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(1),
                row_h,
                &[
                    ("4", HitAction::Char("4"), 2, false, false),
                    ("5", HitAction::Char("5"), 2, false, false),
                    ("6", HitAction::Char("6"), 2, false, false),
                    ("-", HitAction::Char("-"), 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(2),
                row_h,
                &[
                    ("7", HitAction::Char("7"), 2, false, false),
                    ("8", HitAction::Char("8"), 2, false, false),
                    ("9", HitAction::Char("9"), 2, false, false),
                    (".", HitAction::Char("."), 2, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(3),
                row_h,
                &[
                    ("Pin", HitAction::Mode(InputMode::ZhPinyin), 2, false, true),
                    ("0", HitAction::Char("0"), 2, false, false),
                    (":", HitAction::Char(":"), 2, false, true),
                    ("OK", HitAction::Done, 2, true, false),
                ],
                &mut emit,
            )?;
        }
        InputMode::Symbols => {
            emit_key_row(
                w,
                pad,
                gap,
                row_y(0),
                row_h,
                &[
                    (".", HitAction::Char("."), 2, false, false),
                    (",", HitAction::Char(","), 2, false, false),
                    ("?", HitAction::Char("?"), 2, false, false),
                    ("!", HitAction::Char("!"), 2, false, false),
                    (":", HitAction::Char(":"), 2, false, false),
                    ("-", HitAction::Char("-"), 2, false, false),
                    ("/", HitAction::Char("/"), 2, false, false),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(1),
                row_h,
                &[
                    ("(", HitAction::Char("("), 2, false, false),
                    (")", HitAction::Char(")"), 2, false, false),
                    ("@", HitAction::Char("@"), 2, false, false),
                    ("#", HitAction::Char("#"), 2, false, false),
                    ("%", HitAction::Char("%"), 2, false, false),
                    ("+", HitAction::Char("+"), 2, false, false),
                    ("*", HitAction::Char("*"), 2, false, false),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(2),
                row_h,
                &[
                    ("=", HitAction::Char("="), 2, false, false),
                    ("_", HitAction::Char("_"), 2, false, false),
                    ("[", HitAction::Char("["), 2, false, false),
                    ("]", HitAction::Char("]"), 2, false, false),
                    ("~", HitAction::Char("~"), 2, false, false),
                    ("Del", HitAction::Backspace, 4, false, true),
                ],
                &mut emit,
            )?;
            emit_key_row(
                w,
                pad,
                gap,
                row_y(3),
                row_h,
                &[
                    ("Pin", HitAction::Mode(InputMode::ZhPinyin), 3, false, true),
                    ("En", HitAction::Mode(InputMode::EnLower), 2, false, true),
                    ("Space", HitAction::Space, 5, false, false),
                    ("123", HitAction::Mode(InputMode::Numeric), 3, false, true),
                    ("OK", HitAction::Done, 3, true, false),
                ],
                &mut emit,
            )?;
        }
    }
    Ok(())
}

/// Paint the multi-mode `SoftKeyboard` into a `WidgetCanvas`.
pub fn paint_soft_keyboard<D: ImeDict>(
    state: &ImeState,
    dict: &D,
    _props: &StoredProps,
    theme: Theme,
    canvas: &mut WidgetCanvas<'_>,
) -> Result<(), BuildError> {
    let sz = canvas.size();
    canvas.rounded_rect(
        Rect::new(0, 0, sz.width, sz.height),
        theme.surface,
        theme.shape.radius_sm,
        Some(theme.border),
        theme.shape.border_width,
    )?;

    emit_keyboard_layout(state, dict, sz, |btn| {
        let is_pressed = state.pressed_action == Some(btn.action);
        let (bg, fg, border) = if is_pressed {
            (theme.focus, theme.colors.background, Some(theme.focus))
        } else if btn.accent {
            (theme.accent, theme.on_accent, None)
        } else if btn.secondary {
            (theme.surface_alt, theme.text, Some(theme.border))
        } else {
            (theme.colors.surface_raised, theme.text, Some(theme.border))
        };
        canvas.rounded_rect(
            btn.rect,
            bg,
            4,
            border,
            if border.is_some() { 1 } else { 0 },
        )?;
        let th = 15i32.min(btn.rect.h as i32);
        let ty = btn.rect.y + ((btn.rect.h as i32 - th).max(0) / 2);
        let tx = btn.rect.x + 1;
        let tw = btn.rect.w.saturating_sub(2);
        if tw > 0 && th > 0 {
            canvas.text(
                Rect::new(tx, ty, tw, th as u32),
                btn.label,
                fg,
                FontId::Small12,
                TextAlign::Center,
            )?;
        }
        Ok(())
    })
}

/// Handle pointer events routed to `SoftKeyboard`, returning an `ImeEvent` on release.
pub fn soft_keyboard_on_pointer<'a, D: ImeDict>(
    state: &'a mut ImeState,
    dict: &'a D,
    ev: PointerEvent,
) -> ImeEvent<'a> {
    let mut hit: Option<HitAction> = None;
    let _ = emit_keyboard_layout(state, dict, ev.size, |btn| {
        if hit.is_none() && btn.rect.contains_point(ev.local) {
            hit = Some(btn.action);
        }
        Ok(())
    });

    match ev.phase {
        PointerPhase::Down | PointerPhase::Move => {
            if state.pressed_action != hit {
                state.pressed_action = hit;
                state.bump();
            }
            ImeEvent::None
        }
        PointerPhase::Up => {
            let prev = state.pressed_action.take();
            let target = hit.or(prev);
            if let Some(act) = target {
                if prev.is_some() {
                    state.bump();
                }
                state.apply_action(act, dict)
            } else {
                state.bump();
                ImeEvent::None
            }
        }
        PointerPhase::Cancel => {
            if state.pressed_action.take().is_some() {
                state.bump();
            }
            ImeEvent::None
        }
    }
}
