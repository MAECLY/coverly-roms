//! External archive tools: what no pure-Rust crate opens (RAR, and 7z
//! coders `sevenz-rust2` lacks) goes through a program already on the
//! user's machine, run as a separate process (spec §5.2).
//!
//! This module finds the tools ([`discover`]) and runs them with strict
//! process hygiene ([`run_tool`]): only an absolute regular executable, no
//! shell, `--` before the archive, a fresh empty working directory, a
//! cleared environment, stdin closed, both pipes drained, output capped,
//! and a timeout that kills the whole process tree. The listing parsers
//! (S7) and reads (S8) build on it.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{ArchiveError, Tool};

/// stderr kept per run, for the report; the rest is drained and dropped.
pub const STDERR_CAP: usize = 64 * 1024;
/// Output kept from a version banner call.
const BANNER_CAP: u64 = 64 * 1024;
/// Wall time of a version banner call.
const BANNER_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a run whose stdout has closed polls for the tool's exit.
const REAP_POLL: Duration = Duration::from_millis(5);

/// One external tool found on this machine, with its parsed version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundTool {
    pub tool: Tool,
    /// Absolute path of the executable.
    pub path: PathBuf,
    /// Version from the tool's banner (`7.01`, `24.08`, `3.7.4`).
    pub version: String,
    /// Whether this version opens RAR5 (unrar ≥ 5, 7-Zip ≥ 15, libarchive
    /// ≥ 3.6).
    pub rar5: bool,
}

/// Where [`discover_with`] looks. [`Search::from_env`] is the real one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Search {
    /// The `PATH` value; only its absolute entries are searched.
    pub path_var: Option<OsString>,
    /// The process's working directory, never searched.
    pub cwd: Option<PathBuf>,
    /// Fixed locations searched after `PATH`: a GUI app launched from
    /// Finder or Explorer does not inherit the shell's `PATH`.
    pub fixed: Vec<PathBuf>,
}

impl Search {
    /// This process's `PATH`, working directory and the per-OS fixed
    /// locations of spec §5.2.
    pub fn from_env() -> Self {
        Self {
            path_var: std::env::var_os("PATH"),
            cwd: std::env::current_dir().ok(),
            fixed: fixed_locations(),
        }
    }
}

#[cfg(target_os = "macos")]
fn fixed_locations() -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn fixed_locations() -> Vec<PathBuf> {
    ["/usr/bin", "/usr/local/bin"]
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

#[cfg(windows)]
fn fixed_locations() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(var).map(PathBuf::from) {
            if root.is_absolute() {
                out.push(root.join("7-Zip"));
                out.push(root.join("WinRAR"));
            }
        }
    }
    out
}

/// Executable names in preference order (spec §5.2): `unrar` → `7zz` →
/// `7z` → `bsdtar`.
const CANDIDATES: &[(&str, Tool)] = &[
    ("unrar", Tool::Unrar),
    ("7zz", Tool::SevenZip),
    ("7z", Tool::SevenZip),
    ("bsdtar", Tool::Bsdtar),
];

/// Finds the external tools with the real `PATH` and fixed locations.
/// `extra_roots_to_avoid` are the source and target roots: a `PATH` entry
/// inside one of them is never searched.
pub fn discover(extra_roots_to_avoid: &[&Path]) -> Vec<FoundTool> {
    discover_with(&Search::from_env(), extra_roots_to_avoid)
}

/// [`discover`] over an explicit [`Search`]. Each found executable's
/// version banner is read once; a tool whose banner is not recognised is
/// skipped. The result is in preference order, one entry per executable.
pub fn discover_with(search: &Search, extra_roots_to_avoid: &[&Path]) -> Vec<FoundTool> {
    let cwd = search.cwd.as_deref().map(canonical);
    let avoid: Vec<PathBuf> = extra_roots_to_avoid.iter().map(|p| canonical(p)).collect();
    let safe = |dir: &Path| -> bool {
        if !dir.is_absolute() {
            return false;
        }
        let dir = canonical(dir);
        cwd.as_ref().is_none_or(|c| *c != dir) && !avoid.iter().any(|root| dir.starts_with(root))
    };

    let mut dirs: Vec<PathBuf> = Vec::new();
    let path_dirs = search
        .path_var
        .as_deref()
        .map(|v| std::env::split_paths(v).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in path_dirs.into_iter().chain(search.fixed.iter().cloned()) {
        if safe(&dir) && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for &(name, tool) in CANDIDATES {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let Some(path) = dirs
            .iter()
            .map(|d| d.join(&file))
            .find(|p| executable_problem(p).is_none())
        else {
            continue;
        };
        let real = canonical(&path);
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        if let Some(found) = probe(tool, path) {
            out.push(found);
        }
    }
    out
}

/// The tool that opens a RAR archive: the first found that can (unrar 4
/// cannot open RAR5).
pub fn pick_rar_tool(tools: &[FoundTool], v5: bool) -> Option<&FoundTool> {
    tools.iter().find(|t| !v5 || t.rar5)
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Runs the tool's banner call and parses its version.
fn probe(tool: Tool, path: PathBuf) -> Option<FoundTool> {
    let args: &[&OsStr] = match tool {
        Tool::Bsdtar => &[OsStr::new("--version")],
        // unrar and 7-Zip print their banner first when called bare.
        Tool::Unrar | Tool::SevenZip => &[],
    };
    let mut t = FoundTool {
        tool,
        path,
        version: String::new(),
        rar5: false,
    };
    let run = spawn_bounded(&t, args, BANNER_CAP, BANNER_TIMEOUT, None).ok()?;
    let text = String::from_utf8_lossy(&run.stdout);
    let (version, rar5) = parse_banner(tool, &text)?;
    t.version = version;
    t.rar5 = rar5;
    Some(t)
}

/// `(version, opens RAR5)` from a tool's banner, or `None` when the banner
/// is not that tool's.
pub fn parse_banner(tool: Tool, text: &str) -> Option<(String, bool)> {
    match tool {
        Tool::Unrar => {
            let version = token_after(text, "UNRAR ")?;
            let rar5 = major(&version)? >= 5;
            Some((version, rar5))
        }
        Tool::SevenZip => {
            // `7-Zip (z) 24.08 (arm64)`, `7-Zip [64] 16.02`, `7-Zip 23.01`.
            let line = text.lines().find(|l| l.trim_start().starts_with("7-Zip"))?;
            let version = line
                .split_whitespace()
                .skip(1)
                .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
                .filter(|w| is_version(w))?
                .to_string();
            let rar5 = major(&version)? >= 15;
            Some((version, rar5))
        }
        Tool::Bsdtar => {
            let version = token_after(text, "bsdtar ")?;
            let libarchive = token_after(text, "libarchive ").unwrap_or_else(|| version.clone());
            let rar5 = (major(&libarchive)?, minor(&libarchive)) >= (3, 6);
            Some((version, rar5))
        }
    }
}

/// The version token right after `marker` (ASCII case-insensitive) at the
/// start of a line.
fn token_after(text: &str, marker: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim_start();
        let head = line.get(..marker.len())?;
        if !head.eq_ignore_ascii_case(marker) {
            return None;
        }
        let word = line[marker.len()..].split_whitespace().next()?;
        is_version(word).then(|| word.to_string())
    })
}

fn is_version(w: &str) -> bool {
    w.starts_with(|c: char| c.is_ascii_digit())
        && w.contains('.')
        && w.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn major(v: &str) -> Option<u32> {
    v.split('.').next()?.parse().ok()
}

fn minor(v: &str) -> u32 {
    v.split('.')
        .nth(1)
        .and_then(|m| m.parse().ok())
        .unwrap_or(0)
}

/// What one tool run produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolRun {
    /// Everything printed on stdout, unless a sink took it.
    pub stdout: Vec<u8>,
    /// The first [`STDERR_CAP`] bytes of stderr.
    pub stderr: Vec<u8>,
    /// Exit code; `None` when a signal ended it.
    pub status: Option<i32>,
}

/// Runs `t` with `args`, then `--`, then `archive`, under the hygiene of
/// spec §5.2. stdout goes to `sink` when given (streamed reads), else is
/// returned; either way more than `out_cap` bytes → [`ArchiveError::TooLarge`].
/// A run past `timeout` is killed with its whole process tree →
/// [`ArchiveError::ToolFailed`] with no status. A non-zero exit is
/// returned in [`ToolRun::status`], for the caller to judge.
pub(crate) fn run_tool(
    t: &FoundTool,
    args: &[&OsStr],
    archive: &Path,
    out_cap: u64,
    timeout: Duration,
    sink: Option<&mut dyn Write>,
) -> Result<ToolRun, ArchiveError> {
    // The tool runs in an empty directory: a relative path would not reach
    // the archive.
    let archive = std::path::absolute(archive)?;
    let mut all: Vec<&OsStr> = args.to_vec();
    all.push(OsStr::new("--"));
    all.push(archive.as_os_str());
    spawn_bounded(t, &all, out_cap, timeout, sink)
}

fn refused(t: &FoundTool, why: &str) -> ArchiveError {
    ArchiveError::Unsupported {
        format: format!("{:?} at {} ({why})", t.tool, t.path.display()),
        need: Some(t.tool),
    }
}

/// Why `path` is not a tool we run, or `None` when it is: an absolute path
/// to a regular executable file (`.exe` on Windows; `.bat`, `.cmd`, `.ps1`
/// would go through `cmd.exe` and its quoting).
fn executable_problem(path: &Path) -> Option<&'static str> {
    if !path.is_absolute() {
        return Some("not an absolute path");
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return Some("not found");
    };
    if !meta.is_file() {
        return Some("not a regular file");
    }
    platform_executable_problem(path, &meta)
}

#[cfg(unix)]
fn platform_executable_problem(_path: &Path, meta: &std::fs::Metadata) -> Option<&'static str> {
    use std::os::unix::fs::PermissionsExt;
    (meta.permissions().mode() & 0o111 == 0).then_some("not executable")
}

#[cfg(windows)]
fn platform_executable_problem(path: &Path, _meta: &std::fs::Metadata) -> Option<&'static str> {
    let exe = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    (!exe).then_some("not an .exe")
}

/// A fresh empty directory for one run, removed on drop.
struct RunDir(PathBuf);

impl RunDir {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!("coverly-tool-{}-{n}-{nanos}", std::process::id()));
            match std::fs::create_dir(&dir) {
                // The resolved path, so `HOME` names the same directory the
                // tool sees as its working directory (`/private/var` on macOS).
                Ok(()) => return Ok(Self(dir.canonicalize().unwrap_or(dir))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for RunDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The environment a tool gets: nothing of ours (`RAR`, `TAR_READER_OPTIONS`
/// and tool config dirs would inject switches), only a fixed `PATH`, a
/// UTF-8 locale and a home that is the empty run directory.
fn tool_env(cmd: &mut Command, home: &Path) {
    cmd.env_clear();
    cmd.env("HOME", home);
    platform_env(cmd, home);
}

#[cfg(target_os = "macos")]
fn platform_env(cmd: &mut Command, _home: &Path) {
    cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    // macOS has no `C.UTF-8` before 13; `en_US.UTF-8` always exists.
    cmd.env("LANG", "en_US.UTF-8");
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_env(cmd: &mut Command, _home: &Path) {
    cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    cmd.env("LC_ALL", "C.UTF-8");
}

#[cfg(windows)]
fn platform_env(cmd: &mut Command, home: &Path) {
    // Windows programs need `SystemRoot` to start at all.
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let path = std::env::join_paths([root.join("System32"), root.clone()]).unwrap_or_default();
    cmd.env("SystemRoot", &root);
    cmd.env("PATH", path);
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("USERPROFILE", home);
    cmd.env("TEMP", home);
    cmd.env("TMP", home);
}

/// Puts the child in its own process group, so a timeout kills every
/// process it started.
#[cfg(unix)]
fn isolate(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` is async-signal-safe and touches no memory of ours.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Starts the child suspended: [`Tree::attach`] puts it in a Job object
/// before it runs a single instruction, so nothing it starts can be
/// created outside the job, then resumes it.
#[cfg(windows)]
fn isolate(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
    cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
}

/// The process tree of one run: the Unix process group led by the child,
/// or a Windows Job object with `KILL_ON_JOB_CLOSE`.
#[cfg(unix)]
struct Tree;

#[cfg(unix)]
impl Tree {
    #[allow(clippy::unnecessary_wraps)] // the Windows twin can fail
    fn attach(_child: &Child) -> io::Result<Self> {
        Ok(Self)
    }

    /// Kills the group. Only called while the child is not yet reaped, so
    /// its pid is still the group's id and cannot have been reused.
    fn kill(&self, child: &mut Child) {
        if let Ok(pid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: plain syscall on a group we created.
            unsafe {
                libc::killpg(pid, libc::SIGKILL);
            }
        }
        let _ = child.kill();
    }
}

#[cfg(windows)]
struct Tree(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Tree {
    /// Puts the suspended child in a new Job object, then resumes it. Any
    /// failure fails the run: a tool we could not contain does not run.
    fn attach(child: &Child) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: Win32 calls with valid arguments; the handle is closed on
        // any failure and otherwise owned by `Tree`.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(std::mem::size_of_val(&info)).unwrap_or(u32::MAX);
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                size,
            );
            if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle().cast()) == 0 {
                let e = io::Error::last_os_error();
                CloseHandle(job);
                return Err(e);
            }
            let tree = Self(job);
            resume_threads(child.id())?;
            Ok(tree)
        }
    }

    fn kill(&self, child: &mut Child) {
        // SAFETY: `self.0` is a live Job object handle owned by `self`.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
        let _ = child.kill();
    }
}

#[cfg(windows)]
impl Drop for Tree {
    fn drop(&mut self) {
        // SAFETY: closing our own handle; `KILL_ON_JOB_CLOSE` ends any
        // process still in the job.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// Resumes the threads of the suspended process `pid` (`std` closes the
/// primary thread handle, so they are found through a thread snapshot).
#[cfg(windows)]
fn resume_threads(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    // SAFETY: Win32 calls with valid arguments; every handle opened here is
    // closed here.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = u32::try_from(std::mem::size_of::<THREADENTRY32>()).unwrap_or(u32::MAX);
        let mut resumed = 0u32;
        let mut more = Thread32First(snap, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    if ResumeThread(thread) != u32::MAX {
                        resumed += 1;
                    }
                    CloseHandle(thread);
                }
            }
            more = Thread32Next(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        if resumed == 0 {
            return Err(io::Error::other("could not resume the tool"));
        }
        Ok(())
    }
}

/// How long a run that ended (exited or killed) still waits for the last
/// stderr bytes, when a process that escaped the tree holds the pipe.
const STDERR_GRACE: Duration = Duration::from_millis(200);
/// Chunks in flight between the stdout reader thread and the run.
const STDOUT_QUEUE: usize = 4;

/// Spawns `t` with exactly `args` and waits for it under the hygiene of
/// [`run_tool`] (which adds the `--` and the archive).
///
/// The pipes are read on their own detached threads and the run waits for
/// them only until `timeout`: a process that left the tree (a daemon that
/// called `setsid`) and keeps a pipe open cannot hold the run past it.
fn spawn_bounded(
    t: &FoundTool,
    args: &[&OsStr],
    out_cap: u64,
    timeout: Duration,
    sink: Option<&mut dyn Write>,
) -> Result<ToolRun, ArchiveError> {
    if let Some(why) = executable_problem(&t.path) {
        return Err(refused(t, why));
    }
    let dir = RunDir::new()?;
    let mut cmd = Command::new(&t.path);
    cmd.args(args)
        .current_dir(&dir.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    tool_env(&mut cmd, &dir.0);
    isolate(&mut cmd);

    let deadline = Instant::now() + timeout;
    let mut child = cmd.spawn()?;
    let tree = match Tree::attach(&child) {
        Ok(tree) => tree,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }
    };
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        tree.kill(&mut child);
        let _ = child.wait();
        return Err(ArchiveError::Io(io::Error::other("tool pipes missing")));
    };

    let (out_tx, out_rx) = mpsc::sync_channel(STDOUT_QUEUE);
    thread::spawn(move || pump(stdout, &out_tx));
    let err_kept = Arc::new(Mutex::new(Vec::new()));
    let (err_tx, err_rx) = mpsc::sync_channel::<()>(1);
    {
        let kept = Arc::clone(&err_kept);
        thread::spawn(move || {
            drain_capped(stderr, STDERR_CAP, &kept);
            let _ = err_tx.send(());
        });
    }

    let mut out = Capped::new(sink, out_cap);
    let mut timed_out = false;
    let out_result = loop {
        let Some(left) = remaining(deadline) else {
            timed_out = true;
            break Ok(());
        };
        match out_rx.recv_timeout(left) {
            Ok(Ok(chunk)) => {
                if let Err(e) = out.take(&chunk) {
                    break Err(e);
                }
            }
            Ok(Err(e)) => break Err(ArchiveError::Io(e)),
            Err(RecvTimeoutError::Timeout) => {
                timed_out = true;
                break Ok(());
            }
            // The reader saw EOF.
            Err(RecvTimeoutError::Disconnected) => break Ok(()),
        }
    };
    // The reader thread ends at its next chunk (or when the pipe closes).
    drop(out_rx);

    // Kill before reaping, so the group id is still ours.
    let mut killed = timed_out || out_result.is_err();
    if killed {
        tree.kill(&mut child);
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if killed => break child.wait().unwrap_or_else(|_| killed_status()),
            Ok(None) => {
                if remaining(deadline).is_none() {
                    timed_out = true;
                    killed = true;
                    tree.kill(&mut child);
                } else {
                    thread::sleep(REAP_POLL);
                }
            }
            Err(_) => {
                killed = true;
                tree.kill(&mut child);
            }
        }
    };

    let wait = remaining(deadline).map_or(STDERR_GRACE, |d| d.max(STDERR_GRACE));
    let _ = err_rx.recv_timeout(wait);
    let err = err_kept
        .lock()
        .map_or_else(|e| e.into_inner().clone(), |g| g.clone());

    if timed_out {
        return Err(ArchiveError::ToolFailed {
            tool: t.tool,
            status: None,
        });
    }
    out_result?;
    Ok(ToolRun {
        stdout: out.buf,
        stderr: err,
        status: status.code(),
    })
}

/// Time left before `deadline`, `None` once it has passed.
fn remaining(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
}

/// Reads `from` in chunks into `to` until EOF (the channel then closes),
/// an error (sent), or the run stops listening.
fn pump(mut from: impl Read, to: &SyncSender<io::Result<Vec<u8>>>) {
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let msg = match from.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => Ok(chunk[..n].to_vec()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => Err(e),
        };
        let stop = msg.is_err();
        if to.send(msg).is_err() || stop {
            return;
        }
    }
}

#[cfg(unix)]
fn killed_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(libc::SIGKILL)
}

#[cfg(windows)]
fn killed_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

/// stdout of one run: into `sink` (or `buf`); more than `cap` bytes in
/// all → [`ArchiveError::TooLarge`].
struct Capped<'a> {
    sink: Option<&'a mut dyn Write>,
    buf: Vec<u8>,
    total: u64,
    cap: u64,
}

impl<'a> Capped<'a> {
    fn new(sink: Option<&'a mut dyn Write>, cap: u64) -> Self {
        Self {
            sink,
            buf: Vec::new(),
            total: 0,
            cap,
        }
    }

    fn take(&mut self, chunk: &[u8]) -> Result<(), ArchiveError> {
        self.total = self.total.saturating_add(chunk.len() as u64);
        if self.total > self.cap {
            return Err(ArchiveError::TooLarge {
                declared: self.total,
            });
        }
        match self.sink.as_deref_mut() {
            Some(w) => w.write_all(chunk)?,
            None => self.buf.extend_from_slice(chunk),
        }
        Ok(())
    }
}

/// Reads `from` to EOF, keeping the first `cap` bytes in `kept` as they
/// arrive (a run that stops waiting still has what came so far).
fn drain_capped(mut from: impl Read, cap: usize, kept: &Mutex<Vec<u8>>) {
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match from.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => {
                let mut kept = kept.lock().unwrap_or_else(|e| e.into_inner());
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..n.min(room)]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// For the integration tests through `archive::testing`.
#[cfg(any(test, feature = "test-util"))]
pub(crate) fn run_tool_for_tests(
    t: &FoundTool,
    args: &[&OsStr],
    archive: &Path,
    out_cap: u64,
    timeout: Duration,
) -> Result<ToolRun, ArchiveError> {
    run_tool(t, args, archive, out_cap, timeout, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banners_parse() {
        type Case<'a> = (Tool, &'a str, Option<(&'a str, bool)>);
        let cases: &[Case<'_>] = &[
            (
                Tool::SevenZip,
                "\n7-Zip (z) 24.08 (arm64) : Copyright (c) 1999-2024 Igor Pavlov : 2024-08-11\n",
                Some(("24.08", true)),
            ),
            (
                Tool::SevenZip,
                "7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21",
                Some(("16.02", true)),
            ),
            (
                Tool::SevenZip,
                "7-Zip 9.20  Copyright",
                Some(("9.20", false)),
            ),
            (
                Tool::Unrar,
                "\nUNRAR 7.01 freeware      Copyright (c) 1993-2024 Alexander Roshal\n",
                Some(("7.01", true)),
            ),
            (
                Tool::Unrar,
                "UNRAR 4.20 beta 3 freeware",
                Some(("4.20", false)),
            ),
            (
                Tool::Bsdtar,
                "bsdtar 3.5.3 - libarchive 3.5.3 zlib/1.2.12",
                Some(("3.5.3", false)),
            ),
            (
                Tool::Bsdtar,
                "bsdtar 3.7.4 - libarchive 3.7.4 zlib/1.2.12",
                Some(("3.7.4", true)),
            ),
            (Tool::Unrar, "RAR 7.01 trial", None),
            (Tool::SevenZip, "p7zip Version", None),
            (Tool::Bsdtar, "tar (GNU tar) 1.35", None),
        ];
        for (tool, text, want) in cases {
            let got = parse_banner(*tool, text);
            let want = want.map(|(v, r)| (v.to_string(), r));
            assert_eq!(got, want, "{tool:?}: {text:?}");
        }
    }

    #[test]
    fn capped_stops_past_the_cap() {
        let data = vec![7u8; 1000];
        let mut over = Capped::new(None, 999);
        over.take(&data[..500]).expect("under");
        let err = over.take(&data[500..]).expect_err("cap");
        assert!(matches!(err, ArchiveError::TooLarge { declared: 1000 }));
        let mut exact = Capped::new(None, 1000);
        exact.take(&data).expect("exact cap");
        assert_eq!(exact.buf.len(), 1000);
    }

    #[test]
    fn capped_streams_into_a_sink() {
        let mut sink = Vec::new();
        let mut c = Capped::new(Some(&mut sink), 300);
        c.take(&[9u8; 300]).expect("ok");
        assert!(c.buf.is_empty());
        drop(c);
        assert_eq!(sink.len(), 300);
    }

    #[test]
    fn drain_keeps_only_the_cap() {
        let data = vec![1u8; 200_000];
        let kept = Mutex::new(Vec::new());
        drain_capped(data.as_slice(), STDERR_CAP, &kept);
        assert_eq!(kept.into_inner().expect("lock").len(), STDERR_CAP);
    }

    #[test]
    fn run_dirs_are_fresh_and_removed() {
        let a = RunDir::new().expect("a");
        let b = RunDir::new().expect("b");
        assert_ne!(a.0, b.0);
        assert_eq!(std::fs::read_dir(&a.0).expect("read").count(), 0);
        let path = a.0.clone();
        drop(a);
        assert!(!path.exists());
    }
}
