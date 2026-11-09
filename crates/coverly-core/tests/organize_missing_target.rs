//! `organize --apply` into a folder that does not exist yet (mods/cheats/
//! translators spec D19, `TODO.md` «Fallos conocidos»).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use coverly_core::cache::Cache;
use coverly_core::dedupe::testing::pin_mtime;
use coverly_core::identify::sniff::fixtures as fx;
use coverly_core::organize::{
    apply_organize, plan_organize, ApplyEnv, MetaProbe, OrganizeEnv, OrganizeOptions, OrganizePlan,
    OrganizeReport, TransferMode,
};
use coverly_core::profiles::builtin_profiles;
use coverly_core::progress::PauseKind;
use coverly_core::systems::SystemDb;
use coverly_core::write::trash::FakeTrash;
use coverly_core::write::undo_job;

struct NoCloud;

impl MetaProbe for NoCloud {
    fn cloud_only(&self, _: &Path, _: &fs::Metadata) -> bool {
        false
    }
}

fn put(root: &Path, rel: &str, bytes: &[u8], n: u64) -> PathBuf {
    let p = rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    pin_mtime(&p, n);
    p
}

fn parts(p: &Path) -> Vec<String> {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn options(layout: &str, target: &Path) -> OrganizeOptions {
    OrganizeOptions {
        layout: layout.to_string(),
        target: Some(target.to_path_buf()),
        mode: Some(TransferMode::Move),
        offline: true,
        ..OrganizeOptions::default()
    }
}

fn plan_of(pile: &Path, opts: &OrganizeOptions) -> OrganizePlan {
    let db = SystemDb::builtin();
    let profiles = builtin_profiles();
    let cache = Cache::in_memory().unwrap();
    let env = OrganizeEnv {
        db: &db,
        profiles: &profiles,
        cache: &cache,
        dats: None,
        probe: &NoCloud,
        online: None,
        titles: None,
        now: SystemTime::now(),
    };
    plan_organize(pile, opts, &env, &|_| {}, &AtomicBool::new(false)).unwrap()
}

fn apply_in(plan: &OrganizePlan, data: &Path) -> OrganizeReport {
    apply_organize(
        plan,
        &ApplyEnv {
            data_dir: data,
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
            trash: &FakeTrash::unavailable("no-trash"),
            tools: Some(&[]),
        },
    )
    .unwrap()
}

/// The header line of job `job`'s journal (active or undone).
fn header(data: &Path, job: &str) -> serde_json::Value {
    let jobs = data.join("jobs");
    let path = [format!("{job}.jsonl"), format!("{job}.undone.jsonl")]
        .iter()
        .map(|n| jobs.join(n))
        .find(|p| p.is_file())
        .unwrap();
    let text = fs::read_to_string(path).unwrap();
    serde_json::from_str(text.lines().next().unwrap()).unwrap()
}

/// `<tmp>/pile/Tetris (World).gb` planned into `<tmp>/new/sub`, which does
/// not exist.
fn pile_into_missing_target(tmp: &Path) -> (PathBuf, PathBuf, OrganizePlan) {
    let pile = tmp.join("pile");
    put(&pile, "Tetris (World).gb", &fx::gb_tagged(0x00, 41), 1);
    let target = tmp.join("new").join("sub");
    assert!(!target.exists());
    let plan = plan_of(&pile, &options("anbernic-stock", &target));
    assert_eq!(
        plan.target_anchor.as_ref().map(|a| parts(&a.path)),
        Some(parts(tmp))
    );
    (pile, target, plan)
}

#[test]
fn apply_creates_a_missing_target_and_undo_removes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, target, plan) = pile_into_missing_target(tmp.path());

    let data = tmp.path().join("data");
    let report = apply_in(&plan, &data);
    assert_eq!(report.paused, None, "{report:?}");
    assert_eq!(report.moved, 1, "{report:?}");
    assert!(target.is_dir());
    assert!(!pile.join("Tetris (World).gb").exists());

    let job = report.job.clone().unwrap();
    let created: Vec<Vec<String>> = header(&data, &job)["created_roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| parts(Path::new(v.as_str().unwrap())))
        .collect();
    assert_eq!(
        created,
        vec![parts(&tmp.path().join("new")), parts(&target)],
        "outermost first"
    );

    let undone = undo_job(&data, &job).unwrap();
    assert!(pile.join("Tetris (World).gb").is_file(), "{undone:?}");
    assert!(
        !tmp.path().join("new").exists(),
        "created folders removed innermost first: {undone:?}"
    );
}

#[test]
fn undo_keeps_a_created_target_the_user_added_to() {
    let tmp = tempfile::tempdir().unwrap();
    let (pile, target, plan) = pile_into_missing_target(tmp.path());
    let data = tmp.path().join("data");
    let report = apply_in(&plan, &data);
    assert_eq!(report.paused, None, "{report:?}");
    let job = report.job.clone().unwrap();
    let mine = put(&target, "mine.txt", b"x", 9);

    let undone = undo_job(&data, &job).unwrap();
    assert!(pile.join("Tetris (World).gb").is_file(), "{undone:?}");
    assert!(target.is_dir(), "{undone:?}");
    assert!(tmp.path().join("new").is_dir());
    assert_eq!(fs::read(&mine).unwrap(), b"x");
}

#[test]
fn a_target_gone_since_planning_still_pauses() {
    let tmp = tempfile::tempdir().unwrap();
    let pile = tmp.path().join("pile");
    put(&pile, "Tetris (World).gb", &fx::gb_tagged(0x00, 42), 1);
    let target = tmp.path().join("card");
    fs::create_dir_all(&target).unwrap();
    let plan = plan_of(&pile, &options("anbernic-stock", &target));
    assert_eq!(plan.target_anchor, None);
    fs::remove_dir(&target).unwrap();

    let data = tmp.path().join("data");
    let report = apply_in(&plan, &data);
    assert_eq!(report.paused, Some(PauseKind::Volume), "{report:?}");
    assert_eq!(
        report.pause_reason.map(|r| format!("{r:?}")),
        Some("VolumeGone".to_string())
    );
    assert!(!target.exists(), "nothing created");
    assert_eq!(report.job, None);
    assert!(pile.join("Tetris (World).gb").is_file());
    let jobs = data.join("jobs");
    assert!(
        !jobs.is_dir() || fs::read_dir(&jobs).unwrap().next().is_none(),
        "no job"
    );
}

fn json_path(p: &Path) -> String {
    serde_json::to_string(p.to_str().unwrap()).unwrap()
}

#[test]
fn an_old_header_without_created_roots_still_undoes() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, root, data) = (
        tmp.path().join("S"),
        tmp.path().join("T"),
        tmp.path().join("d"),
    );
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(root.join("ROMS").join("GB")).unwrap();
    let job = "20261017T100000Z-old1";
    let jobs = data.join("jobs");
    fs::create_dir_all(&jobs).unwrap();
    let entry = |p: &Path| {
        format!(
            "{{\"type\":\"entry\",\"target\":{},\"op\":{{\"kind\":\"dir-created\"}}}}\n",
            json_path(p)
        )
    };
    let text = format!(
        "{{\"type\":\"header\",\"job\":\"{job}\",\"root\":{},\"source_root\":{},\"kind\":\"organize\",\"created_at\":\"2026-10-17T10:00:00Z\"}}\n{}{}{{\"type\":\"done\"}}\n",
        json_path(&root),
        json_path(&src),
        entry(&root.join("ROMS")),
        entry(&root.join("ROMS").join("GB")),
    );
    fs::write(jobs.join(format!("{job}.jsonl")), text).unwrap();

    let undone = undo_job(&data, job).unwrap();
    assert!(!root.join("ROMS").exists(), "{undone:?}");
    assert!(root.is_dir(), "a root the job did not create is kept");
    assert!(jobs.join(format!("{job}.undone.jsonl")).is_file());
}
