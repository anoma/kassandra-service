use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicBool, AtomicUsize};
use std::task::{Context, Poll};

use borsh::BorshDeserialize;
use futures::task::AtomicWaker;
use namada::borsh::BorshSerialize;
use namada::chain::BlockHeight;
use tokio::sync::Mutex;

pub struct TaskError<C> {
    pub error: eyre::Error,
    pub context: C,
}

struct AsyncCounterInner {
    waker: AtomicWaker,
    count: AtomicUsize,
}

impl AsyncCounterInner {
    fn increment(&self) {
        self.count.fetch_add(1, atomic::Ordering::Relaxed);
    }

    fn decrement_then_wake(&self) -> bool {
        // NB: if the prev value is 1, the new value
        // is eq to 0, which means we must wake the
        // waiting future
        self.count.fetch_sub(1, atomic::Ordering::Relaxed) == 1
    }

    fn value(&self) -> usize {
        self.count.load(atomic::Ordering::Relaxed)
    }
}

pub struct AsyncCounter {
    inner: Arc<AsyncCounterInner>,
}

impl AsyncCounter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AsyncCounterInner {
                waker: AtomicWaker::new(),
                count: AtomicUsize::new(0),
            }),
        }
    }
}

impl Clone for AsyncCounter {
    fn clone(&self) -> Self {
        let inner = Arc::clone(&self.inner);
        inner.increment();
        Self { inner }
    }
}

impl Drop for AsyncCounter {
    fn drop(&mut self) {
        if self.inner.decrement_then_wake() {
            self.inner.waker.wake();
        }
    }
}

impl Future for AsyncCounter {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.inner.value() == 0 {
            Poll::Ready(())
        } else {
            self.inner.waker.register(cx.waker());
            Poll::Pending
        }
    }
}

#[derive(Clone, Default)]
pub struct AtomicFlag {
    inner: Arc<AtomicBool>,
}

impl AtomicFlag {
    pub fn set(&self) {
        self.inner.store(true, atomic::Ordering::Relaxed)
    }

    pub fn get(&self) -> bool {
        self.inner.load(atomic::Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct InterruptFlag {
    send: Arc<tokio::sync::watch::Sender<bool>>,
    recv: Arc<Mutex<tokio::sync::watch::Receiver<bool>>>,
}

impl InterruptFlag {
    pub fn new() -> Self {
        let (send, recv) = tokio::sync::watch::channel(false);
        Self {
            send: Arc::new(send),
            recv: Arc::new(Mutex::new(recv)),
        }
    }

    pub async fn dropped(&mut self) -> bool {
        _ = self.recv.lock().await.changed().await;
        true
    }
}

impl Drop for InterruptFlag {
    fn drop(&mut self) {
        _ = self.send.send(true);
    }
}

#[derive(Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct FetchedRanges(Vec<BlockHeight>);

impl FetchedRanges {
    /// Get the first block height not contained in `self`
    pub fn first(&self) -> BlockHeight {
        if self.0.is_empty() {
            BlockHeight::first()
        } else {
            self.0[1].checked_add(1).unwrap()
        }
    }

    /// Check if one of the ranges contains `height`
    pub fn contains(&self, height: &BlockHeight) -> bool {
        self.0.chunks(2).any(|r| r[0] <= *height && *height <= r[1])
    }

    /// Insert a new interval
    pub fn insert(&mut self, from: BlockHeight, to: BlockHeight) {
        if self.0.is_empty() {
            self.0.push(from);
            self.0.push(to);
            return;
        }
        let contains_to = self.contains(&to);
        self.0.retain(|x| *x < from || *x > to);
        let from_ix = self.find_index(&from);

        if !contains_to {
            self.0.insert(from_ix, to);
        }
        if from_ix & 1 == 0 {
            self.0.insert(from_ix, from);
        }
        self.simplify();
    }

    fn simplify(&mut self) {
        if self.0.is_empty() {
            return;
        }
        let mut simplified = Vec::with_capacity(self.0.len());

        for (ix, b) in self.0.iter().enumerate().step_by(2) {
            if ix == 0 {
                simplified.push(*b);
                continue;
            }
            if self.0[ix - 1].0 == b.0 - 1 || self.0[ix - 1].0 == b.0 {
                continue;
            } else {
                simplified.push(self.0[ix - 1]);
                simplified.push(self.0[ix]);
            }
        }
        simplified.push(*self.0.last().unwrap());
        std::mem::swap(&mut self.0, &mut simplified);
    }

    fn find_index(&self, h: &BlockHeight) -> usize {
        self.0
            .iter()
            .enumerate()
            .find(|(_, x)| x > &h)
            .map(|(ix, _)| ix)
            .unwrap_or_else(|| self.0.len())
    }

    /// Given an interval [from, to], finds the sub-intervals not contained in `self`
    pub fn blocks_left_to_fetch(&self, from: u64, to: u64) -> Vec<[BlockHeight; 2]> {
        let from = BlockHeight::from(from);
        let to = BlockHeight::from(to);
        const ZERO: BlockHeight = BlockHeight(0);

        if from > to {
            panic!("Empty range passed to `blocks_left_to_fetch`, [{from}, {to}]");
        }
        if from == ZERO || to == ZERO {
            panic!("Block height values start at 1");
        }
        let mut to_fetch = Vec::with_capacity((to.0 - from.0 + 1) as usize);
        let mut current_from = from;
        let mut need_to_fetch = true;

        for height in (from.0..=to.0).map(BlockHeight) {
            let height_in_cache = self.contains(&height);

            // cross an upper gap boundary
            if need_to_fetch && height_in_cache {
                if height > current_from {
                    to_fetch.push([
                        current_from,
                        height.checked_sub(1).expect("Height is greater than zero"),
                    ]);
                }
                need_to_fetch = false;
            } else if !need_to_fetch && !height_in_cache {
                // cross a lower gap boundary
                current_from = height;
                need_to_fetch = true;
            }
        }
        if need_to_fetch {
            to_fetch.push([current_from, to]);
        }
        to_fetch
    }
}

#[cfg(test)]
mod test_utils {
    use super::*;

    #[test]
    fn test_ranges() {
        //Basic cases
        let mut ranges = FetchedRanges::default();
        ranges.insert(5.into(), 5.into());
        assert_eq!(ranges.0, vec![BlockHeight(5), BlockHeight(5)]);
        ranges.insert(2.into(), 4.into());
        assert_eq!(ranges.0, vec![BlockHeight(2), BlockHeight(5)]);
        ranges.insert(7.into(), 8.into());
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(2),
                BlockHeight(5),
                BlockHeight(7),
                BlockHeight(8)
            ]
        );

        // Overlaps
        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(7),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(9), BlockHeight(14));
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(5),
                BlockHeight(7),
                BlockHeight(9),
                BlockHeight(14),
                BlockHeight(16),
                BlockHeight(18)
            ]
        );
        ranges.insert(BlockHeight(7), BlockHeight(16));
        assert_eq!(ranges.0, vec![BlockHeight(5), BlockHeight(18)]);

        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(7),
            BlockHeight(9),
            BlockHeight(14),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(8), BlockHeight(15));
        assert_eq!(ranges.0, vec![BlockHeight(5), BlockHeight(18)]);

        // intersections
        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(7),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(6), BlockHeight(14));
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(5),
                BlockHeight(14),
                BlockHeight(16),
                BlockHeight(18)
            ]
        );

        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(6),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(8), BlockHeight(17));
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(5),
                BlockHeight(6),
                BlockHeight(8),
                BlockHeight(18)
            ]
        );

        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(7),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(6), BlockHeight(17));
        assert_eq!(ranges.0, vec![BlockHeight(5), BlockHeight(18)]);

        // endpoints
        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(7),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(7), BlockHeight(14));
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(5),
                BlockHeight(14),
                BlockHeight(16),
                BlockHeight(18)
            ]
        );

        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(6),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(8), BlockHeight(16));
        assert_eq!(
            ranges.0,
            vec![
                BlockHeight(5),
                BlockHeight(6),
                BlockHeight(8),
                BlockHeight(18)
            ]
        );

        let mut ranges = FetchedRanges(vec![
            BlockHeight(5),
            BlockHeight(6),
            BlockHeight(10),
            BlockHeight(12),
            BlockHeight(16),
            BlockHeight(18),
        ]);
        ranges.insert(BlockHeight(6), BlockHeight(16));
        assert_eq!(ranges.0, vec![BlockHeight(5), BlockHeight(18)]);
    }
}
