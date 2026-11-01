//! Streaming reducer of a `blawar/titledb` region file (adjustments D5).
//!
//! The file is a top-level map `nsuId → entry`. Per entry only `id`,
//! `name`, `publisher`, `releaseDate`, `region` and (with icons on) an
//! eShop `iconUrl` are kept; every other field (`rightsId`, `description`,
//! `screenshots`, ratings…) is skipped as [`IgnoredAny`] and never held.
//! `key` is only tested for null: a non-null `key` abandons the whole
//! import (§13 Q11) without its content ever being kept. Errors carry only
//! a byte offset and a field name, never catalogue text (D1).

use std::fmt;
use std::io::{BufReader, Read};

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use super::{Control, Fail, Stop};
use crate::net::hosts::ESHOP_ICONS;

/// Longest name, publisher or region kept, in characters.
pub(crate) const MAX_TEXT_CHARS: usize = 256;

/// One kept entry of a region file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TitleRow {
    pub id: u64,
    pub name: String,
    pub publisher: Option<String>,
    pub release: Option<String>,
    pub region: Option<String>,
    pub icon_url: Option<String>,
}

/// Reduces a region file read from `reader`, handing each kept row to
/// `sink` in file order.
pub(crate) fn reduce(
    reader: &mut dyn Read,
    keep_icons: bool,
    ctl: &Control,
    sink: &mut dyn FnMut(TitleRow) -> Result<(), Stop>,
) -> Result<(), Fail> {
    let mut de = serde_json::Deserializer::from_reader(BufReader::new(reader));
    let top = Top {
        keep_icons,
        ctl,
        sink,
    };
    let parsed = de.deserialize_map(top).and_then(|()| de.end());
    ctl.finish(parsed)
}

struct Top<'a, 'c> {
    keep_icons: bool,
    ctl: &'a Control<'c>,
    sink: &'a mut dyn FnMut(TitleRow) -> Result<(), Stop>,
}

impl<'de> Visitor<'de> for Top<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a map of titles")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        loop {
            self.ctl.check::<A::Error>()?;
            self.ctl.field.set(None);
            if map.next_key::<IgnoredAny>()?.is_none() {
                return Ok(());
            }
            let entry = map.next_value_seed(EntrySeed {
                keep_icons: self.keep_icons,
                ctl: self.ctl,
            })?;
            if let Some(row) = entry {
                (self.sink)(row).map_err(|stop| self.ctl.stop(stop))?;
            }
        }
    }
}

struct EntrySeed<'a, 'c> {
    keep_icons: bool,
    ctl: &'a Control<'c>,
}

impl<'de> DeserializeSeed<'de> for EntrySeed<'_, '_> {
    type Value = Option<TitleRow>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for EntrySeed<'_, '_> {
    type Value = Option<TitleRow>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a title entry")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut id = None;
        let mut name = None;
        let mut publisher = None;
        let mut release = None;
        let mut region = None;
        let mut icon = None;
        while let Some(field) = map.next_key::<Field>()? {
            self.ctl.field.set(field.name());
            match field {
                Field::Id => id = map.next_value::<Lenient>()?.into_str(),
                Field::Name => name = map.next_value::<Lenient>()?.into_str(),
                Field::Publisher => publisher = map.next_value::<Lenient>()?.into_str(),
                Field::ReleaseDate => release = map.next_value::<Lenient>()?.into_uint(),
                Field::Region => region = map.next_value::<Lenient>()?.into_str(),
                Field::IconUrl if self.keep_icons => icon = map.next_value::<Lenient>()?.into_str(),
                Field::Key => {
                    if map.next_value::<Nullness>()? == Nullness::NotNull {
                        return Err(self.ctl.stop(Stop::Rejected));
                    }
                }
                Field::IconUrl | Field::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        self.ctl.field.set(None);
        let Some(id) = id.as_deref().and_then(parse_title_id) else {
            return Ok(None);
        };
        let Some(name) = name.as_deref().and_then(clean_text) else {
            return Ok(None);
        };
        Ok(Some(TitleRow {
            id,
            name,
            publisher: publisher.as_deref().and_then(clean_text),
            release: release.and_then(iso_date),
            region: region.as_deref().and_then(clean_text),
            icon_url: icon.filter(|u| is_eshop_icon(u)),
        }))
    }

    // An entry that is not an object is skipped, not an error.
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }
}

/// The fields of an entry this reducer looks at; anything else is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Id,
    Name,
    Publisher,
    ReleaseDate,
    Region,
    IconUrl,
    Key,
    Other,
}

impl Field {
    fn name(self) -> Option<&'static str> {
        Some(match self {
            Field::Id => "id",
            Field::Name => "name",
            Field::Publisher => "publisher",
            Field::ReleaseDate => "releaseDate",
            Field::Region => "region",
            Field::IconUrl => "iconUrl",
            Field::Key => "key",
            Field::Other => return None,
        })
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Field;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a field name")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Field, E> {
                Ok(match v {
                    "id" => Field::Id,
                    "name" => Field::Name,
                    "publisher" => Field::Publisher,
                    "releaseDate" => Field::ReleaseDate,
                    "region" => Field::Region,
                    "iconUrl" => Field::IconUrl,
                    "key" => Field::Key,
                    _ => Field::Other,
                })
            }
        }
        d.deserialize_identifier(V)
    }
}

/// A value read without failing on an unexpected type: strings and
/// unsigned integers are kept, everything else is drained and dropped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lenient {
    Str(String),
    Uint(u64),
    Null,
    Other,
}

impl Lenient {
    pub(crate) fn into_str(self) -> Option<String> {
        match self {
            Lenient::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn into_uint(self) -> Option<u64> {
        match self {
            Lenient::Uint(n) => Some(n),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Lenient {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Lenient;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any value")
            }
            fn visit_str<E>(self, v: &str) -> Result<Lenient, E> {
                Ok(Lenient::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> Result<Lenient, E> {
                Ok(Lenient::Str(v))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Lenient, E> {
                Ok(Lenient::Uint(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Lenient, E> {
                Ok(u64::try_from(v).map_or(Lenient::Other, Lenient::Uint))
            }
            fn visit_f64<E>(self, _: f64) -> Result<Lenient, E> {
                Ok(Lenient::Other)
            }
            fn visit_bool<E>(self, _: bool) -> Result<Lenient, E> {
                Ok(Lenient::Other)
            }
            fn visit_unit<E>(self) -> Result<Lenient, E> {
                Ok(Lenient::Null)
            }
            fn visit_none<E>(self) -> Result<Lenient, E> {
                Ok(Lenient::Null)
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Lenient, D::Error> {
                Lenient::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Lenient, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Lenient::Other)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Lenient, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Lenient::Other)
            }
        }
        d.deserialize_any(V)
    }
}

/// Whether a value is JSON `null`; its content, if any, is never kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nullness {
    Null,
    NotNull,
}

impl<'de> Deserialize<'de> for Nullness {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Lenient::deserialize(d)? {
            Lenient::Null => Nullness::Null,
            _ => Nullness::NotNull,
        })
    }
}

/// Exactly 16 hex digits.
pub(crate) fn parse_title_id(s: &str) -> Option<u64> {
    (s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u64::from_str_radix(s, 16).ok())
        .flatten()
}

/// Untrusted catalogue text made safe to show: control characters and the
/// bidi embeddings, overrides and isolates removed, NFC, trimmed, capped
/// at [`MAX_TEXT_CHARS`]. Empty → `None`.
pub(crate) fn clean_text(s: &str) -> Option<String> {
    let cleaned: String = s
        .chars()
        .filter(|&c| !c.is_control() && !is_bidi_control(c))
        .nfc()
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(MAX_TEXT_CHARS)
            .collect::<String>()
            .trim_end()
            .to_owned(),
    )
}

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// An integer `YYYYMMDD` as `YYYY-MM-DD`; an impossible date is `None`.
pub(crate) fn iso_date(n: u64) -> Option<String> {
    let (year, month, day) = (n / 10_000, n / 100 % 100, n % 100);
    if !(1970..=2999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days)
        .contains(&day)
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// An eShop icon URL on the one allowed host, with no spaces or controls.
fn is_eshop_icon(url: &str) -> bool {
    url.starts_with(ESHOP_ICONS)
        && url.len() <= 512
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(iso_date(20200315).as_deref(), Some("2020-03-15"));
        assert_eq!(iso_date(20240229).as_deref(), Some("2024-02-29"));
        assert_eq!(iso_date(20230229), None);
        assert_eq!(iso_date(20211399), None);
        assert_eq!(iso_date(20210400), None);
        assert_eq!(iso_date(0), None);
        assert_eq!(iso_date(u64::MAX), None);
    }

    #[test]
    fn ids() {
        assert_eq!(
            parse_title_id("0100000000aa0000"),
            Some(0x0100_0000_00AA_0000)
        );
        assert_eq!(parse_title_id("0100000000AA000"), None);
        assert_eq!(parse_title_id("0100000000AA00000"), None);
        assert_eq!(parse_title_id("+100000000AA0000"), None);
        assert_eq!(parse_title_id("0100XYZ000AA0000"), None);
    }

    #[test]
    fn text() {
        assert_eq!(clean_text("  a\u{2066}b\u{202E}c\n "), Some("abc".into()));
        assert_eq!(clean_text("\u{0007}\u{202A}"), None);
        assert_eq!(clean_text("e\u{301}").as_deref(), Some("\u{e9}"));
        let long = "x".repeat(1000);
        assert_eq!(clean_text(&long).unwrap().chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn icon_host() {
        assert!(is_eshop_icon("https://img-eshop.cdn.nintendo.net/i/a.jpg"));
        assert!(!is_eshop_icon(
            "https://img-eshop.cdn.nintendo.net.evil/i/a.jpg"
        ));
        assert!(!is_eshop_icon("http://img-eshop.cdn.nintendo.net/i/a.jpg"));
        assert!(!is_eshop_icon(
            "https://img-eshop.cdn.nintendo.net/i/a b.jpg"
        ));
    }
}
