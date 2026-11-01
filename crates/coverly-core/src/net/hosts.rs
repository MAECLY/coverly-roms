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
pub const STEAMGRIDDB_BASE: &str = "https://www.steamgriddb.com/api/v2";
pub const SCREENSCRAPER_BASE: &str = "https://api.screenscraper.fr/api2";
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
    // HTTP clients treat `\` as `/` and some servers decode `%2F`, `%5C`
    // or a double-encoded `%252E` before resolving dot segments, so any of
    // them could walk out of a prefix the plain split below never sees.
    let lower = path.to_ascii_lowercase();
    if path.contains('\\') || ["%2f", "%5c", "%25"].iter().any(|e| lower.contains(e)) {
        return true;
    }
    path.split('/').skip(1).any(|seg| {
        let seg = seg.to_ascii_lowercase().replace("%2e", ".");
        seg == "." || seg == ".."
    })
}

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
            STEAMGRIDDB_BASE,
            SCREENSCRAPER_BASE,
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
