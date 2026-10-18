//! The landing page follows the published release on its own:
//!
//! - nothing in `site/` names a version, a release date, a download link or a
//!   file size; those places carry `{{release.*}}` markers inside
//!   `<!-- release:begin -->` blocks, next to a `norelease` block for the time
//!   before the first release;
//! - `scripts/site/apply_release.py` fills them from the GitHub API record of
//!   the latest release into a copy of `site/` and refuses a release that
//!   lacks any download or SHA256SUMS, or, with `--no-release`, keeps only the
//!   "not published yet" state (exercised here where a `python3` exists);
//! - Vercel builds the landing with `scripts/site/vercel_build.sh`
//!   (`vercel.json`), which checks `site/`, runs it and serves that copy, not
//!   `site/`; `.github/workflows/landing-redeploy.yml` calls the deploy hook
//!   when a release is published.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Every text file of a site folder, as (path relative to it, text).
fn site_texts(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| {
                ["html", "txt", "webmanifest", "xml", "json"].contains(&x.to_str().unwrap())
            }) {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    out.sort();
    assert!(out.len() >= 8, "{out:?}");
    out
}

/// (marker label, asset name for version 9.8.7).
const DOWNLOADS: [(&str, &str); 12] = [
    ("macos_universal", "Coverly.ROMs_9.8.7_universal.dmg"),
    ("windows_x64", "Coverly.ROMs_9.8.7_x64-setup.exe"),
    ("windows_x64_msi", "Coverly.ROMs_9.8.7_x64_en-US.msi"),
    ("linux_x64_appimage", "Coverly.ROMs_9.8.7_amd64.AppImage"),
    ("linux_x64_deb", "Coverly.ROMs_9.8.7_amd64.deb"),
    (
        "linux_arm64_appimage",
        "Coverly.ROMs_9.8.7_aarch64.AppImage",
    ),
    ("linux_arm64_deb", "Coverly.ROMs_9.8.7_arm64.deb"),
    ("cli_macos_arm64", "coverly-9.8.7-darwin-aarch64"),
    ("cli_macos_x64", "coverly-9.8.7-darwin-x86_64"),
    ("cli_windows_x64", "coverly-9.8.7-windows-x86_64.exe"),
    ("cli_linux_x64", "coverly-9.8.7-linux-x86_64"),
    ("cli_linux_arm64", "coverly-9.8.7-linux-aarch64"),
];

const BASE: &str = "https://github.com/MAECLY/coverly-roms/releases";

#[test]
fn site_sources_link_no_release_by_hand() {
    for (file, text) in site_texts(&repo().join("site")) {
        for hard in [
            "/releases/download/",
            "/releases/tag/",
            "Coverly.ROMs_",
            "coverly-0.",
            "0.0.1",
            "0.1.0",
        ] {
            assert!(
                !text.contains(hard),
                "site/{file} hardcodes {hard:?}: use a {{{{release.*}}}} marker"
            );
        }
    }
    for file in ["index.html", "en/index.html"] {
        let text = read(&format!("site/{file}"));
        for (label, _) in DOWNLOADS {
            assert!(
                text.contains(&format!("href=\"{{{{release.{label}.url}}}}\"")),
                "{file} {label}"
            );
        }
        for m in [
            "{{release.version}}",
            "{{release.notes_url}}",
            "{{release.sha256sums_url}}",
            "\"softwareVersion\":\"{{release.version}}\"",
            "\"datePublished\":\"{{release.date}}\"",
            "<!-- norelease:begin -->",
        ] {
            assert!(text.contains(m), "{file} lacks {m}");
        }
    }
    assert!(read("site/agents.txt").contains("Release: {{release.notes_url}}"));
}

#[test]
fn vercel_build_fills_the_release_before_publishing() {
    let conf: Value = serde_json::from_str(&read("vercel.json")).expect("vercel.json is JSON");
    assert_eq!(conf["outputDirectory"], "target/site");
    assert_eq!(conf["buildCommand"], "bash scripts/site/vercel_build.sh");

    let sh = read("scripts/site/vercel_build.sh");
    assert!(sh.contains("set -euo pipefail"));
    let check = sh
        .find("python3 -I scripts/site/check_site.py")
        .expect("vercel_build.sh checks the landing");
    let fetch = sh
        .find("https://api.github.com/repos/$REPO/releases/latest")
        .expect("vercel_build.sh asks for the latest release");
    let fill = sh
        .find("python3 -I scripts/site/apply_release.py --release-json")
        .expect("vercel_build.sh fills the release");
    let none = sh.find("--no-release").expect("a not-published-yet state");
    assert!(
        check < fetch && fetch < fill && fill < none,
        "check, fetch, then fill"
    );
    assert!(sh.contains("OUT=\"target/site\""));
    // Only "no release yet" selects the not-published state; any other
    // answer fails the build.
    assert!(sh.contains("  404)\n"));
    assert!(sh.contains("  *)\n"));
    assert!(sh[sh.find("  *)\n").unwrap()..].contains("exit 1"));
    for bypass in ["|| true", "--skip", "--allow"] {
        assert!(!sh.contains(bypass), "{bypass}");
    }
}

#[test]
fn landing_redeploys_when_a_release_is_published() {
    let wf = read(".github/workflows/landing-redeploy.yml");
    let on = wf
        .split_once("\non:\n")
        .unwrap()
        .1
        .split_once("\npermissions:")
        .unwrap()
        .0;
    assert!(on.contains("  release:\n    types: [published]\n"), "{on}");
    assert!(on.contains("  workflow_dispatch:"), "{on}");
    // Least privilege: the hook URL is the only credential.
    assert!(wf.contains("\npermissions: {}\n"));
    assert!(wf.contains("HOOK: ${{ secrets.VERCEL_DEPLOY_HOOK_URL }}"));
    assert!(wf.contains("if [ -z \"${HOOK:-}\" ]; then"));
    assert!(wf.contains("curl -fsS -X POST \"$HOOK\""));
    assert!(!wf.contains("uses:"), "no third-party action");
    assert!(!wf.contains("continue-on-error") && !wf.contains("|| true"));
}

/// Same rule as release_pipeline.rs: never on Windows, only with a python3.
fn python() -> Option<&'static str> {
    if cfg!(windows) {
        return None;
    }
    Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
        .then_some("python3")
}

fn fixture() -> Value {
    serde_json::from_str(&read("scripts/site/fixtures/release.json")).unwrap()
}

fn run(args: &[&str], site: &Path, out: &Path) -> std::process::Output {
    Command::new(python().unwrap())
        .arg("-I")
        .arg(repo().join("scripts/site/apply_release.py"))
        .args(args)
        .args(["--repo", "MAECLY/coverly-roms", "--site"])
        .arg(site)
        .arg("--out")
        .arg(out)
        .output()
        .expect("run apply_release.py")
}

/// Runs the script on `release`; returns its output and the filled folder.
fn apply(release: &Value) -> (std::process::Output, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("release.json");
    std::fs::write(&json, serde_json::to_vec(release).unwrap()).unwrap();
    let out = run(
        &["--release-json", json.to_str().unwrap()],
        &repo().join("site"),
        &dir.path().join("out"),
    );
    (out, dir)
}

fn ld_software(page: &str) -> Value {
    // The release block adds a second JSON-LD node for the same @id.
    let mut merged = serde_json::Map::new();
    for chunk in page.split("<script type=\"application/ld+json\">").skip(1) {
        let v: Value = serde_json::from_str(chunk.split_once("</script>").unwrap().0).unwrap();
        let nodes = v
            .get("@graph")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![v.clone()]);
        for n in nodes {
            if n["@type"] == "SoftwareApplication" {
                merged.extend(n.as_object().unwrap().clone());
            }
        }
    }
    Value::Object(merged)
}

#[test]
fn apply_release_fills_every_marker_from_the_release() {
    if python().is_none() {
        eprintln!("no python3 here (or Windows); apply_release.py not exercised");
        return;
    }
    let (out, dir) = apply(&fixture());
    assert!(out.status.success(), "{out:?}");
    let filled = dir.path().join("out");
    for (file, text) in site_texts(&filled) {
        assert!(!text.contains("{{"), "{file} keeps a marker");
        assert!(
            !text.contains("release:begin"),
            "{file} keeps a block comment"
        );
        assert!(
            !text.contains("norelease"),
            "{file} keeps the norelease state"
        );
    }
    assert!(!filled.join("CNAME").exists(), "no GitHub Pages leftovers");
    let es = std::fs::read_to_string(filled.join("index.html")).unwrap();
    let en = std::fs::read_to_string(filled.join("en/index.html")).unwrap();
    for (_, name) in DOWNLOADS {
        let href = format!("href=\"{BASE}/download/v9.8.7/{name}\"");
        assert!(es.contains(&href) && en.contains(&href), "{href}");
    }
    // 18,934,784 bytes = 18.1 MiB.
    assert!(es.contains("Descargar .dmg (18,1&nbsp;MiB)"));
    assert!(en.contains("Download .dmg (18.1&nbsp;MiB)"));
    for page in [&es, &en] {
        assert!(page.contains(&format!("href=\"{BASE}/download/v9.8.7/SHA256SUMS\"")));
        assert!(page.contains(&format!("href=\"{BASE}/tag/v9.8.7\"")));
        assert!(!page.contains(".app.tar.gz") && !page.contains(".sig\""));
        let app = ld_software(page);
        assert_eq!(app["softwareVersion"], "9.8.7");
        assert_eq!(app["datePublished"], "2027-03-05");
        assert_eq!(app["name"], "Coverly ROMs");
        assert_eq!(app["downloadUrl"].as_array().unwrap().len(), 4);
    }
    assert!(es.contains("5 de marzo de 2027"));
    assert!(en.contains("5 March 2027"));
    let llms = std::fs::read_to_string(filled.join("llms.txt")).unwrap();
    assert!(llms.contains(&format!(
        "{BASE}/download/v9.8.7/Coverly.ROMs_9.8.7_universal.dmg (about 18.1 MiB)"
    )));
    let sitemap = std::fs::read_to_string(filled.join("sitemap.xml")).unwrap();
    assert_eq!(sitemap.matches("<lastmod>2027-03-05</lastmod>").count(), 2);
    // site/ itself is left alone.
    assert!(read("site/index.html").contains("{{release.windows_x64.url}}"));
}

#[test]
fn no_release_ships_the_not_published_state_and_no_download_link() {
    if python().is_none() {
        eprintln!("no python3 here (or Windows); apply_release.py not exercised");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        &["--no-release"],
        &repo().join("site"),
        &dir.path().join("out"),
    );
    assert!(out.status.success(), "{out:?}");
    let filled = dir.path().join("out");
    for (file, text) in site_texts(&filled) {
        assert!(!text.contains("{{"), "{file} keeps a marker");
        assert!(!text.contains("release:"), "{file} keeps a block comment");
        assert!(
            !text.contains("/releases/download/"),
            "{file} links a download"
        );
        assert!(!text.contains("<lastmod>"), "{file}");
    }
    let es = std::fs::read_to_string(filled.join("index.html")).unwrap();
    let en = std::fs::read_to_string(filled.join("en/index.html")).unwrap();
    assert!(es.contains("Todavía no hay versión publicada"));
    assert!(en.contains("No release published yet"));
    assert!(ld_software(&es).get("softwareVersion").is_none());
}

#[test]
fn apply_release_refuses_a_release_the_landing_cannot_link() {
    if python().is_none() {
        eprintln!("no python3 here (or Windows); apply_release.py not exercised");
        return;
    }
    let refused = |mutate: &dyn Fn(&mut Value), why: &str| {
        let mut release = fixture();
        mutate(&mut release);
        let (out, _dir) = apply(&release);
        assert!(!out.status.success(), "{why}: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(err.starts_with("apply_release: "), "{why}: {err}");
        err
    };
    let drop_asset = |name: &'static str| {
        move |r: &mut Value| {
            r["assets"]
                .as_array_mut()
                .unwrap()
                .retain(|a| a["name"] != name);
        }
    };
    for (label, name) in DOWNLOADS {
        let err = refused(&drop_asset(name), name);
        assert!(err.contains(label), "{err}");
    }
    refused(&drop_asset("SHA256SUMS"), "no SHA256SUMS");
    refused(
        &|r| {
            let mut extra = r["assets"][0].clone();
            extra["name"] = json!("Other.Name_9.8.7_universal.dmg");
            extra["browser_download_url"] = json!(format!(
                "{BASE}/download/v9.8.7/Other.Name_9.8.7_universal.dmg"
            ));
            r["assets"].as_array_mut().unwrap().push(extra);
        },
        "two .dmg files",
    );
    refused(&|r| r["assets"][0]["size"] = json!(0), "empty installer");
    refused(
        &|r| r["assets"][0]["state"] = json!("starter"),
        "upload not finished",
    );
    refused(
        &|r| r["assets"][0]["browser_download_url"] = json!("https://example.com/x.dmg"),
        "download elsewhere",
    );
    refused(&|r| r["prerelease"] = json!(true), "pre-release");
    refused(&|r| r["draft"] = json!(true), "draft");
    refused(&|r| r["tag_name"] = json!("nightly"), "not a version tag");
    refused(
        &|r| r["html_url"] = json!("https://example.com/"),
        "notes elsewhere",
    );
    refused(&|r| r["published_at"] = json!("yesterday"), "no date");
}

/// A marker the script does not know, one outside a release block, an
/// unbalanced block or a missing download fails the deploy.
#[test]
fn apply_release_refuses_unknown_misplaced_or_missing_markers() {
    if python().is_none() {
        eprintln!("no python3 here (or Windows); apply_release.py not exercised");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("release.json");
    std::fs::write(&json_path, serde_json::to_vec(&fixture()).unwrap()).unwrap();
    let all: String = DOWNLOADS
        .iter()
        .map(|(l, _)| format!("{{{{release.{l}.url}}}}\n"))
        .collect();
    let block = |body: &str| format!("<!-- release:begin -->{body}<!-- release:end -->");
    let go = |page: &str, n: usize, release: bool| {
        let site = dir.path().join(format!("site{n}"));
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), page).unwrap();
        let path = json_path.to_str().unwrap().to_string();
        let args: Vec<&str> = if release {
            vec!["--release-json", &path]
        } else {
            vec!["--no-release"]
        };
        run(&args, &site, &dir.path().join(format!("out{n}")))
    };
    let stderr = |o: &std::process::Output| String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(go(&block(&all), 0, true).status.success());
    assert!(go(&block(&all), 1, false).status.success());
    let unknown = go(
        &block(&format!("{all}{{{{release.windows_x86.url}}}}")),
        2,
        true,
    );
    assert!(stderr(&unknown).contains("unknown marker"), "{unknown:?}");
    let outside = go(&format!("{}{{{{release.version}}}}", block(&all)), 3, true);
    assert!(
        stderr(&outside).contains("outside a release block"),
        "{outside:?}"
    );
    let unclosed = go(&format!("<!-- release:begin -->{all}"), 4, true);
    assert!(stderr(&unclosed).contains("never closed"), "{unclosed:?}");
    let missing = go(&block("{{release.windows_x64.url}}"), 5, true);
    assert!(stderr(&missing).contains("never links"), "{missing:?}");
    let malformed = go(&block(&format!("{all}{{{{release.version}}")), 6, true);
    assert!(!malformed.status.success());
    let bad_ld = go(
        &block(&format!(
            "{all}<script type=\"application/ld+json\">{{\"v\":{{{{release.version}}}}}}</script>"
        )),
        7,
        true,
    );
    assert!(stderr(&bad_ld).contains("valid JSON"), "{bad_ld:?}");
}
