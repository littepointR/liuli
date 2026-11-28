use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::NonNull,
};
use linked_list_allocator::Heap;

const EXTERNAL: usize = if cfg!(feature = "diagnostics") {
    0xc020_0000
} else {
    0xc018_0000
};
struct UiHeap(UnsafeCell<[Heap; 2]>);
// The two backends share this single-core allocator. Each heap operation masks interrupts.
unsafe impl Sync for UiHeap {}
unsafe impl GlobalAlloc for UiHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        cortex_m::interrupt::free(|_| {
            unsafe { &mut *self.0.get() }
                .iter_mut()
                .find_map(|heap| heap.allocate_first_fit(layout).ok())
                .map_or(core::ptr::null_mut(), NonNull::as_ptr)
        })
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        cortex_m::interrupt::free(|_| unsafe {
            let index = usize::from(p.addr() >= EXTERNAL);
            (*self.0.get())[index].deallocate(NonNull::new_unchecked(p), layout)
        });
    }
}
#[global_allocator]
static HEAP: UiHeap = UiHeap(UnsafeCell::new([Heap::empty(), Heap::empty()]));
pub unsafe fn init() {
    unsafe extern "C" {
        static mut _ui_heap_start: u8;
        static _ui_heap_end: u8;
    }
    // The linker leaves the AXI SRAM after statics free on both runtimes
    // (the stack lives in DTCM). Keep hot UI pixels off the scanout SDRAM bus.
    unsafe {
        let start = &raw mut _ui_heap_start;
        let internal = (&raw const _ui_heap_end).addr() - start.addr();
        let [fast, external] = &mut *HEAP.0.get();
        if internal >= 3 * size_of::<usize>() {
            fast.init(start, internal);
        }
        external.init(EXTERNAL as *mut u8, 0xc200_0000 - EXTERNAL);
    }
}
