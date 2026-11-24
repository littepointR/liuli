use crate::capacity::MAX_DAMAGE;
use crate::geometry::Rect;

#[derive(Clone, Debug)]
pub struct DamageSet {
    rects: [Rect; MAX_DAMAGE],
    count: usize,
    full_screen: bool,
}

impl DamageSet {
    pub const fn new() -> Self {
        Self {
            rects: [Rect::ZERO; MAX_DAMAGE],
            count: 0,
            full_screen: false,
        }
    }

    pub fn clear(&mut self) {
        self.count = 0;
        self.full_screen = false;
    }

    pub fn is_empty(&self) -> bool {
        !self.full_screen && self.count == 0
    }

    pub fn is_full_screen(&self) -> bool {
        self.full_screen
    }

    pub fn mark_full_screen(&mut self, screen: Rect) {
        if screen.is_empty() {
            return;
        }
        self.full_screen = true;
        self.rects[0] = screen;
        self.count = 1;
    }

    pub fn add(&mut self, rect: Rect, screen: Rect) {
        let clipped = rect.intersect(screen);
        if clipped.is_empty() || self.full_screen {
            return;
        }

        if self.count >= MAX_DAMAGE {
            self.mark_full_screen(screen);
            return;
        }

        self.rects[self.count] = clipped;
        self.count += 1;
        self.normalize(screen);
    }

    /// Expand to the display controller's address grid, measured from the screen origin.
    pub fn align_windows(
        &mut self,
        screen: Rect,
        x_step: core::num::NonZeroU16,
        y_step: core::num::NonZeroU16,
    ) {
        if self.is_empty() || self.full_screen {
            return;
        }
        let expand = |start: i32, len: u32, base: i32, limit: u32, step: core::num::NonZeroU16| {
            let step = u64::from(step.get());
            let start = (i64::from(start) - i64::from(base)).max(0) as u64;
            let left = start / step * step;
            let right = (start + u64::from(len)).div_ceil(step) * step;
            (
                (i64::from(base) + left as i64).clamp(i32::MIN.into(), i32::MAX.into()) as i32,
                right.min(limit.into()).saturating_sub(left) as u32,
            )
        };
        for rect in &mut self.rects[..self.count] {
            let (x, w) = expand(rect.x, rect.w, screen.x, screen.w, x_step);
            let (y, h) = expand(rect.y, rect.h, screen.y, screen.h, y_step);
            *rect = Rect::new(x, y, w, h);
        }
        self.normalize(screen);
    }
    pub fn as_slice(&self) -> &[Rect] {
        &self.rects[..self.count]
    }

    fn normalize(&mut self, screen: Rect) {
        if self.count <= 1 {
            return;
        }

        loop {
            let mut merged_any = false;
            'outer: for i in 0..self.count {
                for j in (i + 1)..self.count {
                    let a = self.rects[i];
                    let b = self.rects[j];
                    let should_merge = if a.intersects(b) {
                        true
                    } else if a.shares_edge_with(b) {
                        let u = a.union(b);
                        let sum = a.area64().saturating_add(b.area64());
                        u.area64().saturating_mul(4) <= sum.saturating_mul(5)
                    } else {
                        false
                    };

                    if should_merge {
                        self.rects[i] = a.union(b);
                        self.count -= 1;
                        self.rects[j] = self.rects[self.count];
                        self.rects[self.count] = Rect::ZERO;
                        merged_any = true;
                        break 'outer;
                    }
                }
            }
            if !merged_any {
                break;
            }
        }

        // Insertion sort by (top, left)
        for i in 1..self.count {
            let key = self.rects[i];
            let mut j = i;
            while j > 0 && (self.rects[j - 1].y, self.rects[j - 1].x) > (key.y, key.x) {
                self.rects[j] = self.rects[j - 1];
                j -= 1;
            }
            self.rects[j] = key;
        }

        self.check_area_threshold(screen);
    }

    fn check_area_threshold(&mut self, screen: Rect) {
        if screen.is_empty() {
            self.count = 0;
            self.full_screen = false;
            return;
        }
        if self.count <= 1 {
            return;
        }
        let mut total_area = 0u64;
        for r in &self.rects[..self.count] {
            total_area = total_area.saturating_add(r.area64());
        }
        let screen_area = screen.area64();
        if total_area.saturating_mul(100) >= screen_area.saturating_mul(60) {
            self.mark_full_screen(screen);
        }
    }
}

impl Default for DamageSet {
    fn default() -> Self {
        Self::new()
    }
}
