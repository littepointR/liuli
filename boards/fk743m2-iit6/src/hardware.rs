//! Board services backed by STM32CubeH7 HAL, LL and CMSIS.
pub const CPU_MHZ: u32 = 400;
unsafe extern "C" {
    #[cfg(feature = "st-hal")]
    fn fk_st_clocks() -> u32;
    fn fk_core_init();
    fn fk_sdram_init() -> u32;
    fn fk_panel_init(front: u32) -> u32;
    fn fk_panel_swap(front: u32) -> u32;
    fn fk_backlight();
    #[cfg(feature = "diagnostics")]
    fn fk_display_status() -> u32;
    fn fk_dcache_clean(addr: u32, len: u32);
    fn fk_dcache_clean_invalidate(addr: u32, len: u32);
    fn fk_dma2d_wait();
    fn fk_dma2d_fill(dst: u32, w: u32, h: u32, off: u32, color: u32);
    fn fk_dma2d_copy_async(src: u32, dst: u32, w: u32, h: u32, off: u32);
    fn fk_cycles() -> u32;
    fn fk_pin_mode(pin: u32, output: u32, open_drain: u32);
    fn fk_pin_write(pin: u32, high: u32);
    fn fk_pin_read(pin: u32) -> u32;
}
fn checked(code: u32) -> Result<(), u32> {
    if code == 0 { Ok(()) } else { Err(code) }
}
pub fn clocks() -> Result<(), u32> {
    #[cfg(feature = "st-hal")]
    checked(unsafe { fk_st_clocks() })?;
    unsafe { fk_core_init() };
    Ok(())
}
pub fn sdram() -> Result<(), u32> {
    checked(unsafe { fk_sdram_init() })
}
pub fn panel(addr: usize) -> Result<(), u32> {
    checked(unsafe { fk_panel_init(addr as u32) })
}
pub fn swap(addr: usize) -> Result<(), u32> {
    checked(unsafe { fk_panel_swap(addr as u32) })
}
pub fn backlight() {
    unsafe { fk_backlight() }
}
#[cfg(feature = "diagnostics")]
pub fn status() -> u32 {
    unsafe { fk_display_status() }
}
pub fn clean_cache(addr: usize, len: usize) {
    unsafe { fk_dcache_clean(addr as u32, len as u32) }
}
pub fn clean_invalidate_cache(addr: usize, len: usize) {
    unsafe { fk_dcache_clean_invalidate(addr as u32, len as u32) }
}
pub fn dma2d_wait() {
    unsafe { fk_dma2d_wait() }
}
pub fn dma2d_fill(dst: usize, w: u32, h: u32, off: u32, color: u16) {
    unsafe { fk_dma2d_fill(dst as u32, w, h, off, u32::from(color)) }
}
pub fn dma2d_copy_async(src: usize, dst: usize, w: u32, h: u32, off: u32) {
    unsafe { fk_dma2d_copy_async(src as u32, dst as u32, w, h, off) }
}
pub fn cycles() -> u32 {
    unsafe { fk_cycles() }
}
pub fn delay(us: u32) {
    let start = cycles();
    while cycles().wrapping_sub(start) < us * CPU_MHZ {
        core::hint::spin_loop()
    }
}
#[repr(u32)]
#[derive(Clone, Copy)]
pub enum Pin {
    Scl,
    Sda,
    Interrupt,
    Reset,
}
impl Pin {
    pub fn mode(self, output: bool, open_drain: bool) {
        unsafe { fk_pin_mode(self as u32, output.into(), open_drain.into()) };
    }
    pub fn set(self, high: bool) {
        unsafe { fk_pin_write(self as u32, high.into()) };
    }
    pub fn get(self) -> bool {
        unsafe { fk_pin_read(self as u32) != 0 }
    }
}
