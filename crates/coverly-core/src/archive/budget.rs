//! Decode budgets and the decoder memory gate (spec
//! `2026-10-23-organize-archives-and-discs.md` §4.4, §4.5).
//!
//! A [`Budget`] is a value passed by `&mut` to every archive read: every
//! byte a decoder produces is charged to it, including bytes skipped to
//! reach an entry inside a solid block and a tar trailer drain. A spent
//! budget is [`ArchiveError::BudgetSpent`], which callers treat as *not
//! read*, never as *damaged*. Plan-time budgets also carry a wall-time
//! deadline; the apply budget has neither a byte nor a time limit and
//! only honours a cancel flag.
//!
//! The [`MemoryGate`] is the one global piece: a counting semaphore over
//! decoder memory (512 MiB across threads), so parallel planning never
//! holds more decoder state than that. A request above the cap is refused
//! with [`ArchiveError::NeedsTooMuchMemory`]; a request that fits waits
//! until enough permits are dropped. A folder with several coders asks for
//! the sum of their memory in one request ([`MemoryGate::acquire_all`]),
//! never coder by coder, so two half-acquired folders cannot deadlock.

use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use super::{ArchiveError, BudgetKind, Limits, MIB};

/// Decoder memory held across every thread at once (spec §4.4).
pub const MEMORY_GATE: u64 = 512 * MIB;

/// Bytes a decoder may still produce, an optional deadline and an optional
/// cancel flag (spec §4.4).
/// Not `Clone`: a copy would double the bytes it allows.
#[derive(Debug)]
pub struct Budget {
    /// `None`: no byte limit (apply).
    left: Option<u64>,
    spent: u64,
    deadline: Option<Instant>,
    cancel: Option<Arc<AtomicBool>>,
}

impl Budget {
    /// Plan time, reading heads: `plan_head_budget` decoded bytes and the
    /// per-archive wall time.
    pub fn plan_head(l: &Limits) -> Budget {
        Self::planned(l.plan_head_budget, l)
    }

    /// Plan time, listing a tar inside a compressor: `plan_listing_budget`
    /// decoded bytes and the per-archive wall time.
    pub fn plan_listing(l: &Limits) -> Budget {
        Self::planned(l.plan_listing_budget, l)
    }

    fn planned(bytes: u64, l: &Limits) -> Budget {
        Budget {
            left: Some(bytes),
            spent: 0,
            // An unrepresentable deadline (a huge test wall time) is none.
            deadline: Instant::now().checked_add(l.plan_wall_time),
            cancel: None,
        }
    }

    /// Apply: no byte limit and no wall time (spec §4.5); only a cancel
    /// flag, when one is attached with [`Budget::with_cancel`].
    pub fn apply() -> Budget {
        Budget {
            left: None,
            spent: 0,
            deadline: None,
            cancel: None,
        }
    }

    /// Attaches a cancel flag: once it is set, every charge fails with
    /// [`ArchiveError::Cancelled`].
    pub fn with_cancel(self, c: Arc<AtomicBool>) -> Budget {
        Budget {
            cancel: Some(c),
            ..self
        }
    }

    /// Fails when the budget is cancelled or past its deadline, without
    /// charging anything.
    pub fn check(&self) -> Result<(), ArchiveError> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(ArchiveError::Cancelled);
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err(ArchiveError::BudgetSpent {
                what: BudgetKind::Time,
            });
        }
        Ok(())
    }

    /// Charges `n` decoded bytes. Fails with `BudgetSpent` when they do not
    /// fit (the bytes still count as spent), when the deadline has passed,
    /// or with `Cancelled` when the cancel flag is set.
    pub fn charge(&mut self, n: u64) -> Result<(), ArchiveError> {
        self.spent = self.spent.saturating_add(n);
        if let Some(left) = self.left.as_mut() {
            if n > *left {
                *left = 0;
                return Err(ArchiveError::BudgetSpent {
                    what: BudgetKind::DecodedBytes,
                });
            }
            *left -= n;
        }
        self.check()
    }

    /// Decoded bytes charged so far.
    pub fn spent(&self) -> u64 {
        self.spent
    }

    /// Decoded bytes still allowed; `None` when there is no byte limit.
    pub fn remaining(&self) -> Option<u64> {
        self.left
    }

    /// The wall-time deadline; `None` at apply.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

/// A reader that charges every byte it returns to a [`Budget`].
///
/// It never returns more bytes than the budget has left: the read that
/// would cross the limit fails with `BudgetSpent` (as an `io::Error`;
/// see [`super::is_budget_spent`]), so an entry that ends exactly at the
/// limit still reads to EOF.
pub struct ChargedRead<'b, R> {
    inner: R,
    budget: &'b mut Budget,
}

impl<'b, R: Read> ChargedRead<'b, R> {
    pub fn new(inner: R, budget: &'b mut Budget) -> Self {
        Self { inner, budget }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for ChargedRead<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.budget.check()?;
        if buf.is_empty() {
            return Ok(0);
        }
        // Ask for at most one byte past what is left: reading exactly the
        // budget and then EOF succeeds, one byte more fails.
        let want = match self.budget.remaining() {
            Some(left) => {
                usize::try_from(left.saturating_add(1)).map_or(buf.len(), |cap| cap.min(buf.len()))
            }
            None => buf.len(),
        };
        let n = self.inner.read(&mut buf[..want])?;
        self.budget.charge(n as u64)?;
        Ok(n)
    }
}

/// The global decoder memory semaphore (spec §4.4 `MEMORY_GATE`).
#[derive(Debug)]
pub struct MemoryGate {
    cap: u64,
    used: Mutex<u64>,
    freed: Condvar,
}

/// Decoder memory held from a [`MemoryGate`]; dropping it (also while a
/// panic unwinds) gives the memory back and wakes waiting threads.
#[derive(Debug)]
#[must_use = "dropping a permit releases its memory at once"]
pub struct Permit {
    gate: &'static MemoryGate,
    bytes: u64,
}

impl MemoryGate {
    fn new(cap: u64) -> Self {
        Self {
            cap,
            used: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// The process-wide gate of [`MEMORY_GATE`] bytes. In tests,
    /// [`with_gate_cap`] swaps it for a smaller one on the calling thread.
    pub fn global() -> &'static MemoryGate {
        #[cfg(any(test, feature = "test-util"))]
        if let Some(gate) = GATE_OVERRIDE.with(std::cell::Cell::get) {
            return gate;
        }
        static GLOBAL: OnceLock<MemoryGate> = OnceLock::new();
        GLOBAL.get_or_init(|| MemoryGate::new(MEMORY_GATE))
    }

    /// A fresh gate with its own cap, for tests that share one gate
    /// between threads.
    #[cfg(any(test, feature = "test-util"))]
    pub fn leaked(cap: u64) -> &'static MemoryGate {
        Box::leak(Box::new(Self::new(cap)))
    }

    pub fn cap(&self) -> u64 {
        self.cap
    }

    /// Bytes currently held by live permits.
    pub fn in_use(&self) -> u64 {
        *self.used.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes `bytes` of decoder memory, waiting until it fits. A request
    /// above the cap can never fit and fails with `NeedsTooMuchMemory`.
    pub fn acquire(&'static self, bytes: u64) -> Result<Permit, ArchiveError> {
        if bytes > self.cap {
            return Err(ArchiveError::NeedsTooMuchMemory {
                needed: bytes,
                cap: self.cap,
            });
        }
        // The lock is never held while a decoder runs, so a poisoned mutex
        // only means a panic between two lines here: the count is intact.
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        while self.cap - *used < bytes {
            used = self
                .freed
                .wait(used)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *used += bytes;
        Ok(Permit { gate: self, bytes })
    }

    /// Takes the sum of several coders' memory in one request (a 7z folder
    /// with BCJ2 + LZMA ×3), never part by part.
    pub fn acquire_all(&'static self, parts: &[u64]) -> Result<Permit, ArchiveError> {
        let total = parts
            .iter()
            .try_fold(0u64, |sum, &p| sum.checked_add(p))
            .unwrap_or(u64::MAX);
        self.acquire(total)
    }
}

impl Permit {
    /// Bytes this permit holds.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if self.bytes == 0 {
            return;
        }
        let mut used = self
            .gate
            .used
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *used = used.saturating_sub(self.bytes);
        drop(used);
        self.gate.freed.notify_all();
    }
}

/// Runs `f` with [`MemoryGate::global`] replaced, on this thread only, by
/// a fresh gate of `cap` bytes. The previous gate comes back afterwards,
/// also when `f` panics.
#[cfg(any(test, feature = "test-util"))]
pub fn with_gate_cap<T>(cap: u64, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<&'static MemoryGate>);
    impl Drop for Restore {
        fn drop(&mut self) {
            GATE_OVERRIDE.with(|g| g.set(self.0));
        }
    }
    let gate = MemoryGate::leaked(cap);
    let _restore = Restore(GATE_OVERRIDE.with(|g| g.replace(Some(gate))));
    f()
}

#[cfg(any(test, feature = "test-util"))]
thread_local! {
    static GATE_OVERRIDE: std::cell::Cell<Option<&'static MemoryGate>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn limits(head: u64, listing: u64, wall: Duration) -> Limits {
        Limits {
            plan_head_budget: head,
            plan_listing_budget: listing,
            plan_wall_time: wall,
            ..Limits::default()
        }
    }

    #[test]
    fn plan_budgets_take_their_limits() {
        let l = Limits::default();
        let head = Budget::plan_head(&l);
        assert_eq!(head.remaining(), Some(l.plan_head_budget));
        assert!(head.deadline().is_some());
        let listing = Budget::plan_listing(&l);
        assert_eq!(listing.remaining(), Some(l.plan_listing_budget));
        assert!(listing.deadline().is_some());
        assert_eq!(head.spent(), 0);
    }

    #[test]
    fn budget_spent_is_not_damage() {
        let l = limits(10, 10, Duration::from_secs(60));
        let mut b = Budget::plan_head(&l);
        b.charge(10).expect("exactly the budget fits");
        let err = b.charge(1).expect_err("one byte over");
        assert!(
            matches!(
                err,
                ArchiveError::BudgetSpent {
                    what: BudgetKind::DecodedBytes
                }
            ),
            "{err:?}"
        );
        let io_err: io::Error = err.into();
        assert!(super::super::is_budget_spent(&io_err));
        assert_ne!(io_err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(b.spent(), 11);
        assert_eq!(b.remaining(), Some(0));

        // A plan budget whose wall time is over is spent too, never damage.
        let late = Budget::plan_head(&limits(1 << 20, 1 << 20, Duration::ZERO));
        let err = late.check().expect_err("deadline passed");
        assert!(
            matches!(
                err,
                ArchiveError::BudgetSpent {
                    what: BudgetKind::Time
                }
            ),
            "{err:?}"
        );
        let io_err: io::Error = err.into();
        assert!(super::super::is_budget_spent(&io_err));
    }

    #[test]
    fn charged_read_counts_every_byte() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let mut b = Budget::plan_head(&limits(1000, 1000, Duration::from_secs(60)));
        let mut out = Vec::new();
        {
            let mut r = ChargedRead::new(io::Cursor::new(&data), &mut b);
            let mut chunk = [0u8; 7];
            loop {
                let n = r.read(&mut chunk).expect("within budget");
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&chunk[..n]);
            }
        }
        assert_eq!(out, data);
        assert_eq!(b.spent(), 1000);
        assert_eq!(b.remaining(), Some(0));

        // One byte more than the budget: the read crossing it fails, and
        // the error is a spent budget (std read loops do not retry it).
        let mut small = Budget::plan_head(&limits(999, 999, Duration::from_secs(60)));
        let mut sink = Vec::new();
        let err = ChargedRead::new(io::Cursor::new(&data), &mut small)
            .read_to_end(&mut sink)
            .expect_err("over budget");
        assert!(super::super::is_budget_spent(&err), "{err:?}");
        assert!(sink.len() <= 999, "never more than the budget");
    }

    #[test]
    fn charged_read_honours_cancel() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut b = Budget::apply().with_cancel(cancel.clone());
        let data = vec![0u8; 64];
        let mut r = ChargedRead::new(io::Cursor::new(&data), &mut b);
        let mut buf = [0u8; 16];
        assert_eq!(r.read(&mut buf).expect("not cancelled yet"), 16);
        cancel.store(true, Ordering::SeqCst);
        let err = r.read(&mut buf).expect_err("cancelled");
        let inner = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<ArchiveError>())
            .expect("ArchiveError inside");
        assert!(matches!(inner, ArchiveError::Cancelled), "{inner:?}");
        assert_ne!(err.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn apply_budget_has_no_deadline() {
        let mut b = Budget::apply();
        assert_eq!(b.deadline(), None);
        assert_eq!(b.remaining(), None);
        for _ in 0..4 {
            b.charge(16 * super::super::GIB)
                .expect("apply has no byte limit");
        }
        assert_eq!(b.spent(), 64 * super::super::GIB);
        b.charge(u64::MAX).expect("never overflows");
        assert_eq!(b.spent(), u64::MAX);
        b.check().expect("no deadline");
    }

    #[test]
    fn gate_blocks_until_release() {
        let gate = MemoryGate::leaked(512 * MIB);
        let first = gate.acquire(300 * MIB).expect("fits");
        assert_eq!(gate.in_use(), 300 * MIB);
        let (tx, rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let second = gate.acquire(300 * MIB).expect("fits after release");
            tx.send(second.bytes()).expect("main is listening");
            drop(second);
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "second permit must wait while 300 + 300 > 512 MiB"
        );
        drop(first);
        let got = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("woken by the release");
        assert_eq!(got, 300 * MIB);
        waiter.join().expect("waiter finished");
        assert_eq!(gate.in_use(), 0);
    }

    #[test]
    fn gate_request_over_cap_refused() {
        let gate = MemoryGate::leaked(512 * MIB);
        let err = gate.acquire(512 * MIB + 1).expect_err("can never fit");
        assert!(
            matches!(
                err,
                ArchiveError::NeedsTooMuchMemory { needed, cap }
                    if needed == 512 * MIB + 1 && cap == 512 * MIB
            ),
            "{err:?}"
        );
        assert_eq!(gate.in_use(), 0);
        let whole = gate.acquire(512 * MIB).expect("the whole cap fits");
        drop(whole);
        assert_eq!(gate.in_use(), 0);
    }

    #[test]
    fn multi_coder_folder_acquires_sum_once() {
        let gate = MemoryGate::leaked(512 * MIB);
        let folder = gate
            .acquire_all(&[MIB, 200 * MIB, 200 * MIB, 100 * MIB])
            .expect("501 MiB fits");
        assert_eq!(folder.bytes(), 501 * MIB);
        assert_eq!(gate.in_use(), 501 * MIB);
        drop(folder);

        // 300 + 300 is over the cap as a whole: refused at once, nothing
        // held half-way (a coder-by-coder request would hold 300 and wait).
        let err = gate
            .acquire_all(&[300 * MIB, 300 * MIB])
            .expect_err("sum over cap");
        assert!(
            matches!(err, ArchiveError::NeedsTooMuchMemory { needed, .. } if needed == 600 * MIB),
            "{err:?}"
        );
        assert_eq!(gate.in_use(), 0);

        let err = gate
            .acquire_all(&[u64::MAX, 1])
            .expect_err("overflowing sum");
        assert!(
            matches!(
                err,
                ArchiveError::NeedsTooMuchMemory {
                    needed: u64::MAX,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn global_gate_is_512_mib_and_overridable_per_thread() {
        assert_eq!(MemoryGate::global().cap(), MEMORY_GATE);
        assert_eq!(MEMORY_GATE, 512 * MIB);
        let inner = with_gate_cap(MIB, || MemoryGate::global().cap());
        assert_eq!(inner, MIB);
        assert_eq!(MemoryGate::global().cap(), MEMORY_GATE, "restored");
        let other = std::thread::spawn(|| with_gate_cap(2 * MIB, || MemoryGate::global().cap()))
            .join()
            .expect("thread");
        assert_eq!(other, 2 * MIB);
        let restored = std::panic::catch_unwind(|| {
            with_gate_cap(MIB, || panic!("inside the override"));
        });
        assert!(restored.is_err());
        assert_eq!(MemoryGate::global().cap(), MEMORY_GATE, "restored on panic");
    }
}
