use liuli::prelude::InstantMs;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SettingsModel {
    pub brightness: i16,
    pub sound_enabled: bool,
    pub dark_theme: bool,
    pub live_monitoring: bool,
    pub temp_deci_c: i16,
    pub sample_origin_ms: InstantMs,
    pub next_sample_ms: Option<InstantMs>,
    pub reset_modal_open: bool,
}

impl SettingsModel {
    pub const fn new(now: InstantMs) -> Self {
        Self {
            brightness: 60,
            sound_enabled: true,
            dark_theme: false,
            live_monitoring: true,
            temp_deci_c: 230,
            sample_origin_ms: now,
            next_sample_ms: Some(now.saturating_add(1000)),
            reset_modal_open: false,
        }
    }

    pub fn restore_defaults(&mut self, now: InstantMs) {
        self.brightness = 60;
        self.sound_enabled = true;
        self.dark_theme = false;
        self.live_monitoring = true;
        self.temp_deci_c = 230;
        self.sample_origin_ms = now;
        self.next_sample_ms = Some(now.saturating_add(1000));
        self.reset_modal_open = false;
    }

    pub fn sample_temperature(&mut self, now: InstantMs) -> bool {
        if !self.live_monitoring {
            return false;
        }
        let Some(due) = self.next_sample_ms else {
            return false;
        };
        if now < due {
            return false;
        }
        let elapsed_ms = now.saturating_sub(self.sample_origin_ms);
        let elapsed_secs = elapsed_ms / 1000;
        self.temp_deci_c = 230 + ((elapsed_secs % 17) as i16);
        self.next_sample_ms = Some(
            self.sample_origin_ms
                .saturating_add((elapsed_secs + 1) * 1000),
        );
        true
    }
}

impl Default for SettingsModel {
    fn default() -> Self {
        Self::new(InstantMs(0))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SettingsMsg {
    SetBrightness(i16),
    ToggleSound,
    ToggleTheme,
    ToggleMonitoring,
    OpenResetModal,
    CancelReset,
    ConfirmReset,
}
