//! EXTI latches an edge across a UI transaction. Only the foreground reads I²C.
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "embassy")]
use {
    core::{cell::RefCell, task::Waker},
    cortex_m::interrupt::Mutex,
};

unsafe extern "C" {
    fn fk_touch_irq_init();
    fn fk_touch_irq_ack();
}
static READY: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "embassy")]
static WAITER: Mutex<RefCell<Option<Waker>>> = Mutex::new(RefCell::new(None));

pub fn enable() {
    unsafe { fk_touch_irq_init() };
}
pub fn take() -> bool {
    READY.swap(false, Ordering::AcqRel)
}
pub fn pending() -> bool {
    READY.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
extern "C" fn EXTI3() {
    unsafe { fk_touch_irq_ack() };
    READY.store(true, Ordering::Release);
    #[cfg(feature = "embassy")]
    if let Some(w) = cortex_m::interrupt::free(|cs| WAITER.borrow(cs).take()) {
        w.wake();
    }
}

#[cfg(feature = "embassy")]
pub async fn wait() {
    core::future::poll_fn(|cx| {
        cortex_m::interrupt::free(|cs| {
            if pending() {
                core::task::Poll::Ready(())
            } else {
                WAITER.borrow(cs).replace(Some(cx.waker().clone()));
                core::task::Poll::Pending
            }
        })
    })
    .await
}
