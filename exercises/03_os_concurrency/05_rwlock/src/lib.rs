//! # Read-Write Lock (Writer-Priority)
//!
//! In this exercise, you will implement a **writer-priority** read-write lock from scratch using atomics.
//! Multiple readers may hold the lock concurrently; a writer holds it exclusively.
//!
//! **Note:** Rust's standard library already provides [`std::sync::RwLock`]. This exercise implements
//! a minimal version for learning the protocol and policy without using the standard one.
//!
//! ## Common policies for read-write locks
//! Different implementations can give different **priority** when both readers and writers are waiting:
//!
//! - **Reader-priority (读者优先)**: New readers are allowed to enter while a writer is waiting, so writers
//!   may be starved if readers keep arriving.
//! - **Writer-priority (写者优先)**: Once a writer is waiting, no new readers are admitted until that writer
//!   has run; this exercise implements this policy.
//! - **Read-write fair (读写公平)**: Requests are served in a fair order (e.g. FIFO or round-robin), so
//!   neither readers nor writers are systematically starved.
//!
//! ## Key Concepts
//! - **Readers**: share access; many threads can hold a read lock at once.
//! - **Writer**: exclusive access; only one writer, and no readers while the writer holds the lock.
//! - **Writer-priority (this implementation)**: when at least one writer is waiting, new readers block
//!   until the writer runs.
//!
//! ## State
//! `state` stores the reader count and writer flags; `waiting_writers` counts pending writers.
//! All logic is implemented with compare_exchange and load/store; no use of `std::sync::RwLock`.

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of concurrent readers (fits in state bits).
const READER_MASK: u32 = (1 << 30) - 1;
/// Bit set when a writer holds the lock.
const WRITER_HOLDING: u32 = 1 << 30;
/// Bit set when at least one writer is waiting (writer-priority: block new readers).
const WRITER_WAITING: u32 = 1 << 31;

/// Writer-priority read-write lock. Implemented from scratch; does not use `std::sync::RwLock`.
pub struct RwLock<T> {
    state: AtomicU32,
    waiting_writers: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            waiting_writers: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire a read lock. Blocks (spins) until no writer holds and no writer is waiting (writer-priority).
    ///
    /// TODO: Implement read lock acquisition
    /// 1. In a loop, load state (Acquire).
    /// 2. If WRITER_HOLDING or WRITER_WAITING is set, spin_loop and continue (writer-priority: no new readers while writer waits).
    /// 3. If reader count (state & READER_MASK) is already READER_MASK, spin and continue.
    /// 4. Try compare_exchange(s, s + 1, AcqRel, Acquire); on success return RwLockReadGuard { lock: self }.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        // TODO
        loop {
            let state = self.state.load(Ordering::Acquire);
            if state & (WRITER_HOLDING | WRITER_WAITING) != 0
                || state & READER_MASK == READER_MASK
                || self.waiting_writers.load(Ordering::SeqCst) != 0
            {
                core::hint::spin_loop();
                continue;
            }
            if self
                .state
                .compare_exchange(state, state + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                if self.waiting_writers.load(Ordering::SeqCst) == 0 {
                    return RwLockReadGuard { lock: self };
                }
                self.state.fetch_sub(1, Ordering::Release);
            }
        }
    }

    /// Acquire the write lock. Blocks until no readers and no other writer.
    ///
    /// TODO: Implement write lock acquisition (writer-priority)
    /// 1. Set WRITER_WAITING first: fetch_or(WRITER_WAITING, Release) so new readers will block.
    /// 2. In a loop: load state; if any readers (READER_MASK) or WRITER_HOLDING, spin_loop and continue.
    /// 3. Try compare_exchange(WRITER_WAITING, WRITER_HOLDING, ...) to take the lock; or compare_exchange(0, WRITER_HOLDING, ...) if a writer just released.
    /// 4. On success return RwLockWriteGuard { lock: self }.
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        // TODO
        self.waiting_writers.fetch_add(1, Ordering::SeqCst);
        self.state.fetch_or(WRITER_WAITING, Ordering::Release);
        loop {
            let state = self.state.load(Ordering::Acquire);
            if state & (READER_MASK | WRITER_HOLDING) == 0
                && self
                    .state
                    .compare_exchange(state, WRITER_HOLDING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                self.waiting_writers.fetch_sub(1, Ordering::SeqCst);
                return RwLockWriteGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }
}

/// Guard for a read lock; releases the read lock on drop.
pub struct RwLockReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

// TODO: Implement Deref for RwLockReadGuard
// Return shared reference to data: unsafe { &*self.lock.data.get() }
impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

// TODO: Implement Drop for RwLockReadGuard
// Decrement reader count: self.lock.state.fetch_sub(1, Ordering::Release)
impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

/// Guard for a write lock; releases the write lock on drop.
pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

// TODO: Implement Deref for RwLockWriteGuard
// Return shared reference: unsafe { &*self.lock.data.get() }
impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

// TODO: Implement DerefMut for RwLockWriteGuard
// Return mutable reference: unsafe { &mut *self.lock.data.get() }
impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

// TODO: Implement Drop for RwLockWriteGuard
// Clear writer bits so lock is free: self.lock.state.fetch_and(!(WRITER_HOLDING | WRITER_WAITING), Ordering::Release)
impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock
            .state
            .fetch_and(!(WRITER_HOLDING | WRITER_WAITING), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_multiple_readers() {
        let lock = Arc::new(RwLock::new(0u32));
        let mut handles = vec![];
        for _ in 0..10 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                let g = l.read();
                assert_eq!(*g, 0);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_writer_excludes_readers() {
        let lock = Arc::new(RwLock::new(0u32));
        let lock_w = Arc::clone(&lock);
        let writer = thread::spawn(move || {
            let mut g = lock_w.write();
            *g = 42;
        });
        writer.join().unwrap();
        let g = lock.read();
        assert_eq!(*g, 42);
    }

    #[test]
    fn test_concurrent_reads_after_write() {
        let lock = Arc::new(RwLock::new(Vec::<i32>::new()));
        {
            let mut g = lock.write();
            g.push(1);
            g.push(2);
        }
        let mut handles = vec![];
        for _ in 0..5 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                let g = l.read();
                assert_eq!(g.len(), 2);
                assert_eq!(&*g, &[1, 2]);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_writes_serialized() {
        let lock = Arc::new(RwLock::new(0u64));
        let mut handles = vec![];
        for _ in 0..10 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let mut g = l.write();
                    *g += 1;
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*lock.read(), 1000);
    }

    #[test]
    fn test_waiting_writers_run_before_new_reader() {
        let lock = Arc::new(RwLock::new(0u32));
        let initial_reader = lock.read();
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut writers = Vec::new();
        for _ in 0..2 {
            let lock = Arc::clone(&lock);
            let sender = sender.clone();
            writers.push(thread::spawn(move || {
                let mut guard = lock.write();
                *guard += 1;
                sender.send("writer").unwrap();
            }));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lock.waiting_writers.load(Ordering::SeqCst) != 2 {
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
        let reader_lock = Arc::clone(&lock);
        let reader = thread::spawn(move || {
            let guard = reader_lock.read();
            assert_eq!(*guard, 2);
            sender.send("reader").unwrap();
        });
        drop(initial_reader);
        for expected in ["writer", "writer", "reader"] {
            assert_eq!(
                receiver
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                expected
            );
        }
        for writer in writers {
            writer.join().unwrap();
        }
        reader.join().unwrap();
    }
}
