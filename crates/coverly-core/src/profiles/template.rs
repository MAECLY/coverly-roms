//! Path templates of media slots.

use std::path::{Path, PathBuf};

/// Values substituted into a slot template.
#[derive(Clone, Copy, Debug)]
pub struct TemplateVars<'a> {
    /// Library root (`{root}`).
    pub root: &'a Path,
    /// The ROM's system dir (`{system_dir}`).
    pub system_dir: &'a Path,
    /// The ROM's folder relative to the system dir, empty at top level
    /// (`{rel_dir}`).
    pub rel_dir: &'a Path,
    /// ROM file name without extension (`{stem}`).
    pub stem: &'a str,
    /// Canonical system id, e.g. `snes` (`{system_id}`).
    pub system_id: &'a str,
    /// libretro playlist name (`{libretro}`).
    pub libretro: Option<&'a str>,
}

/// Renders a slot template into a path.
///
/// Templates always use `/` as separator. Tokens: `{root}`, `{system_dir}`,
/// `{rel_dir}`, `{stem}`, `{system_id}` and `{libretro}`. A segment made of
/// a path token alone is joined as a path, so an empty `{rel_dir}` (or a
/// missing `{libretro}`) leaves no empty or doubled separator; callers that
/// need `{libretro}` must check the system has one. Unknown tokens are kept
/// verbatim.
pub fn render(template: &str, vars: &TemplateVars) -> PathBuf {
    let mut out = PathBuf::new();
    if template.starts_with('/') {
        out.push("/");
    }
    for segment in template.split('/').filter(|s| !s.is_empty()) {
        match segment {
            "{root}" => out.push(vars.root),
            "{system_dir}" => out.push(vars.system_dir),
            "{rel_dir}" => out.push(vars.rel_dir),
            _ => {
                let text = substitute(segment, vars);
                if !text.is_empty() {
                    out.push(text);
                }
            }
        }
    }
    out
}

/// Replaces every token inside one path segment in a single pass, so values
/// (e.g. a stem containing `{stem}`) are never expanded again.
fn substitute(segment: &str, vars: &TemplateVars) -> String {
    let mut out = String::with_capacity(segment.len());
    let mut rest = segment;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(close) = after.find('}') else {
            rest = after;
            break;
        };
        let token = &after[..=close];
        if token[1..].contains('{') {
            // A stray `{`: keep it and look for a token after it.
            out.push('{');
            rest = &after[1..];
            continue;
        }
        match token {
            "{root}" => out.push_str(&vars.root.to_string_lossy()),
            "{system_dir}" => out.push_str(&vars.system_dir.to_string_lossy()),
            "{rel_dir}" => out.push_str(&vars.rel_dir.to_string_lossy()),
            "{stem}" => out.push_str(vars.stem),
            "{system_id}" => out.push_str(vars.system_id),
            "{libretro}" => out.push_str(vars.libretro.unwrap_or("")),
            unknown => out.push_str(unknown),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars<'a>(system_dir: &'a Path, rel_dir: &'a Path) -> TemplateVars<'a> {
        TemplateVars {
            root: Path::new("/sd"),
            system_dir,
            rel_dir,
            stem: "Zelda (USA)",
            system_id: "snes",
            libretro: Some("Nintendo - Super Nintendo Entertainment System"),
        }
    }

    #[test]
    fn renders_system_dir_and_stem() {
        let v = vars(Path::new("/sd/ROMS/SFC"), Path::new(""));
        assert_eq!(
            render("{system_dir}/Imgs/{stem}.png", &v),
            PathBuf::from("/sd/ROMS/SFC/Imgs/Zelda (USA).png")
        );
    }

    #[test]
    fn empty_rel_dir_collapses() {
        let v = vars(Path::new("/sd/ROMS/SFC"), Path::new(""));
        assert_eq!(
            render("{system_dir}/{rel_dir}/Imgs/{stem}.png", &v),
            PathBuf::from("/sd/ROMS/SFC/Imgs/Zelda (USA).png")
        );
    }

    #[test]
    fn rel_dir_mirrors_subfolders() {
        let v = vars(Path::new("/sd/ROMS/SFC"), Path::new("Hacks"));
        assert_eq!(
            render("{system_dir}/{rel_dir}/Imgs/{stem}.png", &v),
            PathBuf::from("/sd/ROMS/SFC/Hacks/Imgs/Zelda (USA).png")
        );
        let nested = vars(Path::new("/sd/ROMS/SFC"), Path::new("Hacks/Kaizo"));
        assert_eq!(
            render("{system_dir}/{rel_dir}/Imgs/{stem}.png", &nested),
            PathBuf::from("/sd/ROMS/SFC/Hacks/Kaizo/Imgs/Zelda (USA).png")
        );
    }

    #[test]
    fn renders_root_system_id_and_libretro() {
        let v = vars(Path::new("/sd/ROMs/snes"), Path::new("Hacks"));
        assert_eq!(
            render(
                "{root}/downloaded_media/{system_id}/covers/{rel_dir}/{stem}.png",
                &v
            ),
            PathBuf::from("/sd/downloaded_media/snes/covers/Hacks/Zelda (USA).png")
        );
        assert_eq!(
            render("{root}/thumbnails/{libretro}/Named_Boxarts/{stem}.png", &v),
            PathBuf::from(
                "/sd/thumbnails/Nintendo - Super Nintendo Entertainment System/Named_Boxarts/Zelda (USA).png"
            )
        );
    }

    #[test]
    fn tokens_inside_a_segment() {
        let v = vars(Path::new("/sd/roms/snes"), Path::new(""));
        assert_eq!(
            render("{system_dir}/images/{stem}-image.png", &v),
            PathBuf::from("/sd/roms/snes/images/Zelda (USA)-image.png")
        );
    }

    #[test]
    fn values_are_not_expanded_again() {
        let mut v = vars(Path::new("/sd/ROMS/SFC"), Path::new(""));
        v.stem = "Odd {system_id} {name";
        assert_eq!(
            render("{system_dir}/Imgs/{stem}.png", &v),
            PathBuf::from("/sd/ROMS/SFC/Imgs/Odd {system_id} {name.png")
        );
        assert_eq!(
            render("{system_dir}/x{a{stem}-{b", &v),
            PathBuf::from("/sd/ROMS/SFC/x{aOdd {system_id} {name-{b")
        );
    }

    #[test]
    fn unknown_tokens_are_kept_verbatim() {
        let v = vars(Path::new("/sd/ROMS/SFC"), Path::new(""));
        assert_eq!(
            render("{system_dir}/{nope}/{stem}.png", &v),
            PathBuf::from("/sd/ROMS/SFC/{nope}/Zelda (USA).png")
        );
    }
}
