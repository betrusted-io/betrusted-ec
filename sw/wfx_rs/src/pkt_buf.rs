use betrusted_hal::mem_locs::*;
use debug::{logln, LL};
const LOG_LEVEL: LL = LL::Info;
use core::{slice::from_raw_parts, slice::from_raw_parts_mut};

/// PktPtr indices map directly onto the underlying storage position, e.g.
/// a &[u8]
#[derive(Copy, Clone)]
pub struct PktPtr {
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
pub const MAX_PKTS: usize = 32;
pub struct PktBuf {
    pub ptr_storage: [Option<PktPtr>; MAX_PKTS],
    /// index of where to look to figure out the next enqueue location
    pub enqueue_index: Option<usize>,
    /// index of where to look to figure out the next dequeue location
    pub dequeue_index: Option<usize>,
    pub was_polled: bool,
    pub was_init: bool,
}
impl PktBuf {
    pub fn init(&mut self) {
        if !self.was_init {
            let rawbuf = unsafe{from_raw_parts_mut(PKT_BUF_BASE as *mut u8, PKT_BUF_LEN)};
            // initialize the memory per safety requirement
            for b in rawbuf.iter_mut() {
                *b = 0;
            }
            self.was_init = true;
        }
    }

    /// returns a slice that can be used to store packet data
    pub fn get_enqueue_slice(&mut self, len: usize) -> Option<&mut [u8]> {
        self.was_polled = false; // this will trigger another interrupt to the host
        let alloc_end = if let Some(eq_idx) = self.enqueue_index {
            self.ptr_storage[eq_idx].expect("pktbuf assert A").end
        } else {
            0
        };
        let alloc_start = if let Some(dq_idx) = self.dequeue_index {
            self.ptr_storage[dq_idx].expect("pktbuf assert B").start
        } else {
            0
        };
        for (idx, ptr) in self.ptr_storage.iter_mut().enumerate() {
            if ptr.is_none() {
                let newstart = if len < PKT_BUF_LEN - alloc_end {
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
                *ptr = Some(newpkt);

                if let Some(eq_idx) = self.enqueue_index {
                    if self.ptr_storage[eq_idx].unwrap().next_index.is_some() {
                        logln!(LL::Debug, "ASSERT: expected next_index to be NULL");
                        return None;
                    }
                    self.ptr_storage[eq_idx].unwrap().next_index = Some(idx)
                }
                self.enqueue_index = Some(idx);

                // append the newly enqueued index to the end of the dequeue linked list
                if let Some(dq_idx) = self.dequeue_index {
                    let mut search_idx = dq_idx;
                    logln!(LL::Debug, "dq search start {}", search_idx);
                    loop {
                        if let Some(next) = self.ptr_storage[search_idx].expect("pktbuf assert C").next_index {
                            logln!(LL::Debug, "dq search to {}", next);
                            search_idx = next;
                        } else {
                            let mut pkt_copy = self.ptr_storage[search_idx].expect("pktbuf assert C");
                            pkt_copy.next_index = Some(idx);
                            self.ptr_storage[search_idx] = Some(pkt_copy);
                            logln!(LL::Debug, "dq[{}] -> {}", search_idx, idx);
                            break;
                        }
                    }
                } else {
                    // handle the case that this is the very, very first time we've enqueued anything:
                    // dequeue is the enqueue because there is only one entry
                    logln!(LL::Debug, "first eq/dq entry: {}", idx);
                    self.dequeue_index = Some(idx);
                }
                //return Some(&mut self.rawbuf.borrow_mut()[newpkt.start..newpkt.end])
                logln!(LL::Debug, "enq idx: {} [{}..{}]", idx, newpkt.start, newpkt.end);
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
    pub fn peek_dequeue_slice(&mut self) -> Option<&'static [u8]> {
        if let Some(dq_idx) = self.dequeue_index {
            if let Some(ptr) = self.ptr_storage[dq_idx] {
                // Some(& self.rawbuf.borrow()[ptr.start..ptr.end])
                logln!(LL::Debug, "deq idx: {} [{}..{}]", dq_idx, ptr.start, ptr.end);
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
        self.was_polled = false;
        if let Some(dq_idx) = self.dequeue_index {
            if let Some(ptr) = self.ptr_storage[dq_idx] {
                if let Some(next_dq) = ptr.next_index {
                    logln!(LL::Debug, "next deq idx: {}", next_dq);
                    self.dequeue_index = Some(next_dq);
                } else {
                    // there is no future dq, which means we /should/ have just dequeued
                    // the current enqueue point. check this is true, and if so,
                    // clear both dq and eq pointers as we are now in the empty state
                    if self.enqueue_index.expect("ASSERT: no eq but dq") != dq_idx {
                        logln!(LL::Debug, "ASSERT: last eq should equal dq");
                        return false;
                    } else {
                        logln!(LL::Debug, "empty queue");
                        self.dequeue_index = None;
                        self.enqueue_index = None;
                    }
                }
                self.ptr_storage[dq_idx] = None;
                true
            } else {
                logln!(LL::Debug, "ASSERT: dequeue points at None entry (dq)");
                false
            }
        } else {
            false
        }
    }

    /// this will return the length of the latest available entry, but only once
    /// upon poll. Repeated polls without a dequeue() will return None.
    /// Polling state resets when dequeue() is called, or if a new packet is enqueued.
    pub fn poll_new_avail(&mut self) -> Option<u16> {
        if !self.was_polled {
            if let Some(dq_idx) = self.dequeue_index {
                if let Some(ptr) = self.ptr_storage[dq_idx] {
                    logln!(LL::Debug, "raise pkt avail of {}", ptr.end - ptr.start);
                    self.was_polled = true;
                    Some((ptr.end - ptr.start) as u16)
                } else {
                    logln!(LL::Debug, "ASSERT: dequeue points at None entry (poll)");
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}
