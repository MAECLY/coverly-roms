//! The listing a folder probe looks at (adjustments spec D20).
//!
//! A probe never touches the file system itself: it asks its
//! [`DirListing`]'s [`ListingFs`] to list a named child, look at an
//! entry, read a file head or open a file for budgeted reads, and to say
//! where an icon is. [`DiskFs`] answers from a real folder with the same
//! calls the probes made before (`symlink_metadata`, one bounded
//! `read_dir`, reads through [`Budgeted`]); an archive answers from the
//! entries it already listed.

use std::fs;
use std::path::{Path, PathBuf};

use super::{ArtFormat, ArtLocator, Budgeted, FolderMatch};
use crate::identify::sniff::{FileAt, ReadAt};

/// Most names one child listing returns (spec D12).
pub(crate) const MAX_CHILD_ENTRIES: usize = 256;

/// What a probe may know about one entry, never following a link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryMeta {
    /// A real directory (not a link to one).
    pub dir: bool,
    /// A regular file (not a link to one).
    pub file: bool,
    /// A symbolic link (neither `dir` nor `file`).
    pub link: bool,
    /// The entry's length in bytes.
    pub len: u64,
    /// Whether a directory may be a FAT «archive bit» split file
    /// (`nx::probe_split`). `None` where no such attribute exists (an
    /// archive's entries), so the split probe never matches there.
    pub fat_archive_bit: Option<bool>,
}

/// The file system a folder probe reads through. Every `rel` is relative
/// to the root of the file system, never absolute, and never leaves it.
pub trait ListingFs {
    /// Up to [`MAX_CHILD_ENTRIES`] names of the children of the real
    /// directory `rel` (never a linked one), or `None`.
    fn list(&self, rel: &Path) -> Option<Vec<String>>;
    /// The entry at `rel`, or `None` when there is none.
    fn meta(&self, rel: &Path) -> Option<EntryMeta>;
    /// The first `cap` bytes of the regular file `rel` (all of it when
    /// shorter), read through the implementation's budget.
    fn read(&self, rel: &Path, cap: u64) -> Option<Vec<u8>>;
    /// The regular file `rel` for budgeted reads at any offset, with its
    /// length (headers whose fields sit past the head: XCI, STFS, DOL).
    fn open(&self, rel: &Path) -> Option<(Box<dyn ReadAt + '_>, u64)>;
    /// Where the icon at `rel` is, for the embedded art source.
    fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator;
}

/// A real folder on disk: the root is the folder the walk listed.
#[derive(Clone, Copy, Debug)]
pub struct DiskFs<'a> {
    pub root: &'a Path,
}

impl DiskFs<'_> {
    fn path(&self, rel: &Path) -> PathBuf {
        if rel.as_os_str().is_empty() {
            self.root.to_path_buf()
        } else {
            self.root.join(rel)
        }
    }
}

impl ListingFs for DiskFs<'_> {
    fn list(&self, rel: &Path) -> Option<Vec<String>> {
        list_child(&self.path(rel))
    }

    fn meta(&self, rel: &Path) -> Option<EntryMeta> {
        let meta = fs::symlink_metadata(self.path(rel)).ok()?;
        let t = meta.file_type();
        Some(EntryMeta {
            dir: t.is_dir(),
            file: t.is_file(),
            link: t.is_symlink(),
            len: meta.len(),
            // The attribute itself is not read: a disk folder may carry it
            // (a split file copied from a FAT card), and its name decides.
            fat_archive_bit: t.is_dir().then_some(true),
        })
    }

    fn read(&self, rel: &Path, cap: u64) -> Option<Vec<u8>> {
        let path = self.path(rel);
        let len = plain_file(&path, u64::MAX)?;
        read_head(&path, len, usize::try_from(cap).unwrap_or(usize::MAX))
    }

    fn open(&self, rel: &Path) -> Option<(Box<dyn ReadAt + '_>, u64)> {
        let path = self.path(rel);
        let len = plain_file(&path, u64::MAX)?;
        let file = super::open_rom(&path).ok()?;
        Some((Box::new(FileAt(file)), len))
    }

    fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator {
        ArtLocator::File {
            rel: rel.to_path_buf(),
            format,
        }
    }
}

/// The names the walk already listed for one directory (spec D12): every
/// entry, files and folders, AppleDouble twins and OS junk left out, and
/// the file system the probes read the rest through (D20).
pub struct DirListing<'a> {
    pub fs: &'a dyn ListingFs,
    /// The listing root inside `fs` (`Path::new("")` for [`DiskFs`]).
    pub dir: &'a Path,
    /// The folder's own name (the disk folder's name, the root folder's
    /// name inside an archive, or the archive stem for a root at its top).
    pub name: &'a str,
    /// The name of the folder holding it (`""` when there is none): a
    /// Homebrew Channel app sits in `apps/`.
    pub parent: &'a str,
    pub names: &'a [String],
}

impl DirListing<'_> {
    fn rel(&self, rel: &Path) -> PathBuf {
        self.dir.join(rel)
    }

    /// The names of the child directory `rel` (one bounded listing).
    pub fn list(&self, rel: &Path) -> Option<Vec<String>> {
        self.fs.list(&self.rel(rel))
    }

    /// The entry at `rel` inside the folder.
    pub fn meta(&self, rel: &Path) -> Option<EntryMeta> {
        self.fs.meta(&self.rel(rel))
    }

    /// Whether `rel` is a real directory (never a link).
    pub fn is_dir(&self, rel: &Path) -> bool {
        self.meta(rel).is_some_and(|m| m.dir)
    }

    /// The length of the regular file `rel` (never a link) when it is at
    /// most `max` bytes.
    pub fn plain_file(&self, rel: &Path, max: u64) -> Option<u64> {
        self.meta(rel)
            .filter(|m| m.file && m.len <= max)
            .map(|m| m.len)
    }

    /// The first `n` bytes of the regular file `rel` of at most `max`
    /// bytes (all of it when shorter).
    pub fn read_head(&self, rel: &Path, max: u64, n: u64) -> Option<Vec<u8>> {
        self.plain_file(rel, max)?;
        self.fs.read(&self.rel(rel), n)
    }

    /// The whole regular file `rel` of at most `max` bytes.
    pub fn read_small(&self, rel: &Path, max: u64) -> Option<Vec<u8>> {
        self.read_head(rel, max, max)
    }

    /// Runs `f` over the regular file `rel` through a fresh [`Budgeted`].
    pub fn with_file<R>(
        &self,
        rel: &Path,
        f: impl FnOnce(&mut Budgeted) -> Option<R>,
    ) -> Option<R> {
        let (mut src, len) = self.fs.open(&self.rel(rel))?;
        let mut b = Budgeted::new(src.as_mut(), len);
        f(&mut b)
    }

    /// Where the icon at `rel` (relative to the folder) is.
    pub fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator {
        self.fs.art(&self.rel(rel), format)
    }
}

/// Runs `f` over the listing of the disk folder `dir` whose names the
/// walk already listed.
pub fn on_disk<R>(dir: &Path, names: &[String], f: impl FnOnce(&DirListing) -> R) -> R {
    let fs = DiskFs { root: dir };
    let lossy = |p: Option<&Path>| {
        p.and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let name = lossy(Some(dir));
    let parent = lossy(dir.parent());
    f(&DirListing {
        fs: &fs,
        dir: Path::new(""),
        name: &name,
        parent: &parent,
        names,
    })
}

/// [`super::identify_dir`] on the disk folder `dir`.
pub fn identify_dir_on_disk(
    dir: &Path,
    names: &[String],
) -> Result<Option<FolderMatch>, super::DirConflict> {
    on_disk(dir, names, super::identify_dir)
}

/// A regular file (never a symlink) of at most `max` bytes.
pub(super) fn plain_file(path: &Path, max: u64) -> Option<u64> {
    let meta = fs::symlink_metadata(path).ok()?;
    (meta.file_type().is_file() && meta.len() <= max).then_some(meta.len())
}

/// The first `n` bytes of `path` (or all of it when shorter), through
/// [`Budgeted`]; `path` was checked by [`plain_file`].
fn read_head(path: &Path, len: u64, n: usize) -> Option<Vec<u8>> {
    let file = super::open_rom(path).ok()?;
    let mut src = FileAt(file);
    let mut b = Budgeted::new(&mut src, len);
    let n = usize::try_from(len).ok()?.min(n);
    b.read(0, n).ok()
}

/// Up to [`MAX_CHILD_ENTRIES`] names of the directory `dir` (never a
/// symlinked one).
fn list_child(dir: &Path) -> Option<Vec<String>> {
    if !fs::symlink_metadata(dir).ok()?.file_type().is_dir() {
        return None;
    }
    #[cfg(test)]
    list_count::bump();
    Some(
        fs::read_dir(dir)
            .ok()?
            .flatten()
            .take(MAX_CHILD_ENTRIES)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
    )
}

/// How many directories [`DiskFs::list`] listed on this thread (unit
/// tests of the one-listing rule, systems batch 3 D5).
#[cfg(test)]
pub(super) mod list_count {
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub(in super::super) fn bump() {
        COUNT.with(|c| c.set(c.get() + 1));
    }

    /// The count since the last call, then zero.
    pub(in super::super) fn take() -> usize {
        COUNT.with(|c| c.replace(0))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::super::testing as t;
    use super::super::{identify_dir, DirConflict, IdSource, TitleKind};
    use super::*;

    /// A [`DiskFs`] that records every call, can drop the archive bit and
    /// can answer art as an archive would.
    struct RecordingFs<'a> {
        disk: DiskFs<'a>,
        calls: RefCell<Vec<(&'static str, PathBuf)>>,
        archive_bit: bool,
        zip_art: bool,
    }

    impl<'a> RecordingFs<'a> {
        fn new(root: &'a Path) -> Self {
            RecordingFs {
                disk: DiskFs { root },
                calls: RefCell::new(Vec::new()),
                archive_bit: true,
                zip_art: false,
            }
        }

        fn log(&self, op: &'static str, rel: &Path) {
            self.calls.borrow_mut().push((op, rel.to_path_buf()));
        }

        fn ops(&self) -> Vec<&'static str> {
            self.calls.borrow().iter().map(|(op, _)| *op).collect()
        }
    }

    impl ListingFs for RecordingFs<'_> {
        fn list(&self, rel: &Path) -> Option<Vec<String>> {
            self.log("list", rel);
            self.disk.list(rel)
        }

        fn meta(&self, rel: &Path) -> Option<EntryMeta> {
            self.log("meta", rel);
            let mut m = self.disk.meta(rel)?;
            if !self.archive_bit {
                m.fat_archive_bit = None;
            }
            Some(m)
        }

        fn read(&self, rel: &Path, cap: u64) -> Option<Vec<u8>> {
            self.log("read", rel);
            self.disk.read(rel, cap)
        }

        fn open(&self, rel: &Path) -> Option<(Box<dyn ReadAt + '_>, u64)> {
            self.log("open", rel);
            self.disk.open(rel)
        }

        fn art(&self, rel: &Path, format: ArtFormat) -> ArtLocator {
            self.log("art", rel);
            if !self.zip_art {
                return self.disk.art(rel, format);
            }
            let name: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            ArtLocator::ZipEntry {
                name: name.join("/"),
                format,
            }
        }
    }

    fn names_of(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn through(
        fs: &RecordingFs,
        name: &str,
        parent: &str,
        names: &[String],
    ) -> Result<Option<FolderMatch>, DirConflict> {
        identify_dir(&DirListing {
            fs,
            dir: Path::new(""),
            name,
            parent,
            names,
        })
    }

    /// Every built-in probe recognises its fixture through the trait
    /// alone, with the same answer as the disk listing (D20).
    #[test]
    fn every_probe_reads_through_the_listing_fs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        let mut dirs: Vec<(PathBuf, &str)> = Vec::new();
        let at = |n: &str| root.join(n);
        t::ps3_jb(&at("jb"), "BLUS30443", "DG");
        dirs.push((at("jb"), "ps3"));
        t::ps3_hdd(&at("hdd"), "NPUB30024", "HG");
        dirs.push((at("hdd"), "ps3"));
        t::vita_app(&at("vita"), "PCSE00120");
        dirs.push((at("vita"), "psvita"));
        t::ps4_app(
            &at("ps4"),
            "CUSA00900",
            "gd",
            "UP9000-CUSA00900_00-BLOODBORNE000000",
        );
        dirs.push((at("ps4"), "ps4"));
        t::wiiu_folder(&at("loadiine"), "0005000010101C00", "WUP-P-ARKE", "Mario");
        dirs.push((at("loadiine"), "wiiu"));
        t::nus_folder(&at("nus"), 0x0005_0000_1010_ED00);
        dirs.push((at("nus"), "wiiu"));
        t::put_file(&at("halo"), "default.xbe", &t::xbe(0x4D53_0004, "Halo"));
        dirs.push((at("halo"), "xbox"));
        t::put_file(&at("h3"), "default.xex", &t::xex2(0x4D53_07E6));
        dirs.push((at("h3"), "xbox360"));
        let god = t::god_folder(&at("god"), 0x4D53_07E6, 0x0000_7000, b"LIVE");
        dirs.push((god, "xbox360"));
        t::gdk_folder(&at("gdk"), &t::GdkSpec::default());
        dirs.push((at("gdk"), "xboxone"));
        t::era_folder(&at("era"), &t::era_manifest("A.B", "Era", "Era", true));
        dirs.push((at("era"), "xboxone"));
        let split = at("Mario [0100000000010000][v0].nsp");
        let nsp = t::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
        t::put_file(&split, "00", &nsp.bytes());
        t::put_file(&split, "01", &[0u8; 16]);
        dirs.push((split, "switch"));
        t::put_file(&at("apps").join("hb"), "boot.dol", &t::dol(1, true));
        dirs.push((at("apps").join("hb"), "wii"));

        for (dir, system) in &dirs {
            let names = names_of(dir);
            let on_disk = identify_dir_on_disk(dir, &names);
            let fs = RecordingFs::new(dir);
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let parent = dir
                .parent()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let found = through(&fs, &name, &parent, &names);
            assert_eq!(found, on_disk, "{}", dir.display());
            let m = found
                .unwrap()
                .unwrap_or_else(|| panic!("{}", dir.display()));
            assert_eq!(m.id.system, *system, "{}", dir.display());
            let ops = fs.ops();
            assert!(
                ops.iter().any(|op| *op == "read" || *op == "open"),
                "{}: {ops:?}",
                dir.display()
            );
            for (_, rel) in fs.calls.borrow().iter() {
                assert!(rel.is_relative(), "{}", rel.display());
            }
        }
    }

    /// The `CUSA…` fallback reads the listing's name, not the root path.
    #[test]
    fn the_cusa_fallback_reads_the_listing_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("x");
        let sfo = t::param_sfo_with(&[("CATEGORY", "gp"), ("TITLE", "T")], &[]);
        t::put_file(&dir, "sce_sys/param.sfo", &sfo);
        let names = names_of(&dir);
        let fs = RecordingFs::new(&dir);
        let m = through(&fs, "CUSA00900-UPDATE", "", &names)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                m.id.system.as_str(),
                m.id.title_id.as_deref(),
                m.id.id_source,
                m.id.kind
            ),
            (
                "ps4",
                Some("CUSA00900"),
                Some(IdSource::FileName),
                TitleKind::Update
            )
        );
        let disk = identify_dir_on_disk(&dir, &names).unwrap().unwrap();
        assert_eq!(disk.id.title_id, None);
    }

    /// Art comes from the file system's own locator.
    #[test]
    fn art_is_built_by_the_listing_fs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("CUSA00900");
        t::ps4_app(
            &dir,
            "CUSA00900",
            "gd",
            "UP9000-CUSA00900_00-BLOODBORNE000000",
        );
        let names = names_of(&dir);
        let mut fs = RecordingFs::new(&dir);
        fs.zip_art = true;
        let m = through(&fs, "CUSA00900", "", &names).unwrap().unwrap();
        assert_eq!(
            m.id.art,
            [ArtLocator::ZipEntry {
                name: "sce_sys/icon0.png".into(),
                format: ArtFormat::Png
            }]
        );
        let disk = identify_dir_on_disk(&dir, &names).unwrap().unwrap();
        assert_eq!(
            disk.id.art,
            [ArtLocator::File {
                rel: Path::new("sce_sys").join("icon0.png"),
                format: ArtFormat::Png
            }]
        );
    }

    /// Without the archive attribute (an archive's entries) a split
    /// folder is no match; on disk it is.
    #[test]
    fn split_files_need_the_archive_bit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("Mario [0100000000010000][v0].nsp");
        let nsp = t::nsp(&[("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca", &[0x55; 64])]);
        t::put_file(&dir, "00", &nsp.bytes());
        t::put_file(&dir, "01", &[0u8; 16]);
        let names = names_of(&dir);
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let mut fs = RecordingFs::new(&dir);
        assert_eq!(
            through(&fs, &name, "", &names).unwrap().unwrap().id.system,
            "switch"
        );
        fs.archive_bit = false;
        fs.calls.borrow_mut().clear();
        assert_eq!(through(&fs, &name, "", &names), Ok(None));
        assert!(!fs.ops().contains(&"open"), "{:?}", fs.ops());
    }

    /// A Homebrew Channel app is found by the listing's parent name.
    #[test]
    fn apps_reads_the_listing_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("hb");
        t::put_file(&dir, "boot.dol", &t::dol(1, true));
        let names = names_of(&dir);
        let fs = RecordingFs::new(&dir);
        assert_eq!(through(&fs, "hb", "", &names), Ok(None));
        let m = through(&fs, "hb", "APPS", &names).unwrap().unwrap();
        assert_eq!(m.id.detail, "apps boot.dol");
    }
}
