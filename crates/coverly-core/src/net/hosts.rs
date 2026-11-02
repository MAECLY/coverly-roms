//! Every network endpoint Coverly may reach, in one registry (spec D30).
//! `UreqHttp` refuses any HTTPS URL that does not start with one of these
//! prefixes or with one of the user's own URLs (the nlib fallback, an
//! update endpoint override). Prefixes are matched by path, not host,
//! because `raw.githubusercontent.com` serves several datasets. The credits
//! test checks that every entry with a credit names an existing credit.

/// One allowed URL prefix and the credit entry that covers it (spec D30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub prefix: &'static str,
    pub credit: Option<&'static str>,
}

pub const LIBRETRO_THUMBNAILS: &str = "https://thumbnails.libretro.com/";
pub const LIBRETRO_THUMBNAILS_RAW: &str = "https://raw.githubusercontent.com/libretro-thumbnails/";
pub const LIBRETRO_DATABASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/";
pub const TITLEDB: &str = "https://raw.githubusercontent.com/blawar/titledb/";
pub const STEAMGRIDDB: &str = "https://www.steamgriddb.com/";
pub const STEAMGRIDDB_CDN: &str = "https://cdn2.steamgriddb.com/";
pub const SCREENSCRAPER: &str = "https://api.screenscraper.fr/";
pub const ESHOP_ICONS: &str = "https://img-eshop.cdn.nintendo.net/";
pub const NLIB: &str = "https://api.nlib.cc/";
pub const MAECLY_GITHUB: &str = "https://github.com/MAECLY/";

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        prefix: LIBRETRO_THUMBNAILS,
        credit: Some("libretro-thumbnails"),
    },
    Endpoint {
        prefix: LIBRETRO_THUMBNAILS_RAW,
        credit: Some("libretro-thumbnails"),
    },
    Endpoint {
        prefix: LIBRETRO_DATABASE,
        credit: Some("libretro-database"),
    },
    Endpoint {
        prefix: TITLEDB,
        credit: Some("titledb"),
    },
    Endpoint {
        prefix: STEAMGRIDDB,
        credit: Some("steamgriddb"),
    },
    Endpoint {
        prefix: STEAMGRIDDB_CDN,
        credit: Some("steamgriddb"),
    },
    Endpoint {
        prefix: SCREENSCRAPER,
        credit: Some("screenscraper"),
    },
    Endpoint {
        prefix: ESHOP_ICONS,
        credit: Some("nintendo-eshop-icons"),
    },
    Endpoint {
        prefix: NLIB,
        credit: Some("nlib"),
    },
    // The updater: Coverly's own releases, nothing to credit.
    Endpoint {
        prefix: MAECLY_GITHUB,
        credit: None,
    },
];

// Base URLs the providers build on. Each starts with a registered prefix
// (tested below); they live here so no other source file holds a URL.
pub const LIBRETRO_BASE: &str = "https://thumbnails.libretro.com";
pub const LIBRETRO_RAW_BASE: &str = "https://raw.githubusercontent.com/libretro-thumbnails";
pub const DATABASE_BASE: &str =
    "https://raw.githubusercontent.com/libretro/libretro-database/master";
/// The `blawar/titledb` files (region files and `ncas.json`, spec D3, D6).
pub const TITLEDB_BASE: &str = "https://raw.githubusercontent.com/blawar/titledb/master";
pub const STEAMGRIDDB_BASE: &str = "https://www.steamgriddb.com/api/v2";
pub const SCREENSCRAPER_BASE: &str = "https://api.screenscraper.fr/api2";
/// Default base of the optional nlib fallback (adjustments D9); the user
/// may replace it in the settings.
pub const NLIB_BASE: &str = "https://api.nlib.cc";
pub const UPDATE_STABLE: &str =
    "https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json";
pub const UPDATE_BETA: &str =
    "https://github.com/MAECLY/coverly-roms/releases/download/channel-beta/latest.json";

/// Whether `url` may be fetched: it starts with a registered prefix, or
/// with one of the user's own HTTPS URLs (settings already validated them;
/// an entry that is not a plain HTTPS URL is ignored). A user URL matches
/// only on a boundary (`/`, `?`, `#` or the end), so `https://a.example`
/// never admits `https://a.example.evil/`. Any `.` or `..` path segment
/// (also percent-encoded) is refused, so a prefix cannot be walked out of.
pub fn allowed(url: &str, user: &[&str]) -> bool {
    if !super::plain_authority(url) || has_dot_segment(url) {
        return false;
    }
    ENDPOINTS.iter().any(|e| url.starts_with(e.prefix))
        || user.iter().any(|u| user_prefix_matches(u.trim(), url))
}

/// A user base URL the settings may store (the nlib fallback): plain
/// `https://host[:port][/path]` with no spaces, control characters, query,
/// fragment, credentials, backslashes or dot / encoded separator segments,
/// so every URL built on it passes [`allowed`].
pub fn is_user_base(url: &str) -> bool {
    url.starts_with("https://")
        && super::plain_authority(url)
        && !has_dot_segment(url)
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\'))
}

fn user_prefix_matches(prefix: &str, url: &str) -> bool {
    if !prefix.starts_with("https://")
        || !super::plain_authority(prefix)
        || prefix.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let Some(rest) = url.strip_prefix(prefix) else {
        return false;
    };
    prefix.ends_with('/') || rest.is_empty() || rest.starts_with(['/', '?', '#'])
}

fn has_dot_segment(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = rest.split(['?', '#']).next().unwrap_or("");
    // HTTP clients treat `\` as `/`, and some servers decode `%2F`, `%5C`
    // or a doubly encoded `%252E` before resolving dot segments, so each
    // segment is decoded until it stops changing: a separator or a dot
    // segment at any depth is refused. A plain `%25` (a `%` in a game
    // name, «Asuka 120% Special») decodes to neither.
    if path.contains('\\') {
        return true;
    }
    path.split('/').skip(1).any(|seg| {
        let mut seg = seg.to_string();
        for _ in 0..MAX_DECODES {
            let decoded = percent_encoding::percent_decode_str(&seg)
                .decode_utf8_lossy()
                .into_owned();
            if decoded.contains(['/', '\\']) || decoded == "." || decoded == ".." {
                return true;
            }
            if decoded == seg {
                return false;
            }
            seg = decoded;
        }
        // Still encoded after that many rounds: no plain name is.
        true
    })
}

/// Rounds of percent-decoding [`has_dot_segment`] looks through.
const MAX_DECODES: usize = 4;

/// `scheme://authority/` of an HTTPS URL, used to admit the user's update
/// endpoint override and the artifacts its manifest points to on that host.
pub fn https_origin(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let origin = format!("https://{authority}/");
    (!authority.is_empty() && super::plain_authority(&origin)).then_some(origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_or_backslash_separators_cannot_walk_out_of_a_prefix() {
        let base = LIBRETRO_THUMBNAILS_RAW;
        assert!(allowed(
            &format!("{base}Nintendo_-_SNES/master/Named_Boxarts/Dr.%20Mario.png"),
            &[]
        ));
        for tail in [
            "x/..%2f..%2fother/repo/a.png",
            "x/..%2F..%2Fother/repo/a.png",
            "x\\..\\..\\other/repo/a.png",
            "x/..%5c..%5cother/repo/a.png",
            "x/%252e%252e/other/a.png",
            "x/%2e%2e/other/a.png",
        ] {
            assert!(!allowed(&format!("{base}{tail}"), &[]), "{tail}");
        }
    }

    #[test]
    fn a_percent_sign_in_a_game_name_is_no_separator() {
        // «Asuka 120% Special», «100% Star»: libretro names keep `%`, so
        // the encoded URL holds `%25` that decodes to no dot or separator.
        let name = crate::sources::libretro::encode_segment("Asuka 120% Special (Japan).png");
        assert!(name.contains("%25"), "{name}");
        let base = LIBRETRO_THUMBNAILS_RAW;
        assert!(allowed(
            &format!("{base}Sony_-_PlayStation/master/Named_Boxarts/{name}"),
            &[]
        ));
        assert!(allowed(
            &format!("{LIBRETRO_THUMBNAILS}Sega%20-%20Saturn/Named_Boxarts/100%25%20Star.png"),
            &[]
        ));
        // Doubly encoded separators and dots still walk nowhere.
        for tail in [
            "x/%252f..%252fother/a.png",
            "x/%255c/a.png",
            "x/%25252e%25252e/a.png",
        ] {
            assert!(!allowed(&format!("{base}{tail}"), &[]), "{tail}");
        }
    }

    #[test]
    fn prefixes_are_https_unique_and_end_with_a_slash() {
        for (i, e) in ENDPOINTS.iter().enumerate() {
            assert!(e.prefix.starts_with("https://"), "{}", e.prefix);
            assert!(e.prefix.ends_with('/'), "{}", e.prefix);
            assert!(
                ENDPOINTS[..i].iter().all(|o| o.prefix != e.prefix),
                "{} twice",
                e.prefix
            );
        }
    }

    #[test]
    fn every_base_url_is_registered() {
        for base in [
            LIBRETRO_BASE,
            LIBRETRO_RAW_BASE,
            DATABASE_BASE,
            TITLEDB_BASE,
            STEAMGRIDDB_BASE,
            SCREENSCRAPER_BASE,
            NLIB_BASE,
        ] {
            assert!(allowed(&format!("{base}/x"), &[]), "{base}");
        }
        assert!(allowed(UPDATE_STABLE, &[]));
        assert!(allowed(UPDATE_BETA, &[]));
    }

    #[test]
    fn prefixes_match_by_path_not_host() {
        assert!(allowed(
            "https://raw.githubusercontent.com/blawar/titledb/master/ncas.json",
            &[]
        ));
        assert!(!allowed(
            "https://raw.githubusercontent.com/blawar/other/master/x",
            &[]
        ));
        assert!(!allowed("https://raw.githubusercontent.com/", &[]));
        assert!(!allowed("https://thumbnails.libretro.com", &[]));
        assert!(!allowed("https://thumbnails.libretro.com.evil/x", &[]));
        assert!(!allowed("https://user@thumbnails.libretro.com/x", &[]));
    }

    #[test]
    fn dot_segments_are_refused() {
        for url in [
            "https://raw.githubusercontent.com/blawar/titledb/../x/y",
            "https://raw.githubusercontent.com/blawar/titledb/%2E%2E/x",
            "https://raw.githubusercontent.com/blawar/titledb/./x",
        ] {
            assert!(!allowed(url, &[]), "{url}");
        }
        assert!(allowed(
            "https://thumbnails.libretro.com/a..b/c.png?x=..",
            &[]
        ));
    }

    #[test]
    fn user_urls_match_on_a_boundary_and_only_over_https() {
        let user = ["https://mirror.example/api", "http://plain.example/"];
        assert!(allowed("https://mirror.example/api", &user));
        assert!(allowed("https://mirror.example/api/0100", &user));
        assert!(allowed("https://mirror.example/api?x=1", &user));
        assert!(!allowed("https://mirror.example/apix", &user));
        assert!(!allowed("https://mirror.example.evil/api", &user));
        assert!(!allowed("http://plain.example/x", &user));
        assert!(!allowed("https://plain.example/x", &user));
    }

    #[test]
    fn origin_of_an_https_url() {
        assert_eq!(
            https_origin("https://dl.example:8443/a/latest.json").as_deref(),
            Some("https://dl.example:8443/")
        );
        assert_eq!(https_origin("http://dl.example/a"), None);
        assert_eq!(https_origin("https://"), None);
        assert_eq!(https_origin("https://a@b/x"), None);
    }
}
