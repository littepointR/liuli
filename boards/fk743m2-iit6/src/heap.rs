use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::NonNull,
};
use linked_list_allocator::Heap;

pub struct SdramHeap(UnsafeCell<Heap>);
// The two backends share this single-core allocator. Each heap operation masks interrupts.
unsafe impl Sync for SdramHeap {}
unsafe impl GlobalAlloc for SdramHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        cortex_m::interrupt::free(|_| unsafe { (&mut *self.0.get()).allocate_first_fit(layout) })
            .map_or(core::ptr::null_mut(), NonNull::as_ptr)
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        cortex_m::interrupt::free(|_| unsafe {
            (*self.0.get()).deallocate(NonNull::new_unchecked(p), layout)
        });
    }
}
#[global_allocator]
static HEAP: SdramHeap = SdramHeap(UnsafeCell::new(Heap::empty()));
pub unsafe fn init() {
    // The first 2 MiB of the 32 MiB SDRAM are reserved for both scanout buffers.
    unsafe {
        (*HEAP.0.get()).init(0xc0200000 as *mut u8, 30 * 1024 * 1024);
    }
}
