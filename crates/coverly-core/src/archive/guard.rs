//! Panic guard around every archive backend (spec
//! `2026-10-23-organize-archives-and-discs.md` §13, S1).
//!
//! Third-party parsers and decoders can panic on hostile input (an index
//! out of bounds, an arithmetic overflow, an `unwrap`). [`guarded`] runs a
//! backend call inside `catch_unwind` and turns a panic into
//! [`ArchiveError::Damaged`] with the detail [`PANIC_DETAIL`]. A reader
//! that panicked once is poisoned ([`Poison`]): every later call fails with
//! the same error, and the reader is never reused or cached.
//!
//! The panic hook is process-global, so it is installed once
//! ([`install_panic_hook`]) and suppresses the stderr print only while the
//! panicking thread is inside a guard (a thread-local depth counter). Every
//! other panic still reaches the previous hook. A hook swapped per call
//! would race between planner threads and could hide a real panic.
//!
//! What a guard cannot catch: allocation failure and stack overflow abort
//! the process. The memory caps (S2, S3, S5) and iterative parsers prevent
//! them. Decoders run on the calling thread, because a panic in a spawned
//! thread escapes `catch_unwind`.

use std::cell::Cell;
use std::io::{self, Read};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

use super::{ArchiveError, ArchiveReader, Budget, Entry, Format};

// A guard only works when panics unwind; `panic = "abort"` would kill the
// planner on the first hostile archive.
const _: () = assert!(cfg!(panic = "unwind"));

/// The `Damaged` detail of a caught panic.
pub const PANIC_DETAIL: &str = "decoder panicked";

thread_local! {
    /// How many [`guarded`] calls are running on this thread.
    static IN_GUARD: Cell<u32> = const { Cell::new(0) };
}

/// Installs the process-wide panic hook once. It stays silent for panics
/// raised inside [`guarded`] on the panicking thread and hands every other
/// panic to the hook that was there before. The app and the CLI call it at
/// startup; [`guarded`] also calls it on first use.
pub fn install_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if IN_GUARD.with(Cell::get) > 0 {
                return;
            }
            #[cfg(test)]
            tests::record_printed(info);
            previous(info);
        }));
    });
}

/// Restores the thread's guard depth, also while a panic unwinds.
struct Depth;

impl Depth {
    fn enter() -> Self {
        IN_GUARD.with(|d| d.set(d.get().saturating_add(1)));
        Depth
    }
}

impl Drop for Depth {
    fn drop(&mut self) {
        IN_GUARD.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// The result of a guarded call: its own value, or the caught panic.
fn catch<T>(f: impl FnOnce() -> T) -> Option<T> {
    install_panic_hook();
    let _depth = Depth::enter();
    panic::catch_unwind(AssertUnwindSafe(f)).ok()
}

fn panicked() -> ArchiveError {
    ArchiveError::Damaged {
        detail: PANIC_DETAIL.into(),
    }
}

/// Runs a backend call; a panic inside it becomes
/// `Damaged { detail: "decoder panicked" }`. Values dropped while the
/// panic unwinds (a [`super::Permit`], open files) are released as usual.
pub fn guarded<T>(f: impl FnOnce() -> Result<T, ArchiveError>) -> Result<T, ArchiveError> {
    catch(f).unwrap_or_else(|| Err(panicked()))
}

/// The poisoned flag of one reader. Clones share it: a panic in any
/// guarded call or stream read of the reader poisons all of them.
#[derive(Clone, Debug, Default)]
pub struct Poison(Arc<AtomicBool>);

impl Poison {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_poisoned(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn set(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// `Damaged` once the reader has panicked; readers check it at every
    /// entry point.
    pub fn check(&self) -> Result<(), ArchiveError> {
        if self.is_poisoned() {
            Err(panicked())
        } else {
            Ok(())
        }
    }

    /// [`guarded`] that fails at once on a poisoned reader and poisons it
    /// when `f` panics.
    pub fn guard<T>(&self, f: impl FnOnce() -> Result<T, ArchiveError>) -> Result<T, ArchiveError> {
        self.check()?;
        catch(f).unwrap_or_else(|| {
            self.set();
            Err(panicked())
        })
    }
}

/// A decoded stream whose every `read` runs inside the guard. After a
/// panic (here or in another call of the same reader) every read fails
/// with `Damaged`, mapped to `io::ErrorKind::InvalidData`.
pub struct GuardedRead<R> {
    inner: R,
    poison: Poison,
}

impl<R> GuardedRead<R> {
    pub fn new(inner: R, poison: Poison) -> Self {
        Self { inner, poison }
    }

    pub fn poison(&self) -> &Poison {
        &self.poison
    }
}

impl<R: Read> Read for GuardedRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.poison.check()?;
        let inner = &mut self.inner;
        match catch(|| inner.read(buf)) {
            Some(result) => result,
            None => {
                self.poison.set();
                Err(panicked().into())
            }
        }
    }
}

/// An [`ArchiveReader`] whose every backend call, and every read of the
/// streams it returns, runs inside the guard. `open_archive` wraps each
/// backend in one; a poisoned one is never put in the reader cache.
pub struct GuardedArchive<A> {
    inner: A,
    poison: Poison,
}

impl<A: ArchiveReader> GuardedArchive<A> {
    pub fn new(inner: A) -> Self {
        Self {
            inner,
            poison: Poison::new(),
        }
    }

    /// Whether a backend call panicked; such a reader is dropped, not
    /// reused.
    pub fn is_poisoned(&self) -> bool {
        self.poison.is_poisoned()
    }

    pub fn poison(&self) -> &Poison {
        &self.poison
    }
}

impl<A: ArchiveReader> ArchiveReader for GuardedArchive<A> {
    fn format(&self) -> Format {
        self.inner.format()
    }

    fn entries(&self) -> &[Entry] {
        self.inner.entries()
    }

    fn open_entry<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let inner = &mut self.inner;
        let stream = self.poison.guard(move || inner.open_entry(index, budget))?;
        Ok(Box::new(GuardedRead::new(stream, self.poison.clone())))
    }

    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let inner = &mut self.inner;
        let poison = self.poison.clone();
        self.poison.guard(move || {
            inner.stream_entries(wanted, budget, &mut |entry, stream| {
                sink(entry, &mut GuardedRead::new(stream, poison.clone()))
            })
        })
    }
}

#[cfg(test)]
pub(crate) fn depth() -> u32 {
    IN_GUARD.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{EntryKind, MemoryGate, Method, RelPath};
    use std::panic::PanicHookInfo;
    use std::sync::Mutex;

    const MIB: u64 = 1024 * 1024;

    /// Messages of the panics the hook let through to the previous hook.
    static PRINTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    pub(super) fn record_printed(info: &PanicHookInfo<'_>) {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        PRINTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(msg);
    }

    fn printed(msg: &str) -> bool {
        PRINTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|m| m == msg)
    }

    fn is_panic_damage(e: &ArchiveError) -> bool {
        matches!(e, ArchiveError::Damaged { detail } if detail == PANIC_DETAIL)
    }

    /// A stream that panics on its first read, like a decoder hitting an
    /// out-of-bounds index on hostile input.
    struct PanickingStream;

    impl Read for PanickingStream {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            panic!("index out of bounds in a fake decoder");
        }
    }

    /// A backend whose streams panic; `stream_entries` panics too when
    /// `panic_in_stream_entries` is set.
    struct PanickingBackend {
        entries: Vec<Entry>,
        panic_in_stream_entries: bool,
    }

    impl PanickingBackend {
        fn new() -> Self {
            Self {
                entries: vec![Entry {
                    index: 0,
                    name: RelPath::parse("game.bin").unwrap(),
                    raw_name: b"game.bin".to_vec(),
                    kind: EntryKind::File,
                    size: 4,
                    packed: None,
                    crc32: None,
                    encrypted: false,
                    method: Method::Stored,
                    solid_block: None,
                    mtime: None,
                    unsafe_name: None,
                }],
                panic_in_stream_entries: false,
            }
        }
    }

    impl ArchiveReader for PanickingBackend {
        fn format(&self) -> Format {
            Format::Zip
        }
        fn entries(&self) -> &[Entry] {
            &self.entries
        }
        fn open_entry<'a>(
            &'a mut self,
            _index: usize,
            _budget: &'a mut Budget,
        ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
            Ok(Box::new(PanickingStream))
        }
        fn stream_entries(
            &mut self,
            _wanted: &[usize],
            _budget: &mut Budget,
            sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
        ) -> Result<(), ArchiveError> {
            if self.panic_in_stream_entries {
                panic!("overflow in a fake block walker");
            }
            sink(&self.entries[0], &mut PanickingStream)
        }
    }

    fn drain(r: &mut dyn Read) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        r.read_to_end(&mut out).map(|_| out)
    }

    #[test]
    fn guarded_passes_values_and_errors_through() {
        assert_eq!(guarded(|| Ok(7)).unwrap(), 7);
        let err = guarded::<()>(|| Err(ArchiveError::Encrypted)).unwrap_err();
        assert!(matches!(err, ArchiveError::Encrypted));
    }

    #[test]
    fn panicking_backend_maps_to_damaged() {
        let mut reader = GuardedArchive::new(PanickingBackend::new());
        let mut budget = Budget::apply();
        let mut stream = reader.open_entry(0, &mut budget).expect("open succeeds");
        let err = drain(&mut stream).expect_err("the read panicked");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let inner = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<ArchiveError>())
            .expect("an ArchiveError inside");
        assert!(is_panic_damage(inner), "{inner:?}");
        drop(stream);
        assert!(reader.is_poisoned());

        // A panic in the sink's stream of `stream_entries` is caught too.
        let mut reader = GuardedArchive::new(PanickingBackend::new());
        let err = reader
            .stream_entries(&[0], &mut budget, &mut |_, s| {
                drain(s).map(drop).map_err(ArchiveError::from)
            })
            .expect_err("the stream panicked");
        assert!(reader.is_poisoned());
        assert!(
            matches!(&err, ArchiveError::Io(e) if e.kind() == io::ErrorKind::InvalidData),
            "{err:?}"
        );

        // And a panic in the backend's own walker.
        let mut backend = PanickingBackend::new();
        backend.panic_in_stream_entries = true;
        let mut reader = GuardedArchive::new(backend);
        let err = reader
            .stream_entries(&[0], &mut budget, &mut |_, _| Ok(()))
            .expect_err("the walker panicked");
        assert!(is_panic_damage(&err), "{err:?}");
    }

    #[test]
    fn poisoned_reader_stays_damaged() {
        let mut reader = GuardedArchive::new(PanickingBackend::new());
        let mut budget = Budget::apply();
        {
            let mut stream = reader.open_entry(0, &mut budget).unwrap();
            assert!(drain(&mut stream).is_err());
        }
        assert!(reader.is_poisoned());
        // Every later entry point fails without reaching the backend.
        let err = reader.open_entry(0, &mut budget).err().expect("poisoned");
        assert!(is_panic_damage(&err), "{err:?}");
        let mut called = false;
        let err = reader
            .stream_entries(&[0], &mut budget, &mut |_, _| {
                called = true;
                Ok(())
            })
            .expect_err("poisoned");
        assert!(is_panic_damage(&err), "{err:?}");
        assert!(!called);

        // A stream of a poisoned reader fails before touching its inner.
        let poison = Poison::new();
        let mut first = GuardedRead::new(PanickingStream, poison.clone());
        let mut second = GuardedRead::new(io::Cursor::new(vec![1u8; 4]), poison.clone());
        assert!(drain(&mut first).is_err());
        assert!(poison.is_poisoned());
        let err = drain(&mut second).expect_err("shared poison");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn panic_hook_suppresses_only_guarded() {
        install_panic_hook();
        let guarded_msg = "guarded panic 5f1c";
        let err = guarded::<()>(|| panic!("{guarded_msg}")).unwrap_err();
        assert!(is_panic_damage(&err));

        let unguarded_msg = "unguarded panic 9e2a";
        let joined = std::thread::Builder::new()
            .name("unguarded".into())
            .spawn(move || panic!("{unguarded_msg}"))
            .unwrap()
            .join();
        assert!(joined.is_err());

        assert!(printed(unguarded_msg), "an unguarded panic is printed");
        assert!(!printed(guarded_msg), "a guarded panic is not");
    }

    #[test]
    fn gate_permit_released_on_panic() {
        crate::archive::budget::with_gate_cap(4 * MIB, || {
            let gate = MemoryGate::global();
            let err = guarded::<()>(|| {
                let _permit = gate.acquire(4 * MIB)?;
                panic!("decoder blew up while holding memory");
            })
            .unwrap_err();
            assert!(is_panic_damage(&err));
            assert_eq!(gate.in_use(), 0);
            // The whole cap is free again: this would block otherwise.
            let again = gate.acquire(4 * MIB).expect("cap free");
            assert_eq!(again.bytes(), 4 * MIB);
        });
    }

    #[test]
    fn nested_guard_depth_restored() {
        assert_eq!(depth(), 0);
        let out = guarded(|| {
            assert_eq!(depth(), 1);
            let inner = guarded::<()>(|| {
                assert_eq!(depth(), 2);
                panic!("inner decoder");
            });
            assert!(is_panic_damage(&inner.unwrap_err()));
            assert_eq!(depth(), 1);
            Ok(3)
        });
        assert_eq!(out.unwrap(), 3);
        assert_eq!(depth(), 0);

        // An outer panic after an inner one unwinds both levels.
        let err = guarded::<()>(|| {
            let _ = guarded::<()>(|| panic!("inner"));
            panic!("outer");
        })
        .unwrap_err();
        assert!(is_panic_damage(&err));
        assert_eq!(depth(), 0);
    }
}
