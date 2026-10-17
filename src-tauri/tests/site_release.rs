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
//! - `.github/workflows/pages.yml` runs it before uploading the Pages
//!   artifact, publishes that copy and not `site/`, and redeploys when a
//!   release is published.

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
    assert_eq!(read("site/CNAME").trim(), "coverly-roms.maecly.com");
}

#[test]
fn pages_workflow_fills_the_release_before_publishing() {
    let wf = read(".github/workflows/pages.yml");
    let on = wf
        .split_once("\non:\n")
        .unwrap()
        .1
        .split_once("\npermissions:")
        .unwrap()
        .0;
    assert!(on.contains("  release:\n    types: [published]\n"), "{on}");
    assert!(on.contains("  workflow_dispatch:"), "{on}");
    assert!(on.contains("      - 'site/**'"), "{on}");
    assert!(on.contains("      - 'scripts/site/**'"), "{on}");

    let check = wf
        .find("python3 -I scripts/site/check_site.py")
        .expect("pages.yml checks the landing");
    let fill = wf
        .find("python3 -I scripts/site/apply_release.py")
        .expect("pages.yml runs apply_release.py");
    let upload = wf.find("actions/upload-pages-artifact@").unwrap();
    assert!(check < fill && fill < upload, "check, fill, then upload");
    assert!(wf.contains("gh api \"repos/$REPO/releases/latest\" > \"$RUNNER_TEMP/release.json\""));
    // Only "no release yet" selects the not-published state.
    assert!(wf.contains("grep -q \"HTTP 404\" \"$RUNNER_TEMP/release.err\""));
    // The filled copy is what Pages gets, never site/ with its markers.
    assert!(wf.contains("--out \"$RUNNER_TEMP/site\""));
    assert!(wf.contains("          path: ${{ runner.temp }}/site\n"));
    assert!(!wf.contains("          path: site\n"));
    let step = &wf[fill..upload];
    for bypass in ["continue-on-error", "|| true", "--skip", "--allow"] {
        assert!(!wf.contains(bypass) && !step.contains(bypass), "{bypass}");
    }
    // Least privilege.
    assert!(wf.contains("\npermissions: {}\n"));
    assert!(!wf.contains("contents: write"));
    // Every action pinned to a full commit SHA.
    for line in wf
        .lines()
        .filter(|l| l.trim_start().starts_with("- uses:") || l.trim_start().starts_with("uses:"))
    {
        let rev = line.split_once('@').unwrap().1;
        assert!(
            rev.len() > 40 && rev[..40].chars().all(|c| c.is_ascii_hexdigit()),
            "{line}"
        );
    }

    let redeploy =
        &wf[wf.find("  redeploy:\n").expect("a redeploy job")..wf.find("  deploy:\n").unwrap()];
    assert!(redeploy.contains("if: github.event_name == 'release'"));
    assert!(redeploy.contains("actions: write"));
    assert!(
        redeploy.contains("gh workflow run pages.yml --repo \"$REPO\" --ref \"$DEFAULT_BRANCH\"")
    );
    let deploy = &wf[wf.find("  deploy:\n").unwrap()..];
    assert!(deploy.contains("if: github.event_name != 'release'"));
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
    assert_eq!(
        std::fs::read(filled.join("CNAME")).unwrap(),
        std::fs::read(repo().join("site/CNAME")).unwrap()
    );
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
