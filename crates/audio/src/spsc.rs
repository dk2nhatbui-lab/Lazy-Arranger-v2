//! A fixed-size, lock-free single-producer/single-consumer queue of `f32`.
//!
//! Audio callbacks must never block or allocate. One callback pushes, another
//! thread pops; neither takes a lock and the storage is allocated up front.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// A counter on its own cache line, so producer and consumer do not fight
/// over one line.
#[derive(Debug, Default)]
#[repr(align(64))]
struct Padded(AtomicUsize);

/// A bounded SPSC sample queue. Share it in an `Arc`; exactly one thread may
/// push and exactly one thread may pop.
#[derive(Debug)]
pub struct SampleQueue {
    /// Power-of-two storage, so the running counters wrap cleanly.
    slots: Box<[AtomicU32]>,
    mask: usize,
    /// The most samples held at once, as asked for in `new`.
    capacity: usize,
    /// Total samples pushed (written only by the producer).
    head: Padded,
    /// Total samples popped (written only by the consumer).
    tail: Padded,
}

impl SampleQueue {
    /// A queue that holds up to `capacity` samples (at least one).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let size = capacity.next_power_of_two();
        Self {
            slots: (0..size).map(|_| AtomicU32::new(0)).collect(),
            mask: size - 1,
            capacity,
            head: Padded::default(),
            tail: Padded::default(),
        }
    }

    /// Appends a sample; returns `false` and drops it when the queue is full.
    pub fn push(&self, sample: f32) -> bool {
        let head = self.head.0.load(Ordering::Relaxed);
        let tail = self.tail.0.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity {
            return false;
        }
        self.slots[head & self.mask].store(sample.to_bits(), Ordering::Relaxed);
        self.head.0.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    /// Removes the oldest sample, if any.
    pub fn pop(&self) -> Option<f32> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        let head = self.head.0.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let bits = self.slots[tail & self.mask].load(Ordering::Relaxed);
        self.tail.0.store(tail.wrapping_add(1), Ordering::Release);
        Some(f32::from_bits(bits))
    }

    /// Samples waiting to be popped.
    #[must_use]
    pub fn len(&self) -> usize {
        // Tail first: it can only fall further behind the head read after it,
        // so a third thread watching never sees a negative length.
        let tail = self.tail.0.load(Ordering::Acquire);
        let head = self.head.0.load(Ordering::Acquire);
        head.wrapping_sub(tail).min(self.capacity)
    }

    /// Room left for pushes.
    #[must_use]
    pub fn free(&self) -> usize {
        self.capacity.saturating_sub(self.len())
    }

    /// True when nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `value` clamped to `-1.0..=1.0`, or silence when it is NaN or infinite.
#[must_use]
pub fn sanitize(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_come_out_in_order_and_a_full_queue_drops() {
        let queue = SampleQueue::new(2);
        assert!(queue.push(1.0));
        assert!(queue.push(2.0));
        assert!(!queue.push(3.0));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop(), Some(1.0));
        assert!(queue.push(4.0));
        assert_eq!(queue.pop(), Some(2.0));
        assert_eq!(queue.pop(), Some(4.0));
        assert_eq!(queue.pop(), None);
        assert!(queue.is_empty());
    }

    #[test]
    fn sanitize_silences_non_finite_values() {
        assert_eq!(sanitize(f32::NAN), 0.0);
        assert_eq!(sanitize(f32::INFINITY), 0.0);
        assert_eq!(sanitize(2.0), 1.0);
        assert_eq!(sanitize(-0.5), -0.5);
    }

    #[test]
    fn counters_wrapping_past_usize_max_keep_order() {
        let queue = SampleQueue::new(3);
        let start = usize::MAX - 1;
        queue.head.0.store(start, Ordering::Relaxed);
        queue.tail.0.store(start, Ordering::Relaxed);
        for value in 0..3 {
            assert!(queue.push(value as f32));
        }
        assert!(!queue.push(9.0));
        assert_eq!(queue.len(), 3);
        for value in 0..3 {
            assert_eq!(queue.pop(), Some(value as f32));
        }
        assert!(queue.is_empty());
    }
}
