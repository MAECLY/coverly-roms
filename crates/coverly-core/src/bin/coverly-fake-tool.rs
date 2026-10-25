//! Scripted stand-in for `7zz`, `unrar` and `bsdtar` in tests (feature
//! `test-util`, plan AP-5). It reads its [`FakeScript`] from the JSON file
//! beside its own executable (the environment is cleared by `run_tool`) or
//! from `COVERLY_FAKE_TOOL_SCRIPT`, and prints what the matching case says.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

use coverly_core::archive::testing::{
    fake_script_path, FakeScript, FAKE_GRANDCHILD_ARG, FAKE_REPORTED_ENV,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(FAKE_GRANDCHILD_ARG) {
        grandchild(&args[1..]);
        return ExitCode::SUCCESS;
    }
    let exe = std::env::current_exe().expect("current exe");
    let script_path = std::env::var_os("COVERLY_FAKE_TOOL_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| fake_script_path(&exe));
    let script: FakeScript = match std::fs::read(&script_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).expect("fake script json"),
        Err(_) => FakeScript::default(),
    };
    let Some(case) = script.case_for(&args) else {
        return ExitCode::SUCCESS;
    };

    if case.grandchild_sleep_ms > 0 {
        let marker = case.grandchild_marker.clone().unwrap_or_default();
        // Inherits stdout/stderr, so it holds the pipes like a real helper.
        let mut cmd = Command::new(&exe);
        if case.grandchild_stderr_only {
            cmd.stdout(Stdio::null());
        }
        let _ = cmd
            .arg(FAKE_GRANDCHILD_ARG)
            .arg(case.grandchild_sleep_ms.to_string())
            .arg(marker)
            .arg(if case.grandchild_detach {
                "detach"
            } else {
                "stay"
            })
            .stdin(Stdio::null())
            .spawn();
    }

    let mut err = std::io::stderr().lock();
    let _ = err.write_all(case.stderr.as_bytes());
    fill(&mut err, b'e', case.stderr_fill);
    let _ = err.flush();
    drop(err);

    let mut out = std::io::stdout().lock();
    let _ = out.write_all(case.stdout.as_bytes());
    if let Some(file) = &case.stdout_file {
        if let Ok(bytes) = std::fs::read(file) {
            let _ = out.write_all(&bytes);
        }
    }
    if case.cat_archive {
        let operand = args.iter().skip_while(|a| *a != "--").nth(1);
        for file in operand.map(|o| archive_matches(o)).unwrap_or_default() {
            if let Ok(bytes) = std::fs::read(file) {
                let _ = out.write_all(&bytes);
            }
        }
    }
    if case.report {
        report(&mut out, &args);
    }
    fill(&mut out, b'o', case.stdout_fill);
    let _ = out.flush();
    drop(out);

    if !case.append_to_archive.is_empty() {
        if let Some(archive) = args.last() {
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(archive) {
                let _ = f.write_all(case.append_to_archive.as_bytes());
            }
        }
    }

    if case.sleep_ms > 0 {
        thread::sleep(Duration::from_millis(case.sleep_ms));
    }
    ExitCode::from(u8::try_from(case.exit).unwrap_or(1))
}

fn fill(w: &mut dyn Write, byte: u8, mut n: u64) {
    let chunk = [byte; 64 * 1024];
    while n > 0 {
        let take = usize::try_from(n.min(chunk.len() as u64)).unwrap_or(chunk.len());
        if w.write_all(&chunk[..take]).is_err() {
            return;
        }
        n -= take as u64;
    }
}

fn report(out: &mut dyn Write, args: &[String]) {
    let _ = writeln!(
        out,
        "args={}",
        serde_json::to_string(args).unwrap_or_default()
    );
    let cwd = std::env::current_dir().unwrap_or_default();
    let entries = std::fs::read_dir(&cwd).map_or(usize::MAX, Iterator::count);
    let _ = writeln!(out, "cwd={}", cwd.display());
    let _ = writeln!(out, "cwd_entries={entries}");
    for key in FAKE_REPORTED_ENV {
        match std::env::var_os(key) {
            Some(v) => {
                let _ = writeln!(out, "env.{key}={}", v.to_string_lossy());
            }
            None => {
                let _ = writeln!(out, "env.{key}=<unset>");
            }
        }
    }
}

/// The files an archive operand reaches when its file name is read as a
/// pattern: `*` any run, `?` one character, `[...]` one of a set (a `[`
/// without its `]` is literal). Relative to the working directory.
fn archive_matches(operand: &str) -> Vec<PathBuf> {
    let path = Path::new(operand);
    let Some(pattern) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let pattern: Vec<char> = pattern.chars().collect();
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name().to_str().is_some_and(|n| {
                let name: Vec<char> = n.chars().collect();
                wild_match(&pattern, &name)
            })
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn wild_match(p: &[char], n: &[char]) -> bool {
    match p.first() {
        None => n.is_empty(),
        Some('*') => (0..=n.len()).any(|i| wild_match(&p[1..], &n[i..])),
        Some('?') => !n.is_empty() && wild_match(&p[1..], &n[1..]),
        Some('[') if p[1..].contains(&']') => {
            let end = 1 + p[1..].iter().position(|&c| c == ']').unwrap_or(0);
            n.first().is_some_and(|c| p[1..end].contains(c)) && wild_match(&p[end + 1..], &n[1..])
        }
        Some(&c) => n.first() == Some(&c) && wild_match(&p[1..], &n[1..]),
    }
}

fn grandchild(args: &[String]) {
    if args.get(2).map(String::as_str) == Some("detach") {
        detach();
    }
    let ms = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    thread::sleep(Duration::from_millis(ms));
    if let Some(marker) = args.get(1).filter(|m| !m.is_empty()) {
        let _ = std::fs::write(Path::new(marker), b"survived");
    }
}

/// Leaves the tool's process group, as a daemonising helper would.
#[cfg(unix)]
fn detach() {
    // SAFETY: plain syscall with no arguments.
    unsafe {
        libc::setsid();
    }
}

/// A Job object cannot be left without `BREAKAWAY_OK`, which `run_tool`
/// never grants.
#[cfg(windows)]
fn detach() {}
