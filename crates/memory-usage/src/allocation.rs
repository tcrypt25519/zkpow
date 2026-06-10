use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicI64, Ordering};

pub struct AllocationTracker {
    live_bytes: AtomicI64,
    peak_bytes: AtomicI64,
}

impl AllocationTracker {
    pub const fn new() -> Self {
        Self {
            live_bytes: AtomicI64::new(0),
            peak_bytes: AtomicI64::new(0),
        }
    }

    pub fn live_bytes(&self) -> u64 {
        self.live_bytes.load(Ordering::Relaxed).max(0) as u64
    }

    pub fn live_kb(&self) -> u64 {
        self.live_bytes() / 1024
    }

    pub fn peak_bytes(&self) -> u64 {
        self.peak_bytes.load(Ordering::Relaxed).max(0) as u64
    }

    pub fn peak_kb(&self) -> u64 {
        self.peak_bytes() / 1024
    }

    fn record_alloc(&self, size: usize) {
        let live = self.live_bytes.fetch_add(size as i64, Ordering::Relaxed) + size as i64;
        self.peak_bytes.fetch_max(live, Ordering::Relaxed);
    }

    fn record_dealloc(&self, size: usize) {
        self.live_bytes.fetch_sub(size as i64, Ordering::Relaxed);
    }

    fn record_realloc(&self, old_size: usize, new_size: usize) {
        let delta = new_size as i64 - old_size as i64;
        let live = self.live_bytes.fetch_add(delta, Ordering::Relaxed) + delta;
        self.peak_bytes.fetch_max(live, Ordering::Relaxed);
    }
}

impl Default for AllocationTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TrackingAllocator<'a, A> {
    inner: A,
    tracker: &'a AllocationTracker,
}

impl<'a, A> TrackingAllocator<'a, A> {
    pub const fn new(inner: A, tracker: &'a AllocationTracker) -> Self {
        Self { inner, tracker }
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<'_, A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            self.tracker.record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc_zeroed(layout) };
        if !ptr.is_null() {
            self.tracker.record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.inner.dealloc(ptr, layout) };
        self.tracker.record_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            self.tracker.record_realloc(layout.size(), new_size);
        }
        new_ptr
    }
}

#[cfg(test)]
mod tests {
    use super::{AllocationTracker, TrackingAllocator};
    use std::alloc::{GlobalAlloc, Layout, System};

    #[test]
    fn tracking_allocator_counts_live_and_peak_bytes() {
        let tracker = AllocationTracker::new();
        let allocator = TrackingAllocator::new(System, &tracker);
        let layout = Layout::from_size_align(16, 8).unwrap();

        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        assert_eq!(tracker.live_bytes(), 16);
        assert_eq!(tracker.peak_bytes(), 16);

        let ptr = unsafe { allocator.realloc(ptr, layout, 40) };
        assert!(!ptr.is_null());
        assert_eq!(tracker.live_bytes(), 40);
        assert_eq!(tracker.peak_bytes(), 40);

        unsafe { allocator.dealloc(ptr, Layout::from_size_align(40, 8).unwrap()) };
        assert_eq!(tracker.live_bytes(), 0);
        assert_eq!(tracker.peak_bytes(), 40);
    }
}
