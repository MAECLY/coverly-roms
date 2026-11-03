//! Terminal echo for typing a secret (`coverly integrations set`,
//! integrations spec D20): while an [`EchoOff`] lives, what is typed on
//! the terminal attached to stdin is not shown. The previous mode comes
//! back when the guard drops, also on an error or a panic that unwinds,
//! and on Unix before Ctrl+C, a closed terminal or `kill` ends the process.
//! Only the platform calls the workspace already depends on: termios
//! through `libc` on Unix, `SetConsoleMode` through `windows-sys` on
//! Windows; no new crate.

use std::io;

/// Turns echo off on the terminal behind stdin until dropped.
pub struct EchoOff {
    #[cfg(unix)]
    fd: libc::c_int,
    #[cfg(unix)]
    saved: libc::termios,
    /// The signals whose handler this guard installed, and the
    /// disposition each had before.
    #[cfg(unix)]
    previous: Vec<(libc::c_int, libc::sigaction)>,
    #[cfg(windows)]
    saved: u32,
}

/// The signals that end the process at a prompt (Ctrl+C, Ctrl+\, a closed
/// terminal, `kill`): `Drop` never runs for them, so a handler puts the
/// echo back before the default action.
#[cfg(unix)]
const ENDING_SIGNALS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGQUIT, libc::SIGHUP, libc::SIGTERM];

/// What a signal handler needs to restore the terminal: written before
/// `ARMED` is set, read only while it is.
#[cfg(unix)]
struct Pending {
    fd: std::sync::atomic::AtomicI32,
    saved: std::cell::UnsafeCell<std::mem::MaybeUninit<libc::termios>>,
}

// SAFETY: `saved` is written only while `ARMED` is false (no handler reads
// it then) and read only after `ARMED` was seen true.
#[cfg(unix)]
unsafe impl Sync for Pending {}

#[cfg(unix)]
static PENDING: Pending = Pending {
    fd: std::sync::atomic::AtomicI32::new(-1),
    saved: std::cell::UnsafeCell::new(std::mem::MaybeUninit::uninit()),
};

#[cfg(unix)]
static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Puts back the terminal mode a live guard saved (once). Only
/// async-signal-safe calls: it runs inside a signal handler.
#[cfg(unix)]
fn restore_saved() {
    use std::sync::atomic::Ordering;
    if ARMED.swap(false, Ordering::SeqCst) {
        let fd = PENDING.fd.load(Ordering::SeqCst);
        // SAFETY: `saved` was initialised before ARMED was set; tcsetattr
        // is async-signal-safe.
        unsafe {
            libc::tcsetattr(fd, libc::TCSANOW, (*PENDING.saved.get()).as_ptr());
        }
    }
}

/// The handler of [`ENDING_SIGNALS`] while a guard lives: echo back, then
/// the default action of the signal (it was the default before).
#[cfg(unix)]
extern "C" fn on_signal(sig: libc::c_int) {
    restore_saved();
    // SAFETY: signal and raise are async-signal-safe.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(unix)]
impl EchoOff {
    /// Echo off on stdin; an error when stdin is not a terminal.
    pub fn stdin() -> io::Result<EchoOff> {
        EchoOff::on_fd(libc::STDIN_FILENO)
    }

    fn on_fd(fd: libc::c_int) -> io::Result<EchoOff> {
        use std::sync::atomic::Ordering;
        // SAFETY: an all-zero termios is a valid value to be overwritten
        // by tcgetattr.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: a descriptor and a valid pointer to a termios we own.
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // Disarmed first, so no handler reads `saved` while it changes.
        ARMED.store(false, Ordering::SeqCst);
        // SAFETY: not armed, so nothing else reads or writes it now.
        unsafe { (*PENDING.saved.get()).write(saved) };
        PENDING.fd.store(fd, Ordering::SeqCst);
        ARMED.store(true, Ordering::SeqCst);
        let previous = install_handlers();
        let mut quiet = saved;
        quiet.c_lflag &= !libc::ECHO;
        // The Enter that ends the line still moves to the next line.
        quiet.c_lflag |= libc::ECHONL;
        // SAFETY: the descriptor and a valid termios copied from its own.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) } != 0 {
            let e = io::Error::last_os_error();
            ARMED.store(false, Ordering::SeqCst);
            restore_handlers(&previous);
            return Err(e);
        }
        Ok(EchoOff {
            fd,
            saved,
            previous,
        })
    }
}

/// Installs [`on_signal`] for each ending signal left at its default
/// action (one the caller ignores or handles is not touched).
#[cfg(unix)]
fn install_handlers() -> Vec<(libc::c_int, libc::sigaction)> {
    let mut previous = Vec::new();
    for sig in ENDING_SIGNALS {
        // SAFETY: zeroed sigaction structs filled by the kernel or by us;
        // the handler only makes async-signal-safe calls.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig, std::ptr::null(), &mut old) != 0
                || old.sa_sigaction != libc::SIG_DFL
            {
                continue;
            }
            let mut new: libc::sigaction = std::mem::zeroed();
            new.sa_sigaction = on_signal as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut new.sa_mask);
            if libc::sigaction(sig, &new, std::ptr::null_mut()) == 0 {
                previous.push((sig, old));
            }
        }
    }
    previous
}

#[cfg(unix)]
fn restore_handlers(previous: &[(libc::c_int, libc::sigaction)]) {
    for (sig, old) in previous {
        // SAFETY: a disposition read from the kernel for this signal.
        unsafe {
            libc::sigaction(*sig, old, std::ptr::null_mut());
        }
    }
}

#[cfg(unix)]
impl Drop for EchoOff {
    fn drop(&mut self) {
        ARMED.store(false, std::sync::atomic::Ordering::SeqCst);
        restore_handlers(&self.previous);
        // SAFETY: the descriptor and the termios read when the guard was made.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved);
        }
    }
}

#[cfg(windows)]
impl EchoOff {
    /// Echo off on the console input; an error when stdin is not a console.
    pub fn stdin() -> io::Result<EchoOff> {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, SetConsoleMode, ENABLE_ECHO_INPUT,
        };
        let handle = stdin_handle()?;
        let mut saved = 0u32;
        // SAFETY: a console handle from GetStdHandle and a valid out pointer.
        if unsafe { GetConsoleMode(handle, &mut saved) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the same handle and a mode derived from its current one.
        if unsafe { SetConsoleMode(handle, saved & !ENABLE_ECHO_INPUT) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(EchoOff { saved })
    }
}

#[cfg(windows)]
fn stdin_handle() -> io::Result<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE};
    // SAFETY: GetStdHandle takes a constant and has no other input.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other("no console input"));
    }
    Ok(handle)
}

#[cfg(windows)]
impl Drop for EchoOff {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Console::SetConsoleMode;
        if let Ok(handle) = stdin_handle() {
            // SAFETY: the console handle and the mode read when the guard
            // was made.
            unsafe {
                SetConsoleMode(handle, self.saved);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
impl EchoOff {
    /// No terminal control on this platform.
    pub fn stdin() -> io::Result<EchoOff> {
        Err(io::Error::other("no terminal control on this platform"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::IsTerminal;

    #[test]
    fn stdin_that_is_not_a_terminal_is_an_error() {
        // `cargo test` runs with stdin from the harness, not a terminal;
        // skip on a developer's interactive run.
        if std::io::stdin().is_terminal() {
            return;
        }
        assert!(EchoOff::stdin().is_err());
    }

    /// The terminal side of a fresh pseudo-terminal (closed on drop).
    #[cfg(unix)]
    struct Pty {
        master: libc::c_int,
        slave: libc::c_int,
    }

    #[cfg(unix)]
    impl Pty {
        fn open() -> Pty {
            // SAFETY: plain libc calls on descriptors this test owns; the
            // name from ptsname is copied before any other pty call.
            unsafe {
                let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
                assert!(master >= 0, "posix_openpt");
                assert_eq!(libc::grantpt(master), 0);
                assert_eq!(libc::unlockpt(master), 0);
                let name = std::ffi::CStr::from_ptr(libc::ptsname(master)).to_owned();
                let slave = libc::open(name.as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
                assert!(slave >= 0, "open slave");
                Pty { master, slave }
            }
        }

        fn echo(&self) -> bool {
            // SAFETY: a zeroed termios filled by tcgetattr on our fd.
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                assert_eq!(libc::tcgetattr(self.slave, &mut t), 0);
                t.c_lflag & libc::ECHO != 0
            }
        }
    }

    #[cfg(unix)]
    impl Drop for Pty {
        fn drop(&mut self) {
            // SAFETY: descriptors opened by `open`.
            unsafe {
                libc::close(self.slave);
                libc::close(self.master);
            }
        }
    }

    #[cfg(unix)]
    fn sigint_handler() -> libc::sighandler_t {
        // SAFETY: querying (not changing) the disposition of SIGINT.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            assert_eq!(libc::sigaction(libc::SIGINT, std::ptr::null(), &mut old), 0);
            old.sa_sigaction
        }
    }

    /// Ctrl+C at the hidden prompt kills the process without running
    /// `Drop`: the signal handler must bring the echo back first, or the
    /// shell is left typing blind.
    #[cfg(unix)]
    #[test]
    fn a_signal_while_typing_brings_the_echo_back() {
        let pty = Pty::open();
        assert!(pty.echo());
        let before = sigint_handler();
        let guard = EchoOff::on_fd(pty.slave).unwrap();
        assert!(!pty.echo());
        if before == libc::SIG_DFL {
            assert_eq!(
                sigint_handler(),
                on_signal as *const () as libc::sighandler_t
            );
        }
        // What the handler does before re-raising the signal.
        restore_saved();
        assert!(pty.echo());
        drop(guard);
        assert!(pty.echo());
        assert_eq!(sigint_handler(), before);
    }
}
