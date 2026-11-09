//! Whether an organise job's roots are there, and which missing target
//! folders apply may create (mods/cheats/translators spec D19).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where a target that did not exist at plan time hangs from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetAnchor {
    /// Nearest existing ancestor of a target that did not exist at plan
    /// time (absolute, as given).
    pub path: PathBuf,
    /// Unix `st_dev` of `path`; `None` on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev: Option<u64>,
}

/// What apply finds when it checks the job's roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootsState {
    /// Source and target are there.
    Ready,
    /// The target is missing but hangs from the recorded anchor: apply
    /// creates these folders, outermost first.
    CreateTarget { missing: Vec<PathBuf> },
    /// The source, or the volume the target lives on, is gone.
    VolumeGone,
}

/// The filesystem lookups `roots_state` needs, so tests can fake them.
pub trait RootProbe {
    fn is_dir(&self, p: &Path) -> bool;
    fn exists(&self, p: &Path) -> bool;
    fn dev(&self, p: &Path) -> Option<u64>;
}

/// The real filesystem.
pub struct OsRootProbe;

impl RootProbe for OsRootProbe {
    fn is_dir(&self, p: &Path) -> bool {
        p.is_dir()
    }
    fn exists(&self, p: &Path) -> bool {
        p.symlink_metadata().is_ok()
    }
    #[cfg(unix)]
    fn dev(&self, p: &Path) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(p).ok().map(|m| m.dev())
    }
    #[cfg(not(unix))]
    fn dev(&self, _: &Path) -> Option<u64> {
        None
    }
}

fn same(a: &Path, b: &Path) -> bool {
    a.components().eq(b.components())
}

/// A filesystem root, a Windows drive root (`E:\`, `E:`) or a UNC share
/// root (`\\server\share`); the Windows forms by text so the rule is the
/// same (and tested) on every OS.
pub fn is_volume_root(path: &Path) -> bool {
    if path.parent().is_none() {
        return true;
    }
    let text = path.to_string_lossy();
    let t = text.trim_end_matches(['\\', '/']);
    let b = t.as_bytes();
    if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return true;
    }
    if let Some(rest) = t.strip_prefix(r"\\").or_else(|| t.strip_prefix("//")) {
        return rest.split(['\\', '/']).filter(|s| !s.is_empty()).count() <= 2;
    }
    false
}

fn parent_of(p: &Path) -> Option<&Path> {
    p.parent().filter(|q| !q.as_os_str().is_empty())
}

/// The anchor a plan records for a target that does not exist yet: its
/// nearest existing ancestor and that folder's device. `None` when the
/// target exists, or when nothing above it is an existing folder.
pub fn anchor_of(target: &Path, probe: &dyn RootProbe) -> Option<TargetAnchor> {
    if probe.is_dir(target) {
        return None;
    }
    let mut cur = parent_of(target);
    while let Some(p) = cur {
        if probe.exists(p) {
            return probe.is_dir(p).then(|| TargetAnchor {
                path: p.to_path_buf(),
                dev: probe.dev(p),
            });
        }
        cur = parent_of(p);
    }
    None
}

/// Whether apply may start (D19). A missing source is a vanished volume; a
/// missing target is created only when it did not exist at plan time and
/// still hangs from the same anchor on the same device, which is not a
/// mount parent nor a filesystem, drive or UNC share root.
pub fn roots_state(
    source: &Path,
    target: &Path,
    anchor: Option<&TargetAnchor>,
    mount_parents: &[PathBuf],
    probe: &dyn RootProbe,
) -> RootsState {
    if !probe.is_dir(source) {
        return RootsState::VolumeGone;
    }
    if probe.is_dir(target) {
        return RootsState::Ready;
    }
    let Some(anchor) = anchor else {
        return RootsState::VolumeGone;
    };
    let mut missing = Vec::new();
    let mut cur = Some(target);
    let found = loop {
        let Some(p) = cur else { break None };
        if probe.exists(p) {
            break Some(p);
        }
        if is_volume_root(p) {
            break None;
        }
        missing.push(p.to_path_buf());
        cur = parent_of(p);
    };
    let Some(found) = found else {
        return RootsState::VolumeGone;
    };
    if !probe.is_dir(found)
        || !same(found, &anchor.path)
        || probe.dev(found) != anchor.dev
        || is_volume_root(found)
        || mount_parents.iter().any(|m| same(m, found))
    {
        return RootsState::VolumeGone;
    }
    missing.reverse();
    RootsState::CreateTarget { missing }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Directories that exist, with their device number, and plain files.
    struct Fake(Vec<(PathBuf, u64)>, Vec<PathBuf>);
    impl RootProbe for Fake {
        fn is_dir(&self, p: &Path) -> bool {
            self.0
                .iter()
                .any(|(d, _)| d.components().eq(p.components()))
        }
        fn exists(&self, p: &Path) -> bool {
            self.is_dir(p) || self.1.iter().any(|f| f.components().eq(p.components()))
        }
        fn dev(&self, p: &Path) -> Option<u64> {
            self.0
                .iter()
                .find(|(d, _)| d.components().eq(p.components()))
                .map(|(_, v)| *v)
        }
    }
    fn pb(parts: &[&str]) -> PathBuf {
        let mut p = PathBuf::from(std::path::MAIN_SEPARATOR_STR);
        for c in parts {
            p.push(c);
        }
        p
    }
    fn world() -> Fake {
        Fake(
            vec![
                (pb(&[]), 1),
                (pb(&["home"]), 1),
                (pb(&["home", "m"]), 1),
                (pb(&["pile"]), 1),
                (pb(&["Volumes"]), 1),
                (pb(&["mnt"]), 1),
                (pb(&["mnt", "usb"]), 1),
            ],
            Vec::new(),
        )
    }
    fn anchor(parts: &[&str], dev: u64) -> TargetAnchor {
        TargetAnchor {
            path: pb(parts),
            dev: Some(dev),
        }
    }

    #[test]
    fn an_existing_target_is_ready() {
        let f = world();
        assert_eq!(
            roots_state(&pb(&["pile"]), &pb(&["home", "m"]), None, &[], &f),
            RootsState::Ready
        );
    }

    #[test]
    fn a_missing_source_is_a_vanished_volume() {
        let f = world();
        assert_eq!(
            roots_state(&pb(&["gone"]), &pb(&["home", "m"]), None, &[], &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn missing_levels_under_the_recorded_anchor_are_created_outermost_first() {
        let f = world();
        let t = pb(&["home", "m", "new", "sub"]);
        assert_eq!(anchor_of(&t, &f), Some(anchor(&["home", "m"], 1)));
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &t,
                Some(&anchor(&["home", "m"], 1)),
                &[],
                &f
            ),
            RootsState::CreateTarget {
                missing: vec![pb(&["home", "m", "new"]), t.clone()]
            }
        );
    }

    #[test]
    fn a_target_that_existed_when_planned_is_never_recreated() {
        let f = world();
        let t = pb(&["home", "m", "new"]);
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, None, &[], &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn an_existing_target_has_no_anchor() {
        let f = world();
        assert_eq!(anchor_of(&pb(&["home", "m"]), &f), None);
    }

    #[test]
    fn an_unplugged_drive_under_a_mount_parent_is_gone() {
        let f = world();
        let t = pb(&["Volumes", "USB", "ROMS"]);
        let a = anchor_of(&t, &f).unwrap();
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&a), &[pb(&["Volumes"])], &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_different_nearest_ancestor_or_device_is_gone() {
        let f = world();
        let t = pb(&["mnt", "usb", "ROMS"]);
        // Planned while mounted: anchor /mnt/usb on device 7; now /mnt/usb
        // is an empty folder on device 1.
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &t,
                Some(&anchor(&["mnt", "usb"], 7)),
                &[],
                &f
            ),
            RootsState::VolumeGone
        );
        // Planned with /mnt/usb missing below /mnt: the ancestor found now
        // is not the recorded one.
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&anchor(&["mnt"], 1)), &[], &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_filesystem_root_as_nearest_ancestor_is_gone() {
        let f = world();
        let t = pb(&["nothing", "here"]);
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&anchor(&[], 1)), &[], &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn windows_drive_and_unc_roots_are_volume_roots_on_every_os() {
        for root in [
            r"E:\",
            "E:",
            "e:/",
            r"\\server\share",
            r"\\server\share\",
            "//server/share",
        ] {
            assert!(is_volume_root(Path::new(root)), "{root}");
        }
        for not in [r"E:\ROMS", r"\\server\share\ROMS"] {
            assert!(!is_volume_root(Path::new(not)), "{not}");
        }
        let f = world();
        // A missing drive: nothing above `E:\ROMS` exists in the fake → gone.
        let t = PathBuf::from(r"E:\ROMS");
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &t,
                Some(&TargetAnchor {
                    path: PathBuf::from(r"E:\"),
                    dev: None
                }),
                &[],
                &f
            ),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_level_that_appeared_as_another_device_is_gone() {
        // /mnt/usb/ROMS is now the mount point of another device.
        let mut f = world();
        f.0.push((pb(&["mnt", "usb", "ROMS"]), 9));
        let t = pb(&["mnt", "usb", "ROMS", "sub"]);
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &t,
                Some(&anchor(&["mnt", "usb"], 1)),
                &[],
                &f
            ),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_stale_folder_under_a_mount_parent_anchor_is_never_written() {
        // Planned while /Volumes/USB was missing (anchor /Volumes); a stale
        // /Volumes/USB folder on the system disk now is not written into,
        // and neither is /Volumes itself.
        let mut f = world();
        let t = pb(&["Volumes", "USB", "ROMS"]);
        let mounts = [pb(&["Volumes"])];
        let a = anchor(&["Volumes"], 1);
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&a), &mounts, &f),
            RootsState::VolumeGone
        );
        f.0.push((pb(&["Volumes", "USB"]), 1));
        assert_eq!(
            roots_state(&pb(&["pile"]), &t, Some(&a), &mounts, &f),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_file_where_a_folder_should_be_is_gone() {
        let mut f = world();
        f.1.push(pb(&["home", "m", "new"]));
        let t = pb(&["home", "m", "new", "sub"]);
        assert_eq!(anchor_of(&t, &f), None, "a file is no anchor");
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &t,
                Some(&anchor(&["home", "m"], 1)),
                &[],
                &f
            ),
            RootsState::VolumeGone
        );
        // The target itself is a file.
        assert_eq!(
            roots_state(
                &pb(&["pile"]),
                &pb(&["home", "m", "new"]),
                Some(&anchor(&["home", "m"], 1)),
                &[],
                &f
            ),
            RootsState::VolumeGone
        );
    }

    #[test]
    fn a_missing_anchor_device_on_windows_still_matches() {
        // Windows records no device: `None` on both sides is the same volume.
        let f = Fake(vec![(pb(&[]), 0), (pb(&["home"]), 0)], Vec::new());
        struct NoDev(Fake);
        impl RootProbe for NoDev {
            fn is_dir(&self, p: &Path) -> bool {
                self.0.is_dir(p)
            }
            fn exists(&self, p: &Path) -> bool {
                self.0.exists(p)
            }
            fn dev(&self, _: &Path) -> Option<u64> {
                None
            }
        }
        let probe = NoDev(f);
        let t = pb(&["home", "ROMS"]);
        let a = anchor_of(&t, &probe).unwrap();
        assert_eq!(a.dev, None);
        assert_eq!(
            roots_state(&pb(&["home"]), &t, Some(&a), &[], &probe),
            RootsState::CreateTarget {
                missing: vec![t.clone()]
            }
        );
    }
}
