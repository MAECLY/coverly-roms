//! Terminal echo for typing a secret (`coverly integrations set`,
//! integrations spec D20): while an [`EchoOff`] lives, what is typed on
//! the terminal attached to stdin is not shown. The previous mode comes
//! back when the guard drops, also on an error or a panic that unwinds.
//! Only the platform calls the workspace already depends on: termios
//! through `libc` on Unix, `SetConsoleMode` through `windows-sys` on
//! Windows; no new crate.

use std::io;

/// Turns echo off on the terminal behind stdin until dropped.
pub struct EchoOff {
    #[cfg(unix)]
    saved: libc::termios,
    #[cfg(windows)]
    saved: u32,
}

#[cfg(unix)]
impl EchoOff {
    /// Echo off on stdin; an error when stdin is not a terminal.
    pub fn stdin() -> io::Result<EchoOff> {
        // SAFETY: an all-zero termios is a valid value to be overwritten
        // by tcgetattr.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: fd 0 and a valid pointer to a termios we own.
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut quiet = saved;
        quiet.c_lflag &= !libc::ECHO;
        // The Enter that ends the line still moves to the next line.
        quiet.c_lflag |= libc::ECHONL;
        // SAFETY: fd 0 and a valid termios copied from the current one.
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &quiet) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(EchoOff { saved })
    }
}

#[cfg(unix)]
impl Drop for EchoOff {
    fn drop(&mut self) {
        // SAFETY: fd 0 and the termios read when the guard was made.
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
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
}
