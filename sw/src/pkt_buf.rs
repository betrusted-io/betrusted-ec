use betrusted_hal::mem_locs::*;
use debug::{log, loghex, loghexln, logln, LL};
const LOG_LEVEL: LL = LL::Debug;
use core::{cell::{Cell, RefCell}, slice::from_raw_parts_mut, slice::from_raw_parts};

/// PktPtr indices map directly onto the underlying storage position, e.g.
/// a &[u8]
#[derive(Copy, Clone)]
struct PktPtr {
    start: usize,
    end: usize,
    next_index: Option<usize>,
}
/// A very simple packet buffer which creates sub-slices out of
/// a single range of memory allocated with exactly one unsafe operation.
/// The implementation is slightly ineffecient in that once we reach the
/// end of the available space, we opt to leave it unused and wrap around
/// to the beginning again, rather than have to implement a custom deref
/// to reclaim it.
const MAX_PTRS: usize = 20;
pub struct PktBuf {
    rawbuf: RefCell<[u8; PKT_BUF_LEN]>,
    ptr_storage: [Cell<Option<PktPtr>>; MAX_PTRS],
    /// index of where to look to figure out the next enqueue location
    enqueue_index: Cell<Option<usize>>,
    /// index of where to look to figure out the next dequeue location
    dequeue_index: Cell<Option<usize>>,
}
impl PktBuf {
    /// Nothing prevents you from calling this multiple times, but it's definitely a bad idea to do that.
    /// Thus the function is marked as unsafe, because it wraps the fundamentally unsound opertaion of
    /// creating a pointer out of thin air.
    pub unsafe fn new() -> PktBuf {
        let rawbuf: *mut [u8; PKT_BUF_LEN] = PKT_BUF_BASE as *mut u8 as *mut [u8; PKT_BUF_LEN];
        // initialize the memory per safety requirement
        for b in (*rawbuf).iter_mut() {
            *b = 0;
        }
        PktBuf {
            rawbuf: RefCell::new(*rawbuf),
            ptr_storage: [
                Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None),
                Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None),
                Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None),
                Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None), Cell::new(None),
            ],
            enqueue_index: Cell::new(None),
            dequeue_index: Cell::new(None),
        }
    }

    /// returns a slice that can be used to store packet data
    pub fn get_enqueue_slice(&self, len: usize) -> Option<&mut [u8]> {
        let alloc_end = if let Some(eq_idx) = self.enqueue_index.get() {
            self.ptr_storage[eq_idx].get().expect("pktbuf assert A").end
        } else {
            0
        };
        let alloc_start = if let Some(dq_idx) = self.dequeue_index.get() {
            self.ptr_storage[dq_idx].get().expect("pktbuf assert B").start
        } else {
            0
        };
        for (idx, ptr) in self.ptr_storage.iter().enumerate() {
            if ptr.get().is_none() {
                let newstart = if len < self.rawbuf.borrow().len() - alloc_end {
                    alloc_end
                } else if len < alloc_start {
                    0
                } else {
                    return None;
                };
                let newpkt = PktPtr {
                    start: newstart,
                    end: newstart + len,
                    next_index: None,
                };
                ptr.replace(Some(newpkt));

                if let Some(eq_idx) = self.enqueue_index.get() {
                    if self.ptr_storage[eq_idx].get().unwrap().next_index.is_some() {
                        logln!(LL::Debug, "ASSERT: expected next_index to be NULL");
                        return None;
                    }
                    self.ptr_storage[eq_idx].get().unwrap().next_index = Some(idx)
                } else {
                    self.enqueue_index.replace(Some(idx));
                }
                if self.dequeue_index.get().is_none() {
                    self.dequeue_index.replace(Some(idx));
                }
                //return Some(&mut self.rawbuf.borrow_mut()[newpkt.start..newpkt.end])
                return Some(
                    unsafe{
                        from_raw_parts_mut(
                        (PKT_BUF_BASE + newpkt.start) as *mut u8,
                        newpkt.end - newpkt.start)
                    }
                )
            }
        }
        None
    }

    /// returns an immutable slice that we can freely read anytime; but, the slice is *not* dequeued
    /// from the system. A subsequent call to dequeue() is necessary to release the memory and advance
    /// the dequeue pointer. This arrangement allows an interrupt routine to pop in part way
    /// through a copy out of the dequeue packet, without worry of it being overwritten, and
    /// without having to allocate a second copy of the memory to prevent such overwriting.
    pub fn peek_dequeue_slice(&self) -> Option<&[u8]> {
        if let Some(dq_idx) = self.dequeue_index.get() {
            if let Some(ptr) = self.ptr_storage[dq_idx].get() {
                //Some(& self.rawbuf.borrow()[ptr.start..ptr.end])
                Some(
                    unsafe{
                        from_raw_parts(
                        (PKT_BUF_BASE + ptr.start) as *const u8,
                        ptr.end - ptr.start)
                    }
                )
            } else {
                logln!(LL::Debug, "ASSERT: dequeue points at None entry (peek)");
                None
            }
        } else {
            // this is "normal" in that, it's fair game to call this function
            // to see if there is anything in the buffer at all.
            None
        }
    }
    /// this actually gets rid of the dequeue slice, immediately, for good. No
    /// pointer is returned because well, you shouldn't be using it after this is called.
    pub fn dequeue(&mut self) -> bool {
        if let Some(dq_idx) = self.dequeue_index.get() {
            if let Some(ptr) = self.ptr_storage[dq_idx].get() {
                if let Some(next_dq) = ptr.next_index {
                    self.dequeue_index.replace(Some(next_dq));
                } else {
                    // there is no future dq, which means we /should/ have just dequeued
                    // the current enqueue point. check this is true, and if so,
                    // clear both dq and eq pointers as we are now in the empty state
                    if self.enqueue_index.get().expect("ASSERT: no eq but dq") != dq_idx {
                        logln!(LL::Debug, "ASSERT: last eq should equal dq");
                        return false;
                    } else {
                        self.dequeue_index.replace(None);
                        self.enqueue_index.replace(None);
                    }
                }
                true
            } else {
                logln!(LL::Debug, "ASSERT: dequeue points at None entry (dq)");
                false
            }
        } else {
            false
        }
    }
}
