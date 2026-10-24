//! External archive tools: what no pure-Rust crate opens (RAR, and 7z
//! coders `sevenz-rust2` lacks) goes through a program already on the
//! user's machine, run as a separate process (spec §5.2).
//!
//! This module finds the tools ([`discover`]) and runs them with strict
//! process hygiene ([`run_tool`]): only an absolute regular executable, no
//! shell, `--` before the archive, a fresh empty working directory, a
//! cleared environment, stdin closed, both pipes drained, output capped,
//! and a timeout that kills the whole process tree. [`ToolReader`] lists
//! an archive through 7-Zip or unrar with strict parsers that a hostile
//! entry name cannot forge (S7); reads by name or stream (S8) build on it.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::budget::{ChargedRead, GateWait};
use super::guard::{guarded, GuardedRead, Poison};
use super::names::name_key;
use super::verify::VerifyingReader;
use super::{
    ArchiveError, ArchiveReader, Budget, Entry, EntryKind, Format, Limits, Method, RelPath, Tool,
};

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

/// The external tools of one job (a plan, an apply), discovered on first
/// need only (an archive that no pure-Rust reader opens), once, and never
/// from a `PATH` entry inside the job's source or target roots (spec §5.2).
#[derive(Debug)]
pub struct ToolsOnDemand {
    search: Search,
    avoid: Vec<PathBuf>,
    found: OnceLock<Vec<FoundTool>>,
}

impl ToolsOnDemand {
    /// This process's [`Search`], avoiding `roots_to_avoid`.
    pub fn new(roots_to_avoid: &[&Path]) -> Self {
        Self::with_search(Search::from_env(), roots_to_avoid)
    }

    /// An explicit [`Search`], avoiding `roots_to_avoid`.
    pub fn with_search(search: Search, roots_to_avoid: &[&Path]) -> Self {
        Self {
            search,
            avoid: roots_to_avoid.iter().map(|p| p.to_path_buf()).collect(),
            found: OnceLock::new(),
        }
    }

    /// The tools, discovered on the first call.
    pub fn get(&self) -> &[FoundTool] {
        self.found.get_or_init(|| {
            let avoid: Vec<&Path> = self.avoid.iter().map(PathBuf::as_path).collect();
            discover_with(&self.search, &avoid)
        })
    }

    /// Whether [`ToolsOnDemand::get`] has run (no process was started
    /// before it).
    pub fn discovered(&self) -> bool {
        self.found.get().is_some()
    }
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

/// One started tool: the child in its own tree, stdout arriving in chunks
/// on a channel, stderr kept as it arrives, and the run directory that is
/// removed when this is dropped.
struct Running {
    child: Child,
    tree: Tree,
    out_rx: Receiver<io::Result<Vec<u8>>>,
    err_kept: Arc<Mutex<Vec<u8>>>,
    err_rx: Receiver<()>,
    _dir: RunDir,
}

/// Spawns `t` with exactly `args` under the hygiene of [`run_tool`]: an
/// absolute regular executable, a fresh empty working directory, a cleared
/// environment, stdin closed, its own process tree, and both pipes read on
/// their own detached threads (a process that left the tree and keeps a
/// pipe open cannot block us).
fn start(t: &FoundTool, args: &[&OsStr]) -> Result<Running, ArchiveError> {
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
    Ok(Running {
        child,
        tree,
        out_rx,
        err_kept,
        err_rx,
        _dir: dir,
    })
}

/// Spawns `t` with exactly `args` and waits for it under the hygiene of
/// [`run_tool`] (which adds the `--` and the archive).
///
/// The run waits for the pipes only until `timeout`: a process that left
/// the tree (a daemon that called `setsid`) and keeps a pipe open cannot
/// hold the run past it.
fn spawn_bounded(
    t: &FoundTool,
    args: &[&OsStr],
    out_cap: u64,
    timeout: Duration,
    sink: Option<&mut dyn Write>,
) -> Result<ToolRun, ArchiveError> {
    let deadline = Instant::now() + timeout;
    let Running {
        mut child,
        tree,
        out_rx,
        err_kept,
        err_rx,
        _dir,
    } = start(t, args)?;

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

// ---------------------------------------------------------------------------
// Strict listing parsers (S7)
// ---------------------------------------------------------------------------

/// Wall time of one listing run (spec §13 S7).
pub const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// The listing switches pinned for one tool, from the oldest version that
/// has all of them (asserted from its banner: an older tool is not used).
struct ListSwitches {
    tool: Tool,
    min: (u32, u32),
    /// The run that prints one record per entry.
    records: &'static [&'static str],
    /// The run that prints the tool's own count and size total.
    summary: &'static [&'static str],
}

const LIST_SWITCHES: &[ListSwitches] = &[
    // `-slt` one `Key = value` record per entry, `-ba` no banner or table,
    // `-sccUTF-8` UTF-8 output, `-p-` a password given, so never a prompt.
    // The summary run is the plain table, whose last line counts.
    ListSwitches {
        tool: Tool::SevenZip,
        min: (16, 2),
        records: &["l", "-slt", "-ba", "-sccUTF-8", "-p-"],
        summary: &["l", "-sccUTF-8", "-p-"],
    },
    // `lt` technical listing, `-cfg-` no `RAR` variable or `.rarrc`, `-p-`
    // never ask a password, `-scur` Unicode redirected output, `-c-` no
    // archive comment (attacker text printed before the records). The
    // summary run is `l`, whose last line has the totals.
    ListSwitches {
        tool: Tool::Unrar,
        min: (5, 0),
        records: &["lt", "-cfg-", "-p-", "-scur", "-c-"],
        summary: &["l", "-cfg-", "-p-", "-scur", "-c-"],
    },
];

fn list_switches(t: &FoundTool) -> Option<&'static ListSwitches> {
    LIST_SWITCHES
        .iter()
        .find(|s| s.tool == t.tool && version_at_least(&t.version, s.min))
}

fn version_at_least(v: &str, min: (u32, u32)) -> bool {
    major(v).is_some_and(|m| (m, minor(v)) >= min)
}

/// The keys one tool prints per record for one archive format, in the
/// tool's order. A record may only hold these, each once, in this order,
/// and must hold `required`.
struct Schema {
    keys: &'static [&'static str],
    required: &'static [&'static str],
}

/// 7-Zip's 7z handler prints only the properties the archive defines, the
/// same set for every record.
const SEVEN_ZIP_7Z: Schema = Schema {
    keys: &[
        "Path",
        "Size",
        "Packed Size",
        "Modified",
        "Created",
        "Accessed",
        "Attributes",
        "CRC",
        "Encrypted",
        "Method",
        "Block",
    ],
    required: &["Path", "Size"],
};

/// 7-Zip's RAR4 handler.
const SEVEN_ZIP_RAR4: Schema = Schema {
    keys: &[
        "Path",
        "Folder",
        "Size",
        "Packed Size",
        "Modified",
        "Created",
        "Accessed",
        "Attributes",
        "Encrypted",
        "Solid",
        "Commented",
        "Split Before",
        "Split After",
        "CRC",
        "Host OS",
        "Method",
        "Version",
        "Volume Index",
    ],
    required: &["Path", "Folder", "Size"],
};

/// 7-Zip's RAR5 handler.
const SEVEN_ZIP_RAR5: Schema = Schema {
    keys: &[
        "Path",
        "Folder",
        "Size",
        "Packed Size",
        "Modified",
        "Created",
        "Accessed",
        "Attributes",
        "Alternate Stream",
        "Encrypted",
        "Solid",
        "Split Before",
        "Split After",
        "CRC",
        "Host OS",
        "Method",
        "Characteristics",
        "Symbolic Link",
        "Hard Link",
        "Copy Link",
        "Volume Index",
    ],
    required: &["Path", "Folder", "Size"],
};

/// unrar `lt` (RAR4 and RAR5), in unrar's `ListFileHeader` order:
/// optional fields are left out, so records differ, but never in order or
/// repetition. A file also needs `Size`. Windows builds print `Modified`,
/// `Created` and `Accessed` where Unix builds print `mtime`, `ctime` and
/// `atime`.
const UNRAR_LT: Schema = Schema {
    keys: &[
        "Name",
        "Type",
        "Target",
        "Size",
        "Packed size",
        "Ratio",
        "RR%",
        "mtime",
        "Modified",
        "ctime",
        "Created",
        "atime",
        "Accessed",
        "Attributes",
        "CRC32",
        "CRC32 MAC",
        "Pack-CRC32",
        "Pack-CRC32 MAC",
        "BLAKE2",
        "BLAKE2 MAC",
        "Pack-BLAKE2",
        "Pack-BLAKE2 MAC",
        "Host OS",
        "Compression",
        "Flags",
        "File version",
        "Unix owner",
    ],
    required: &["Name", "Type"],
};

/// The last line of 7-Zip's table, right before its summary line.
const SEVEN_ZIP_TABLE_END: &str =
    "------------------- ----- ------------ ------------  ------------------------";
/// unrar's `l` table separator, right before its totals line: five dash
/// columns (`----------- ----------  ---------- -----  ----` in unrar 7;
/// the column widths differ between versions).
fn is_unrar_table_end(line: &str) -> bool {
    let cols: Vec<&str> = line.split(' ').filter(|c| !c.is_empty()).collect();
    cols.len() == 5
        && cols.iter().all(|c| c.bytes().all(|b| b == b'-'))
        && cols[0].len() == 11
        && cols[4] == "----"
        && line.starts_with('-')
        && line.ends_with('-')
}

fn forged(detail: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Damaged {
        detail: format!("tool listing: {detail}"),
    }
}

fn unsafe_names() -> ArchiveError {
    ArchiveError::Unsupported {
        format: "unsafe names".into(),
        need: None,
    }
}

/// A tool's stdout as text: UTF-16LE when it starts with a byte-order mark
/// (or holds NUL bytes, which UTF-8 text never does), else strict UTF-8.
/// Anything else rejects the listing.
pub fn decode_tool_text(bytes: &[u8]) -> Result<String, ArchiveError> {
    fn utf16(b: &[u8]) -> Result<String, ArchiveError> {
        let (pairs, odd) = b.as_chunks::<2>();
        if !odd.is_empty() {
            return Err(forged("odd-length UTF-16 output"));
        }
        let units: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16(&units).map_err(|_| forged("output is not valid UTF-16"))
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest);
    }
    if bytes.contains(&0) {
        return utf16(bytes);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| forged("output is not valid UTF-8"))
}

/// The output's lines. `\r\n` endings are taken off only when every line
/// has one (Windows tools), so a `\r` inside a name stays and is refused
/// as a control character.
fn tool_lines(text: &str) -> Vec<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    let crlf = text.contains('\n')
        && text.matches('\n').count() == text.matches("\r\n").count()
        && text.ends_with('\r');
    text.split('\n')
        .map(|l| {
            if crlf {
                l.strip_suffix('\r').unwrap_or(l)
            } else {
                l
            }
        })
        .collect()
}

/// Groups of consecutive non-empty lines.
fn blocks<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    lines
        .split(|l| l.is_empty())
        .filter(|b| !b.is_empty())
        .map(<[&str]>::to_vec)
        .collect()
}

/// One record's fields, as positions in its [`Schema`].
struct Fields<'a> {
    schema: &'static Schema,
    items: Vec<(usize, &'a str)>,
}

impl<'a> Fields<'a> {
    fn new(schema: &'static Schema) -> Self {
        Self {
            schema,
            items: Vec::new(),
        }
    }

    /// Adds one field: a known key, after every key already there.
    fn push(&mut self, key: &str, value: &'a str) -> Result<(), ArchiveError> {
        let Some(pos) = self.schema.keys.iter().position(|k| *k == key) else {
            return Err(forged(format!("unknown key {key:?}")));
        };
        if self.items.last().is_some_and(|(last, _)| *last >= pos) {
            return Err(forged(format!("key {key:?} repeated or out of order")));
        }
        self.items.push((pos, value));
        Ok(())
    }

    fn finish(self) -> Result<Self, ArchiveError> {
        for key in self.schema.required {
            if self.get(key).is_none() {
                return Err(forged(format!("record without {key:?}")));
            }
        }
        Ok(self)
    }

    fn get(&self, key: &str) -> Option<&'a str> {
        let pos = self.schema.keys.iter().position(|k| *k == key)?;
        self.items.iter().find(|(p, _)| *p == pos).map(|(_, v)| *v)
    }

    /// The value when the key is there and not empty.
    fn value(&self, key: &str) -> Option<&'a str> {
        self.get(key).filter(|v| !v.is_empty())
    }

    fn sequence(&self) -> impl Iterator<Item = usize> + '_ {
        self.items.iter().map(|(p, _)| *p)
    }
}

/// A 7-Zip `-slt` line: `Key = value` (value may be empty).
fn seven_zip_fields<'a>(
    block: &[&'a str],
    schema: &'static Schema,
) -> Result<Fields<'a>, ArchiveError> {
    let mut f = Fields::new(schema);
    for line in block {
        let Some((key, value)) = line.split_once(" = ") else {
            return Err(forged("a line that is no `Key = value`"));
        };
        f.push(key, value)?;
    }
    f.finish()
}

/// An unrar `lt` line: the key right-aligned in 12 columns, `: `, the value.
fn unrar_field(line: &str) -> Option<(&str, &str)> {
    let colon = line.find(": ")?;
    let key = line[..colon].trim_start_matches(' ');
    let pad = colon - key.len();
    (!key.is_empty() && pad == 12usize.saturating_sub(key.len())).then(|| (key, &line[colon + 2..]))
}

fn unrar_fields<'a>(block: &[&'a str]) -> Result<Fields<'a>, ArchiveError> {
    let mut f = Fields::new(&UNRAR_LT);
    for line in block {
        let Some((key, value)) = unrar_field(line) else {
            return Err(forged("a line that is no unrar field"));
        };
        f.push(key, value)?;
    }
    f.finish()
}

/// A decimal number, digits only.
fn number(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn optional_number(v: Option<&str>, key: &str) -> Result<Option<u64>, ArchiveError> {
    match v {
        None => Ok(None),
        Some(v) => number(v)
            .map(Some)
            .ok_or_else(|| forged(format!("{key} is not a number"))),
    }
}

fn crc(v: Option<&str>) -> Result<Option<u32>, ArchiveError> {
    match v {
        None => Ok(None),
        Some(v) if v.len() == 8 && v.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(u32::from_str_radix(v, 16).ok())
        }
        Some(_) => Err(forged("CRC is not 8 hex digits")),
    }
}

/// The validated name, or the whole listing is refused (S7 rule 4): a
/// control character (U+0000–U+001F, U+007F–U+009F) or a name `RelPath`
/// refuses is never partly trusted.
fn tool_name(raw: &str) -> Result<RelPath, ArchiveError> {
    if raw
        .chars()
        .any(|c| matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
    {
        return Err(unsafe_names());
    }
    RelPath::parse(raw).map_err(|_| unsafe_names())
}

/// A dictionary or model size as 7-Zip prints it: a power of two (`26`)
/// or a number with a unit (`1536k`, `3m`, `1g`, `65536b`).
fn seven_zip_size(s: &str) -> Option<u32> {
    let (digits, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => s.split_at(i),
        None => (s, ""),
    };
    let n = number(digits)?;
    let bytes = match unit {
        "" if n < 32 => 1u64 << n,
        "" | "b" | "B" => n,
        "k" | "K" => n.checked_mul(1 << 10)?,
        "m" | "M" => n.checked_mul(1 << 20)?,
        "g" | "G" => n.checked_mul(1 << 30)?,
        _ => return None,
    };
    Some(u32::try_from(bytes).unwrap_or(u32::MAX))
}

/// The compressor of a 7-Zip `Method` (`LZMA:26`, `BCJ LZMA2:24`,
/// `PPMD:o6:mem24`, `Copy`). Informational: the tool decodes, not us.
fn seven_zip_method(text: &str) -> Method {
    let other = || Method::Other(text.to_string());
    for coder in text.split_whitespace() {
        let mut parts = coder.split(':');
        let name = parts.next().unwrap_or_default().to_ascii_uppercase();
        let param = parts.next();
        let method = match name.as_str() {
            "COPY" => Method::Stored,
            "DEFLATE" => Method::Deflate,
            "DEFLATE64" => Method::Deflate64,
            "BZIP2" => Method::Bzip2,
            "LZMA" => match param.and_then(seven_zip_size) {
                Some(dict) => Method::Lzma { dict },
                None => return other(),
            },
            "LZMA2" => match param.and_then(seven_zip_size) {
                Some(dict) => Method::Lzma2 { dict },
                None => return other(),
            },
            "PPMD" => {
                let mut order = None;
                let mut mem = None;
                for p in param.into_iter().chain(parts) {
                    if let Some(o) = p.strip_prefix('o') {
                        order = number(o).and_then(|o| u8::try_from(o).ok());
                    } else if let Some(m) = p.strip_prefix("mem") {
                        mem = seven_zip_size(m);
                    }
                }
                match (order, mem) {
                    (Some(order), Some(mem)) => Method::Ppmd { order, mem },
                    _ => return other(),
                }
            }
            _ => continue,
        };
        return method;
    }
    if text.is_empty() {
        Method::Stored
    } else {
        other()
    }
}

/// The letters 7-Zip prints for Windows attribute bits 0–15
/// (`ConvertWinAttribToString`): `_` is the Unix-mode flag (0x8000).
const SEVEN_ZIP_ATTR_LETTERS: &str = "RHS8DAdNTsLCOnE_";

/// What a 7-Zip `Attributes` value says: `D`, `AC`, `A_ -rw-r--r--`,
/// `D drwxr-xr-x`, `A lrwxr-xr-x`. A reparse point (`L`, 0x400) or a Unix
/// link is a symlink (never followed, spec §13.1); then a Unix type wins
/// over the letters.
fn seven_zip_attr_kind(attrs: &str) -> Option<EntryKind> {
    let mut letters: Option<&str> = None;
    let mut unix = None;
    for tok in attrs.split_whitespace() {
        let mut chars = tok.chars();
        let first = chars.next()?;
        if tok.len() == 10 && chars.all(|c| "rwxsStT-".contains(c)) {
            unix = Some(match first {
                '-' => EntryKind::File,
                'd' => EntryKind::Dir,
                'l' => EntryKind::Symlink,
                _ => EntryKind::Other,
            });
        } else if tok.chars().all(|c| SEVEN_ZIP_ATTR_LETTERS.contains(c)) {
            letters = Some(tok);
        }
    }
    if letters.is_some_and(|l| l.contains('L')) {
        return Some(EntryKind::Symlink);
    }
    if unix.is_some() {
        return unix;
    }
    letters.map(|l| {
        if l.contains('D') {
            EntryKind::Dir
        } else if l.contains('d') {
            EntryKind::Other
        } else {
            EntryKind::File
        }
    })
}

fn split_part() -> ArchiveError {
    ArchiveError::Unsupported {
        format: "RAR volume part".into(),
        need: None,
    }
}

fn seven_zip_entry(index: usize, f: &Fields<'_>) -> Result<Entry, ArchiveError> {
    let raw = f.get("Path").unwrap_or_default();
    let name = tool_name(raw)?;
    if f.get("Split Before") == Some("+") || f.get("Split After") == Some("+") {
        return Err(split_part());
    }
    let mut kind = f
        .get("Attributes")
        .and_then(seven_zip_attr_kind)
        .unwrap_or(EntryKind::File);
    if f.get("Folder") == Some("+") {
        kind = EntryKind::Dir;
    } else if f.value("Symbolic Link").is_some() {
        kind = EntryKind::Symlink;
    } else if f.value("Hard Link").is_some() {
        kind = EntryKind::Hardlink;
    } else if f.value("Copy Link").is_some() || f.get("Alternate Stream") == Some("+") {
        kind = EntryKind::Other;
    }
    let size = match (f.value("Size"), kind) {
        (None, EntryKind::Dir) => 0,
        (None, _) => return Err(forged("an entry without a size")),
        (Some(v), _) => number(v).ok_or_else(|| forged("Size is not a number"))?,
    };
    let block = optional_number(f.value("Block"), "Block")?;
    let solid_block = match block {
        Some(b) => Some(u32::try_from(b).map_err(|_| forged("Block out of range"))?),
        None => (f.get("Solid") == Some("+")).then_some(0),
    };
    Ok(Entry {
        index,
        name,
        raw_name: raw.as_bytes().to_vec(),
        kind,
        size,
        packed: optional_number(f.value("Packed Size"), "Packed Size")?,
        crc32: crc(f.value("CRC"))?,
        encrypted: f.get("Encrypted") == Some("+"),
        method: seven_zip_method(f.get("Method").unwrap_or_default()),
        solid_block,
        // 7-Zip prints local wall time without a zone: not an instant.
        mtime: None,
        unsafe_name: None,
    })
}

fn unrar_entry(index: usize, f: &Fields<'_>) -> Result<Entry, ArchiveError> {
    let raw = f.get("Name").unwrap_or_default();
    let name = tool_name(raw)?;
    let kind = match f.get("Type").unwrap_or_default() {
        "File" => EntryKind::File,
        "Directory" => EntryKind::Dir,
        "Unix symbolic link" | "Windows symbolic link" | "NTFS junction point" => {
            EntryKind::Symlink
        }
        "Hard link" => EntryKind::Hardlink,
        "File reference" | "NTFS alternate data stream" | "Service" => EntryKind::Other,
        other => return Err(forged(format!("unknown entry type {other:?}"))),
    };
    if f.get("Pack-CRC32").is_some() || f.get("Pack-BLAKE2").is_some() {
        return Err(split_part());
    }
    let size = match (f.value("Size"), kind) {
        (None, EntryKind::File) => return Err(forged("a file without a size")),
        (None, _) => 0,
        (Some(v), _) => number(v).ok_or_else(|| forged("Size is not a number"))?,
    };
    let flags: Vec<&str> = f
        .get("Flags")
        .map(|v| v.split_whitespace().collect())
        .unwrap_or_default();
    let compression = f.value("Compression");
    Ok(Entry {
        index,
        name,
        raw_name: raw.as_bytes().to_vec(),
        kind,
        size,
        packed: optional_number(f.value("Packed size"), "Packed size")?,
        // `CRC32 MAC` is keyed with the password: not the data's CRC.
        crc32: crc(f.value("CRC32"))?,
        encrypted: flags.contains(&"encrypted"),
        method: match compression {
            Some(c) if c.split_whitespace().any(|w| w == "-m0") => Method::Stored,
            Some(c) => Method::Other(c.to_string()),
            None => Method::Stored,
        },
        solid_block: flags.contains(&"solid").then_some(0),
        mtime: None,
        unsafe_name: None,
    })
}

/// Entry count and declared sizes against `limits` (spec §4.4).
fn check_limits(entries: &[Entry], limits: &Limits) -> Result<(), ArchiveError> {
    let mut total: u64 = 0;
    for e in entries {
        if e.size > limits.max_entry_size {
            return Err(ArchiveError::TooLarge { declared: e.size });
        }
        total = total.saturating_add(e.size);
    }
    if total > limits.max_total_size {
        return Err(ArchiveError::TooLarge { declared: total });
    }
    Ok(())
}

fn check_count(n: usize, limits: &Limits) -> Result<(), ArchiveError> {
    let count = n as u64;
    if count > limits.max_entries {
        return Err(ArchiveError::TooManyEntries { count });
    }
    Ok(())
}

/// Parses `7zz l -slt -ba` output: every record has the same key sequence
/// (7-Zip prints one property list per archive), drawn from the format's
/// [`Schema`].
fn parse_seven_zip(
    text: &str,
    schema: &'static Schema,
    limits: &Limits,
) -> Result<Vec<Entry>, ArchiveError> {
    let lines = tool_lines(text);
    let blocks = blocks(&lines);
    check_count(blocks.len(), limits)?;
    let mut entries = Vec::with_capacity(blocks.len());
    let mut first: Option<Vec<usize>> = None;
    for (index, block) in blocks.iter().enumerate() {
        let f = seven_zip_fields(block, schema)?;
        let seq: Vec<usize> = f.sequence().collect();
        match &first {
            None => first = Some(seq),
            Some(want) if *want != seq => {
                return Err(forged("records with different key sets"));
            }
            Some(_) => {}
        }
        entries.push(seven_zip_entry(index, &f)?);
    }
    check_limits(&entries, limits)?;
    Ok(entries)
}

/// Parses `unrar lt` output: the banner, `Archive:` and `Details:` lines,
/// then records that each start with `Name`.
fn parse_unrar(text: &str, limits: &Limits) -> Result<Vec<Entry>, ArchiveError> {
    let lines = tool_lines(text);
    let blocks = blocks(&lines);
    let is_record = |b: &[&str]| {
        b.first()
            .and_then(|l| unrar_field(l))
            .is_some_and(|(k, _)| k == "Name")
    };
    let start = blocks
        .iter()
        .position(|b| is_record(b))
        .unwrap_or(blocks.len());
    for line in blocks[..start].iter().flatten() {
        let known = ["UNRAR ", "RAR ", "Archive: ", "Details: "]
            .iter()
            .any(|p| line.starts_with(p));
        if !known {
            return Err(forged("an unexpected line before the records"));
        }
    }
    let records = &blocks[start..];
    check_count(records.len(), limits)?;
    let mut entries = Vec::with_capacity(records.len());
    for (index, block) in records.iter().enumerate() {
        if !is_record(block) {
            return Err(forged("a record that does not start with Name"));
        }
        entries.push(unrar_entry(index, &unrar_fields(block)?)?);
    }
    check_limits(&entries, limits)?;
    Ok(entries)
}

/// The tool's own count and size total.
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    /// 7-Zip: files (links included) and folders apart; unrar: every
    /// record in one count.
    files: u64,
    folders: Option<u64>,
    size: u64,
}

/// The line after the last `end` separator, with only blank lines (and
/// `allowed` lines) after it.
fn summary_line<'a>(
    lines: &[&'a str],
    end: impl Fn(&str) -> bool,
    allowed: impl Fn(&str) -> Result<bool, ArchiveError>,
) -> Result<&'a str, ArchiveError> {
    let sep = lines
        .iter()
        .rposition(|l| end(l))
        .ok_or_else(|| forged("no summary"))?;
    let line = lines
        .get(sep + 1)
        .copied()
        .filter(|l| !l.trim().is_empty())
        .ok_or_else(|| forged("no summary"))?;
    for rest in &lines[sep + 2..] {
        if !rest.is_empty() && !allowed(rest)? {
            return Err(forged("an unexpected line after the summary"));
        }
    }
    Ok(line)
}

/// 7-Zip's last table line: `[date time] size [packed]  N files[, M folders]`.
fn parse_seven_zip_summary(text: &str) -> Result<Summary, ArchiveError> {
    let lines = tool_lines(text);
    let line = summary_line(
        &lines,
        |l| l == SEVEN_ZIP_TABLE_END,
        |l| {
            if let Some(n) = l.strip_prefix("Warnings: ") {
                return Ok(number(n).is_some());
            }
            if l.starts_with("Errors: ") {
                return Err(forged("7-Zip reported errors"));
            }
            Ok(false)
        },
    )?;
    let toks: Vec<&str> = line.split_whitespace().collect();
    let bad = || forged("an unreadable 7-Zip summary");
    let (files, folders, head) = match toks.as_slice() {
        [head @ .., n, "files,", m, "folders"] => (*n, *m, head),
        [head @ .., n, "files"] => (*n, "0", head),
        [head @ .., m, "folders"] => ("0", *m, head),
        _ => return Err(bad()),
    };
    let files = number(files).ok_or_else(bad)?;
    let folders = number(folders).ok_or_else(bad)?;
    let mut head = head;
    if let [date, time, rest @ ..] = head {
        let is_date = date.len() == 10 && date.bytes().all(|b| b.is_ascii_digit() || b == b'-');
        if is_date {
            if !time.contains(':') {
                return Err(bad());
            }
            head = rest;
        }
    }
    let size = match head {
        [size] | [size, _] => number(size).ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    if let [_, packed] = head {
        number(packed).ok_or_else(bad)?;
    }
    Ok(Summary {
        files,
        folders: Some(folders),
        size,
    })
}

/// unrar's `l` totals line: `size  count`.
fn parse_unrar_summary(text: &str) -> Result<Summary, ArchiveError> {
    let lines = tool_lines(text);
    let line = summary_line(&lines, is_unrar_table_end, |_| Ok(false))?;
    let bad = || forged("an unreadable unrar totals line");
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        [size, count] => Ok(Summary {
            files: number(count).ok_or_else(bad)?,
            folders: None,
            size: number(size).ok_or_else(bad)?,
        }),
        _ => Err(bad()),
    }
}

/// The parsed records against the tool's own count and size total
/// (S7 rule 3): a record forged by a name is one too many.
fn check_summary(entries: &[Entry], s: &Summary) -> Result<(), ArchiveError> {
    let dirs = entries.iter().filter(|e| e.kind == EntryKind::Dir).count() as u64;
    let all = entries.len() as u64;
    let counts_ok = match s.folders {
        Some(folders) => s.files == all - dirs && folders == dirs,
        None => s.files == all,
    };
    if !counts_ok {
        return Err(forged(format!(
            "{all} records but the tool counts {} files and {:?} folders",
            s.files, s.folders
        )));
    }
    let size = entries
        .iter()
        .try_fold(0u64, |acc, e| acc.checked_add(e.size))
        .ok_or_else(|| forged("sizes overflow"))?;
    if size != s.size {
        return Err(forged(format!(
            "sizes add up to {size} but the tool says {}",
            s.size
        )));
    }
    Ok(())
}

/// Size and modification time: equal before the first listing run and
/// after the second, or the two runs may have seen different archives.
fn stamp(path: &Path) -> Result<(u64, Option<SystemTime>), ArchiveError> {
    let meta = std::fs::metadata(path)?;
    Ok((meta.len(), meta.modified().ok()))
}

/// One listing run as text; a non-zero exit is the tool refusing the
/// archive (encrypted when it says so).
fn run_listing(
    t: &FoundTool,
    args: &[&str],
    archive: &Path,
    limits: &Limits,
) -> Result<String, ArchiveError> {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    let run = run_tool(t, &args, archive, limits.max_header, LIST_TIMEOUT, None)?;
    if run.status != Some(0) {
        let err = String::from_utf8_lossy(&run.stderr).to_ascii_lowercase();
        // unrar exit 11: wrong password (RARX_BADPWD).
        let locked = err.contains("wrong password")
            || err.contains("encrypted")
            || (t.tool == Tool::Unrar && run.status == Some(11));
        return Err(if locked {
            ArchiveError::Encrypted
        } else {
            ArchiveError::Damaged {
                detail: format!(
                    "{:?} could not list the archive (status {:?})",
                    t.tool, run.status
                ),
            }
        });
    }
    decode_tool_text(&run.stdout)
}

/// An archive listed by an external tool (RAR, or a 7z with a coder
/// `sevenz-rust2` lacks; spec §5.2). The listing is strict (S7): forced
/// machine output, an exact key set per record, the record count and size
/// total checked against the tool's own summary from a second run on the
/// same unchanged file, and any unsafe name refusing the whole listing.
///
/// The tool and version are kept ([`ToolReader::tool`]): the plan records
/// them (`ExtractSource::Tool`) and apply uses the same ones.
#[derive(Debug)]
pub struct ToolReader {
    tool: FoundTool,
    path: PathBuf,
    format: Format,
    entries: Vec<Entry>,
    ambiguity: Ambiguity,
    poison: Poison,
}

impl ToolReader {
    /// Lists `path` (a `format` archive, by magic) with the first fitting
    /// tool of `tools`: for RAR unrar or 7-Zip (one that opens RAR5 when
    /// `v5`), for 7z only 7-Zip, and only at a version with the pinned
    /// switches. bsdtar is never text-parsed (its reads go through the pax
    /// transcode). No fitting tool, or `limits.external_tools` off →
    /// `Unsupported { need }`.
    pub fn open(
        tools: &[FoundTool],
        path: &Path,
        format: Format,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let (label, need, schema_7zip) = match format {
            Format::Rar { v5: true } => ("RAR", Tool::Unrar, &SEVEN_ZIP_RAR5),
            Format::Rar { v5: false } => ("RAR", Tool::Unrar, &SEVEN_ZIP_RAR4),
            Format::SevenZip => ("7z", Tool::SevenZip, &SEVEN_ZIP_7Z),
            other => {
                return Err(ArchiveError::Unsupported {
                    format: format!("{other:?} through an external tool"),
                    need: None,
                })
            }
        };
        let missing = || ArchiveError::Unsupported {
            format: label.into(),
            need: Some(need),
        };
        if !limits.external_tools {
            return Err(missing());
        }
        let fits = |t: &FoundTool| match format {
            Format::Rar { v5 } => matches!(t.tool, Tool::Unrar | Tool::SevenZip) && (!v5 || t.rar5),
            _ => t.tool == Tool::SevenZip,
        };
        let (tool, switches) = tools
            .iter()
            .filter(|t| fits(t))
            .find_map(|t| list_switches(t).map(|s| (t, s)))
            .ok_or_else(missing)?;

        let before = stamp(path)?;
        let records = run_listing(tool, switches.records, path, limits)?;
        let entries = guarded(|| match tool.tool {
            Tool::Unrar => parse_unrar(&records, limits),
            _ => parse_seven_zip(&records, schema_7zip, limits),
        })?;
        if entries.is_empty() {
            return Err(forged("no entries"));
        }
        let summary_text = run_listing(tool, switches.summary, path, limits)?;
        let after = stamp(path)?;
        if before != after {
            return Err(ArchiveError::Damaged {
                detail: "the archive changed while it was listed".into(),
            });
        }
        let summary = guarded(|| match tool.tool {
            Tool::Unrar => parse_unrar_summary(&summary_text),
            _ => parse_seven_zip_summary(&summary_text),
        })?;
        check_summary(&entries, &summary)?;
        let ambiguity = Ambiguity::new(&entries);
        Ok(Self {
            tool: tool.clone(),
            path: path.to_path_buf(),
            format,
            entries,
            ambiguity,
            poison: Poison::new(),
        })
    }

    /// The tool and version that listed the archive.
    pub fn tool(&self) -> &FoundTool {
        &self.tool
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Entry `index`, read by its name when that name selects exactly it,
    /// else sliced out of the stream of every file (S8).
    fn open_inner<'a>(
        &self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let entry = self.entries.get(index).ok_or_else(|| no_entry(index))?;
        readable(entry)?;
        let switches = read_switches(self.tool.tool)?;
        let archive = std::path::absolute(&self.path)?;
        let wait = budget.gate_wait();
        if !self.ambiguity.at(index) {
            let name = OsStr::new(entry.name.as_str());
            let stream = spawn_stream(
                &self.tool,
                switches.by_name,
                &[archive.as_os_str(), name],
                wait,
            )?;
            let charged = ChargedRead::new(stream, budget);
            return Ok(Box::new(VerifyingReader::new(
                charged,
                entry.size,
                entry.crc32,
            )));
        }
        stream_refusal(&self.entries)?;
        let (slices, positions) = file_slices(&self.entries);
        let target = positions[index].ok_or_else(|| no_entry(index))?;
        let stream = spawn_stream(&self.tool, switches.stream, &[archive.as_os_str()], wait)?;
        let mut slicer = Slicer::new(ChargedRead::new(stream, budget), slices);
        for _ in 0..target {
            slicer.skip()?;
        }
        let slice = SliceToEnd {
            slicer,
            pos: target,
            done: false,
        };
        Ok(Box::new(VerifyingReader::new(
            slice,
            entry.size,
            entry.crc32,
        )))
    }

    fn stream_inner(
        &self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let mut order = wanted.to_vec();
        order.sort_unstable();
        order.dedup();
        for &index in &order {
            readable(self.entries.get(index).ok_or_else(|| no_entry(index))?)?;
        }
        if order.is_empty() {
            return Ok(());
        }
        let ambiguous = order.iter().any(|&i| self.ambiguity.at(i));
        let refusal = stream_refusal(&self.entries);
        // One pass when a name cannot be used, or when several entries are
        // wanted and the stream is exact (a solid archive decodes once).
        if !(ambiguous || (order.len() > 1 && refusal.is_ok())) {
            for index in order {
                let mut data = self.open_inner(index, budget)?;
                sink(&self.entries[index], &mut data)?;
                drain(&mut data)?;
            }
            return Ok(());
        }
        refusal?;
        let switches = read_switches(self.tool.tool)?;
        let archive = std::path::absolute(&self.path)?;
        let stream = spawn_stream(
            &self.tool,
            switches.stream,
            &[archive.as_os_str()],
            budget.gate_wait(),
        )?;
        let (slices, positions) = file_slices(&self.entries);
        let mut slicer = Slicer::new(ChargedRead::new(stream, budget), slices);
        let mut want = order.into_iter().peekable();
        for (index, pos) in positions.iter().enumerate() {
            let Some(pos) = *pos else { continue };
            if want.peek() == Some(&index) {
                want.next();
                let entry = &self.entries[index];
                let mut part = SliceOf {
                    slicer: &mut slicer,
                    pos,
                };
                sink(entry, &mut part)?;
                if slicer.pos == pos {
                    slicer.skip()?;
                }
            } else {
                slicer.skip()?;
            }
        }
        slicer.finish()
    }
}

impl ArchiveReader for ToolReader {
    fn format(&self) -> Format {
        self.format
    }

    fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Reads by unambiguous name, or by a verified slice of the stream of
    /// every file (S8). The tool's exit status is checked before the read
    /// reports EOF.
    fn open_entry<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let poison = self.poison.clone();
        let this = &*self;
        let stream = poison.guard(|| this.open_inner(index, budget))?;
        Ok(Box::new(GuardedRead::new(stream, poison)))
    }

    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let poison = self.poison.clone();
        poison.guard(|| self.stream_inner(wanted, budget, sink))
    }
}

// ---------------------------------------------------------------------------
// Reads by name, by stream and through bsdtar's pax transcode (S8)
// ---------------------------------------------------------------------------

/// How long a read waits for more output from a tool that is still
/// running before it gives up on it (a plan budget's deadline and a cancel
/// flag stop it sooner).
pub const READ_STALL_TIMEOUT: Duration = Duration::from_secs(120);
/// How often a waiting read checks the budget's cancel flag and deadline.
const READ_POLL: Duration = Duration::from_millis(50);

/// The switches of one tool's reads, before `--` (spec §13 S8). `-spd`
/// turns off 7-Zip's wildcard matching and `-r-` its recursion; unrar has
/// no such switch, so a name it might match as a pattern is never given.
/// `-p-` on both: never a password prompt (unrar asks on the console, not
/// on the closed stdin).
struct ReadSwitches {
    by_name: &'static [&'static str],
    stream: &'static [&'static str],
}

fn read_switches(tool: Tool) -> Result<ReadSwitches, ArchiveError> {
    match tool {
        Tool::SevenZip => Ok(ReadSwitches {
            by_name: &["e", "-so", "-spd", "-r-", "-p-"],
            stream: &["e", "-so", "-spd", "-p-"],
        }),
        Tool::Unrar => Ok(ReadSwitches {
            by_name: &["p", "-inul", "-cfg-", "-p-"],
            stream: &["p", "-inul", "-cfg-", "-p-"],
        }),
        Tool::Bsdtar => Err(ArchiveError::Unsupported {
            format: "bsdtar reads go through the pax transcode".into(),
            need: None,
        }),
    }
}

/// The switches of bsdtar's pax transcode (`bsdtar -c --format=pax -f -
/// @<archive>`): libarchive reads the archive and writes it again as a
/// pax tar on stdout, where each entry is framed by its own header.
const BSDTAR_TRANSCODE: &[&str] = &["-c", "--format=pax", "-f", "-"];

/// Characters unrar or 7-Zip read as a pattern, a list file or a switch.
const PATTERN_CHARS: &[char] = &['*', '?', '[', ']', '{', '}', '\\', '!'];

/// Whether `name`, given to the tool, could select anything but the one
/// entry of `listing` it names (spec §13 S8): a pattern character, a
/// leading `-` or `@`, an (NFC, case-insensitive) twin, another entry
/// under `name + '/'`, or (unrar matches a bare name in every folder)
/// another entry ending in `'/' + name`.
pub fn is_ambiguous(name: &str, listing: &[Entry]) -> bool {
    Ambiguity::new(listing).name(name)
}

/// The listing's name keys, for [`is_ambiguous`] checks in linear time.
#[derive(Debug, Default)]
struct Ambiguity {
    /// How many entries have each key.
    keys: std::collections::HashMap<String, usize>,
    /// Every folder prefix of every key (`a`, `a/b` of `a/b/c`).
    prefixes: std::collections::HashSet<String>,
    /// Every proper suffix after a `/` (`b/c`, `c` of `a/b/c`).
    suffixes: std::collections::HashSet<String>,
    /// Per entry: its stored name differs from the validated one, so the
    /// tool would not know the name we give it.
    renamed: Vec<bool>,
    /// Per entry: the answer, computed once.
    ambiguous: Vec<bool>,
}

impl Ambiguity {
    fn new(listing: &[Entry]) -> Self {
        let mut a = Self::default();
        for e in listing {
            let key = name_key(e.name.as_str());
            for (at, _) in key.match_indices('/') {
                a.prefixes.insert(key[..at].to_string());
                a.suffixes.insert(key[at + 1..].to_string());
            }
            *a.keys.entry(key).or_default() += 1;
            a.renamed.push(e.raw_name != e.name.as_str().as_bytes());
        }
        a.ambiguous = listing
            .iter()
            .zip(&a.renamed)
            .map(|(e, &renamed)| renamed || a.name(e.name.as_str()))
            .collect();
        a
    }

    fn name(&self, name: &str) -> bool {
        if name.contains(PATTERN_CHARS) || name.starts_with(['-', '@']) {
            return true;
        }
        let key = name_key(name);
        self.keys.get(&key).is_some_and(|&n| n > 1)
            || self.prefixes.contains(&key)
            || self.suffixes.contains(&key)
    }

    fn at(&self, index: usize) -> bool {
        self.ambiguous.get(index).copied().unwrap_or(true)
    }
}

/// Stream mode needs every file's output to be exactly its listed size:
/// a link, an alternate stream or an encrypted entry in the listing
/// refuses it (spec §13 S8).
fn stream_refusal(entries: &[Entry]) -> Result<(), ArchiveError> {
    let inexact = entries.iter().any(|e| {
        e.encrypted
            || matches!(
                e.kind,
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other
            )
    });
    if inexact {
        return Err(ArchiveError::Unsupported {
            format: "ambiguous names".into(),
            need: None,
        });
    }
    Ok(())
}

/// The file entries' sizes and CRCs in listing order, and each entry's
/// position among them (`None` for a folder).
fn file_slices(entries: &[Entry]) -> (Vec<Slice>, Vec<Option<usize>>) {
    let mut slices = Vec::new();
    let positions = entries
        .iter()
        .map(|e| {
            (e.kind == EntryKind::File).then(|| {
                slices.push(Slice {
                    size: e.size,
                    crc32: e.crc32,
                });
                slices.len() - 1
            })
        })
        .collect();
    (slices, positions)
}

/// Only safe, readable files are read.
fn readable(entry: &Entry) -> Result<(), ArchiveError> {
    match entry.kind {
        EntryKind::File => {}
        EntryKind::Symlink | EntryKind::Hardlink => {
            return Err(ArchiveError::UnsafeEntry {
                name: entry.name.to_string(),
                why: super::UnsafeWhy::Link,
            })
        }
        EntryKind::Dir | EntryKind::Other => {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("entry {} is not a file", entry.index),
            )))
        }
    }
    if let Some(why) = entry.unsafe_name {
        return Err(ArchiveError::UnsafeEntry {
            name: entry.name.to_string(),
            why,
        });
    }
    if entry.encrypted {
        return Err(ArchiveError::Encrypted);
    }
    Ok(())
}

fn no_entry(index: usize) -> ArchiveError {
    ArchiveError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no entry {index}"),
    ))
}

/// Reads the rest of an entry (checked by its verifier).
fn drain(r: &mut dyn Read) -> Result<(), ArchiveError> {
    io::copy(r, &mut io::sink()).map_err(ArchiveError::from)?;
    Ok(())
}

/// Why a [`ToolStream`] stopped; every later read reports it again.
#[derive(Clone, Copy, Debug)]
enum Fail {
    Tool(Option<i32>),
    Cancelled,
    Time,
    Io(io::ErrorKind),
}

impl Fail {
    fn of(e: &ArchiveError) -> Self {
        match e {
            ArchiveError::ToolFailed { status, .. } => Self::Tool(*status),
            ArchiveError::Cancelled => Self::Cancelled,
            ArchiveError::BudgetSpent { .. } => Self::Time,
            ArchiveError::Io(e) => Self::Io(e.kind()),
            _ => Self::Tool(None),
        }
    }

    fn error(self, tool: Tool) -> ArchiveError {
        match self {
            Self::Tool(status) => ArchiveError::ToolFailed { tool, status },
            Self::Cancelled => ArchiveError::Cancelled,
            Self::Time => ArchiveError::BudgetSpent {
                what: super::BudgetKind::Time,
            },
            Self::Io(kind) => ArchiveError::Io(kind.into()),
        }
    }
}

/// A tool's stdout, read as it arrives. EOF is reported only once the
/// tool has exited with status 0; any other exit is
/// [`ArchiveError::ToolFailed`] (spec §9.1: the exit status must be 0
/// before anything is committed). A cancel flag or a plan deadline stops
/// a waiting read, and so does [`READ_STALL_TIMEOUT`] without output.
/// Dropping the stream before its end kills the tool's whole tree.
pub(crate) struct ToolStream {
    tool: Tool,
    run: Option<Running>,
    chunk: Vec<u8>,
    at: usize,
    wait: GateWait,
    stall: Duration,
    last: Instant,
    failed: Option<Fail>,
}

/// Starts `t` with `switches`, `--`, then `operands` (the archive first),
/// streaming its stdout.
fn spawn_stream(
    t: &FoundTool,
    switches: &[&str],
    operands: &[&OsStr],
    wait: GateWait,
) -> Result<ToolStream, ArchiveError> {
    wait.check()?;
    let mut args: Vec<&OsStr> = switches.iter().map(OsStr::new).collect();
    args.push(OsStr::new("--"));
    args.extend_from_slice(operands);
    let run = start(t, &args)?;
    Ok(ToolStream {
        tool: t.tool,
        run: Some(run),
        chunk: Vec::new(),
        at: 0,
        wait,
        stall: READ_STALL_TIMEOUT,
        last: Instant::now(),
        failed: None,
    })
}

impl ToolStream {
    /// Kills the tool's tree and reaps it; every later read fails with
    /// `fail`.
    fn abort(&mut self, fail: Fail) -> ArchiveError {
        if let Some(mut run) = self.run.take() {
            run.tree.kill(&mut run.child);
            let _ = run.child.wait();
        }
        self.failed = Some(fail);
        fail.error(self.tool)
    }

    /// stdout has closed: waits for the exit (killing a tool that keeps
    /// running past the stall time) and judges its status.
    fn reap(&mut self) -> Result<(), ArchiveError> {
        let Some(run) = self.run.as_mut() else {
            return Ok(());
        };
        let deadline = Instant::now() + self.stall;
        let status = loop {
            match run.child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {
                    if let Err(e) = self.wait.check() {
                        return Err(self.abort(Fail::of(&e)));
                    }
                    if Instant::now() >= deadline {
                        return Err(self.abort(Fail::Tool(None)));
                    }
                    thread::sleep(REAP_POLL);
                }
                Err(_) => return Err(self.abort(Fail::Tool(None))),
            }
        };
        // Reaped: the tree must not be signalled any more.
        self.run = None;
        if status == Some(0) {
            Ok(())
        } else {
            self.failed = Some(Fail::Tool(status));
            Err(Fail::Tool(status).error(self.tool))
        }
    }
}

impl Read for ToolStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.at < self.chunk.len() {
                let n = buf.len().min(self.chunk.len() - self.at);
                buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            if let Some(fail) = self.failed {
                return Err(fail.error(self.tool).into());
            }
            if buf.is_empty() {
                return Ok(0);
            }
            let Some(run) = self.run.as_mut() else {
                return Ok(0);
            };
            match run.out_rx.recv_timeout(READ_POLL) {
                Ok(Ok(chunk)) => {
                    self.chunk = chunk;
                    self.at = 0;
                    self.last = Instant::now();
                }
                Ok(Err(e)) => {
                    self.abort(Fail::Io(e.kind()));
                    return Err(e);
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Err(e) = self.wait.check() {
                        return Err(self.abort(Fail::of(&e)).into());
                    }
                    if self.last.elapsed() >= self.stall {
                        return Err(self.abort(Fail::Tool(None)).into());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.reap()?;
                    return Ok(0);
                }
            }
        }
    }
}

impl Drop for ToolStream {
    fn drop(&mut self) {
        if let Some(mut run) = self.run.take() {
            run.tree.kill(&mut run.child);
            let _ = run.child.wait();
        }
    }
}

/// One file's place in the stream of every file.
#[derive(Clone, Copy, Debug)]
struct Slice {
    size: u64,
    crc32: Option<u32>,
}

/// The stream of every file (`unrar p`, `7zz e -so` with no name), cut
/// into the listed file sizes in listing order. Every slice is checked
/// (size, and CRC32 when listed) whether it is wanted or skipped, and the
/// stream must end right after the last one; any mismatch is `Damaged`.
struct Slicer<R> {
    src: R,
    slices: Vec<Slice>,
    /// The current slice.
    pos: usize,
    /// Its bytes not read yet.
    left: u64,
    hasher: crc32fast::Hasher,
    failed: Option<&'static str>,
}

impl<R: Read> Slicer<R> {
    fn new(src: R, slices: Vec<Slice>) -> Self {
        let left = slices.first().map_or(0, |s| s.size);
        Self {
            src,
            slices,
            pos: 0,
            left,
            hasher: crc32fast::Hasher::new(),
            failed: None,
        }
    }

    fn fail(&mut self, detail: &'static str) -> io::Error {
        self.failed = Some(detail);
        damaged_io(detail)
    }

    /// Reads from the current slice; `Ok(0)` once it is whole and checked,
    /// and the next slice is then current.
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(detail) = self.failed {
            return Err(damaged_io(detail));
        }
        let Some(slice) = self.slices.get(self.pos).copied() else {
            return Ok(0);
        };
        if self.left == 0 {
            let crc = std::mem::take(&mut self.hasher).finalize();
            if slice.crc32.is_some_and(|want| want != crc) {
                return Err(self.fail(super::verify::CRC_MISMATCH));
            }
            self.pos += 1;
            self.left = self.slices.get(self.pos).map_or(0, |s| s.size);
            return Ok(0);
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let want = usize::try_from(self.left).map_or(buf.len(), |l| l.min(buf.len()));
        let n = match self.src.read(&mut buf[..want]) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
            Err(e) => {
                // A tool or budget error stays what it is.
                return Err(e);
            }
        };
        if n == 0 {
            return Err(self.fail(super::verify::SIZE_MISMATCH));
        }
        self.left -= n as u64;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    /// Reads and checks the rest of the current slice.
    fn skip(&mut self) -> Result<(), ArchiveError> {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match self.read_slice(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Checks every slice left, then that the stream ends there (reading
    /// on to EOF makes a [`ToolStream`] judge the exit status).
    fn finish(&mut self) -> Result<(), ArchiveError> {
        while self.pos < self.slices.len() {
            self.skip()?;
        }
        let mut one = [0u8; 1];
        loop {
            match self.src.read(&mut one) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    return Err(ArchiveError::Damaged {
                        detail: "the tool printed more than the listed sizes".into(),
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}

fn damaged_io(detail: &str) -> io::Error {
    ArchiveError::Damaged {
        detail: detail.to_owned(),
    }
    .into()
}

/// Slice `pos` of a [`Slicer`], for a sink; it never reads into the next.
struct SliceOf<'s, R> {
    slicer: &'s mut Slicer<R>,
    pos: usize,
}

impl<R: Read> Read for SliceOf<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.slicer.pos != self.pos {
            return Ok(0);
        }
        self.slicer.read_slice(buf)
    }
}

/// Slice `pos`, then every later slice checked and the stream's end
/// reached, before EOF is reported (one wanted entry of an ambiguous
/// name: the tool's exit status is part of the read).
struct SliceToEnd<R> {
    slicer: Slicer<R>,
    pos: usize,
    done: bool,
}

impl<R: Read> Read for SliceToEnd<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done {
            return Ok(0);
        }
        if self.slicer.pos == self.pos {
            let n = self.slicer.read_slice(buf)?;
            if n > 0 || buf.is_empty() {
                return Ok(n);
            }
        }
        self.slicer.finish()?;
        self.done = true;
        Ok(0)
    }
}

/// bsdtar's pax transcode of `archive` as a stream.
fn transcode(t: &FoundTool, archive: &Path, wait: GateWait) -> Result<ToolStream, ArchiveError> {
    let archive = std::path::absolute(archive)?;
    // `@` makes bsdtar read the archive's entries, also after `--`; the
    // path is absolute, so it never starts with `-`.
    let mut operand = OsString::from("@");
    operand.push(archive.as_os_str());
    spawn_stream(t, BSDTAR_TRANSCODE, &[operand.as_os_str()], wait)
}

/// An archive read through bsdtar (spec §5.2): libarchive opens it and
/// writes it again as a pax tar on stdout, which the tar walker reads
/// (S6 trailer cap, S8: names and sizes framed by tar headers, nothing is
/// text-parsed or sliced). The listing is one transcode under the plan
/// listing budget; a larger archive gets an incomplete listing (moved or
/// copied as it is, never extracted). Every read is a fresh transcode,
/// walked against the listing, and reports EOF only after bsdtar exited
/// with status 0. tar stores no CRC: reads are verified by size.
#[derive(Debug)]
pub struct PaxReader {
    tool: FoundTool,
    path: PathBuf,
    format: Format,
    listing: super::tar::StreamListing,
    limits: Limits,
    poison: Poison,
}

impl PaxReader {
    /// Lists `path` (a `format` archive by magic: RAR or 7z) with the
    /// first bsdtar of `tools` that opens it (libarchive ≥ 3.6 for RAR5).
    /// None, or `limits.external_tools` off → `Unsupported { need: Bsdtar }`.
    pub fn open(
        tools: &[FoundTool],
        path: &Path,
        format: Format,
        limits: &Limits,
    ) -> Result<Self, ArchiveError> {
        let label = match format {
            Format::Rar { .. } => "RAR",
            Format::SevenZip => "7z",
            other => {
                return Err(ArchiveError::Unsupported {
                    format: format!("{other:?} through bsdtar"),
                    need: None,
                })
            }
        };
        let missing = || ArchiveError::Unsupported {
            format: label.into(),
            need: Some(Tool::Bsdtar),
        };
        if !limits.external_tools {
            return Err(missing());
        }
        let v5 = matches!(format, Format::Rar { v5: true });
        let tool = tools
            .iter()
            .find(|t| t.tool == Tool::Bsdtar && (!v5 || t.rar5))
            .ok_or_else(missing)?;
        let poison = Poison::new();
        let mut budget = Budget::plan_listing(limits);
        let listing = poison.guard(|| {
            let stream = transcode(tool, path, budget.gate_wait())?;
            super::tar::list_stream(stream, limits, &mut budget)
        })?;
        Ok(Self {
            tool: tool.clone(),
            path: path.to_path_buf(),
            format,
            listing,
            limits: limits.clone(),
            poison,
        })
    }

    /// The bsdtar and version that listed the archive.
    pub fn tool(&self) -> &FoundTool {
        &self.tool
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ArchiveReader for PaxReader {
    fn format(&self) -> Format {
        self.format
    }

    fn entries(&self) -> &[Entry] {
        &self.listing.entries
    }

    fn listing_complete(&self) -> bool {
        self.listing.complete
    }

    fn open_entry<'a>(
        &'a mut self,
        index: usize,
        budget: &'a mut Budget,
    ) -> Result<Box<dyn Read + 'a>, ArchiveError> {
        let poison = self.poison.clone();
        let Self {
            tool,
            path,
            listing,
            limits,
            ..
        } = &*self;
        let stream = poison.guard(|| {
            let src = transcode(tool, path, budget.gate_wait())?;
            super::tar::open_stream_entry(src, listing, index, limits, budget)
        })?;
        Ok(Box::new(GuardedRead::new(stream, poison)))
    }

    fn stream_entries(
        &mut self,
        wanted: &[usize],
        budget: &mut Budget,
        sink: &mut dyn FnMut(&Entry, &mut dyn Read) -> Result<(), ArchiveError>,
    ) -> Result<(), ArchiveError> {
        let poison = self.poison.clone();
        poison.guard(|| {
            if wanted.is_empty() {
                return Ok(());
            }
            let src = transcode(&self.tool, &self.path, budget.gate_wait())?;
            super::tar::stream_stream_entries(
                src,
                &self.listing,
                wanted,
                &self.limits,
                budget,
                sink,
            )
        })
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
    fn seven_zip_methods_parse() {
        let cases = [
            ("LZMA:26", Method::Lzma { dict: 1 << 26 }),
            ("LZMA2:12", Method::Lzma2 { dict: 1 << 12 }),
            ("BCJ LZMA2:1536k", Method::Lzma2 { dict: 1536 << 10 }),
            ("LZMA2:24 BCJ", Method::Lzma2 { dict: 1 << 24 }),
            (
                "PPMD:o6:mem24",
                Method::Ppmd {
                    order: 6,
                    mem: 1 << 24,
                },
            ),
            (
                "PPMD:o32:mem192m",
                Method::Ppmd {
                    order: 32,
                    mem: 192 << 20,
                },
            ),
            ("Copy", Method::Stored),
            ("", Method::Stored),
            ("Deflate", Method::Deflate),
            ("BZip2", Method::Bzip2),
            ("LZMA:9z", Method::Other("LZMA:9z".into())),
            ("v5:m3:22", Method::Other("v5:m3:22".into())),
        ];
        for (text, want) in cases {
            assert_eq!(seven_zip_method(text), want, "{text}");
        }
    }

    #[test]
    fn seven_zip_attributes_kind() {
        let cases = [
            ("D", Some(EntryKind::Dir)),
            ("AC", Some(EntryKind::File)),
            ("A -rw-r--r--", Some(EntryKind::File)),
            ("D drwxr-xr-x", Some(EntryKind::Dir)),
            ("A lrwxr-xr-x", Some(EntryKind::Symlink)),
            ("A crw-rw-rw-", Some(EntryKind::Other)),
            // 7-Zip's `RHS8DAdNTsLCOnE_` letters: `_` the Unix flag,
            // `n` not content-indexed, `L` a reparse point (spec §13.1).
            ("D_ drwxr-xr-x", Some(EntryKind::Dir)),
            ("A_ -rw-r--r--", Some(EntryKind::File)),
            ("Dn", Some(EntryKind::Dir)),
            ("An", Some(EntryKind::File)),
            ("AL", Some(EntryKind::Symlink)),
            ("DL", Some(EntryKind::Symlink)),
            ("AL -rw-r--r--", Some(EntryKind::Symlink)),
            ("d", Some(EntryKind::Other)),
            ("N", Some(EntryKind::File)),
            ("", None),
        ];
        for (attrs, want) in cases {
            assert_eq!(seven_zip_attr_kind(attrs), want, "{attrs:?}");
        }
    }

    #[test]
    fn seven_zip_summary_shapes() {
        let table = |line: &str| format!("x\n{SEVEN_ZIP_TABLE_END}\n{line}\n\nWarnings: 1\n");
        let got = parse_seven_zip_summary(&table(
            "2018-11-10 15:45:22          405749493    355281155  12 files, 1 folders",
        ))
        .expect("full");
        assert_eq!(
            got,
            Summary {
                files: 12,
                folders: Some(1),
                size: 405_749_493
            }
        );
        // No date (no times stored), no packed size (solid RAR).
        let got = parse_seven_zip_summary(&table("                           13  1 files"))
            .expect("bare");
        assert_eq!((got.files, got.folders, got.size), (1, Some(0), 13));
        for bad in [
            "2018-11-10 15:45:22  12 files",
            "2018-11-10 15:45:22  x  1 files",
            "1 2 3 1 files",
            "13  1 files, 2 alt streams",
        ] {
            assert!(parse_seven_zip_summary(&table(bad)).is_err(), "{bad:?}");
        }
        let trailing = format!("{SEVEN_ZIP_TABLE_END}\n 13  1 files\nPath = evil\n");
        assert!(parse_seven_zip_summary(&trailing).is_err());
    }

    #[test]
    fn crlf_is_taken_off_only_when_every_line_has_it() {
        assert_eq!(tool_lines("a\r\nb\r\n"), vec!["a", "b"]);
        // A name ending in `\r` in LF output keeps it (and is refused).
        assert_eq!(
            tool_lines("Path = a\r\nSize = 1\n"),
            vec!["Path = a\r", "Size = 1"]
        );
    }

    #[test]
    fn unrar_fields_need_exact_alignment() {
        assert_eq!(unrar_field("        Name: a: b"), Some(("Name", "a: b")));
        assert_eq!(unrar_field(" Packed size: 3"), Some(("Packed size", "3")));
        assert_eq!(
            unrar_field("Pack-CRC32 MAC: 1"),
            Some(("Pack-CRC32 MAC", "1"))
        );
        assert_eq!(unrar_field("       Name: a"), None);
        assert_eq!(unrar_field("Name: a"), None);
        assert_eq!(unrar_field("        Name a"), None);
    }

    #[test]
    fn pinned_versions() {
        let t = |tool, version: &str| FoundTool {
            tool,
            path: PathBuf::from("/x"),
            version: version.into(),
            rar5: true,
        };
        assert!(list_switches(&t(Tool::SevenZip, "16.02")).is_some());
        assert!(list_switches(&t(Tool::SevenZip, "26.05")).is_some());
        assert!(list_switches(&t(Tool::SevenZip, "15.14")).is_none());
        assert!(list_switches(&t(Tool::Unrar, "5.00")).is_some());
        assert!(list_switches(&t(Tool::Unrar, "4.20")).is_none());
        assert!(list_switches(&t(Tool::Bsdtar, "3.7.4")).is_none());
    }

    fn listed(names: &[&str]) -> Vec<Entry> {
        names
            .iter()
            .enumerate()
            .map(|(index, raw)| Entry {
                index,
                name: RelPath::parse(raw).expect("name"),
                raw_name: raw.as_bytes().to_vec(),
                kind: EntryKind::File,
                size: 1,
                packed: None,
                crc32: None,
                encrypted: false,
                method: Method::Stored,
                solid_block: None,
                mtime: None,
                unsafe_name: None,
            })
            .collect()
    }

    /// Spec §13 "Encrypted entries": never prompt. unrar asks on the
    /// console, not on stdin, so a closed stdin does not stop it (Windows
    /// has no `setsid` to take the terminal away).
    #[test]
    fn reads_never_prompt_for_a_password() {
        for tool in [Tool::SevenZip, Tool::Unrar] {
            let s = read_switches(tool).expect("switches");
            for args in [s.by_name, s.stream] {
                assert!(args.contains(&"-p-"), "{tool:?} {args:?}");
            }
        }
    }

    #[test]
    fn ambiguity_rules() {
        let entries = listed(&[
            "Game [!].gba",
            "Game {EU}.gba",
            "Hit!.bin",
            "-x.bin",
            "@list.bin",
            "Disc",
            "Disc/Track 01.bin",
            "Pokémon.gba",
            "POKÉMON.GBA",
            "sub/plain.bin",
            "plain.bin",
            "solo.bin",
            "dir\\back.bin",
            "Dup.bin",
            "Dup.bin",
        ]);
        let a = Ambiguity::new(&entries);
        let got: Vec<bool> = (0..entries.len()).map(|i| a.at(i)).collect();
        assert_eq!(
            got,
            vec![
                true,  // `[` `]`
                true,  // `{` `}`
                true,  // `!`
                true,  // leading `-`
                true,  // leading `@`
                true,  // `Disc/` holds another entry
                false, // the track itself
                true,  // NFC case-insensitive twin
                true,  // its twin
                false, // `sub/plain.bin` is only itself
                true,  // a bare `plain.bin` also matches `sub/plain.bin`
                false, true, // stored `\` differs from the validated `/` name
                true, // exact duplicate
                true,
            ]
        );
        assert!(a.at(99), "an unknown index is never trusted");
        assert!(is_ambiguous("a\\b", &[]));
        assert!(is_ambiguous("a*b", &[]));
        assert!(!is_ambiguous("solo.bin", &entries));
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
