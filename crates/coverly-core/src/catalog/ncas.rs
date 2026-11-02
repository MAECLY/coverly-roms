//! Streaming reducer of `blawar/titledb`'s `ncas.json` (adjustments D6).
//!
//! The file is a top-level map `<32-hex NCA id> → { titleId, contentType,
//! … }` (field names checked against the live file on 2026-11-01:
//! `buildId`, `contentIndex`, `contentType`, `cryptoType`, `cryptoType2`,
//! `isGameCard`, `keyIndex`, `rightsId`, `sdkVersion`, `size`, `titleId`).
//! Only `titleId` and `contentType` are kept; `rightsId` and the rest are
//! skipped as [`IgnoredAny`]. A row with a bad NCA id, title id or content
//! type is skipped.

use std::fmt;
use std::io::{BufReader, Read};

use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

use super::titledb::{parse_title_id, Lenient};
use super::{Control, Fail, Stop};

/// One kept row of `ncas.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NcaRow {
    pub nca: [u8; 16],
    pub title: u64,
    pub content_type: u8,
}

pub(crate) fn reduce(
    reader: &mut dyn Read,
    ctl: &Control,
    sink: &mut dyn FnMut(NcaRow) -> Result<(), Stop>,
) -> Result<(), Fail> {
    let mut de = serde_json::Deserializer::from_reader(BufReader::new(reader));
    let parsed = de
        .deserialize_map(Top { ctl, sink })
        .and_then(|()| de.end());
    ctl.finish(parsed)
}

/// `00…ff` × 16, either case.
pub(crate) fn parse_nca_id(s: &str) -> Option<[u8; 16]> {
    let b = s.as_bytes();
    if b.len() != 32 || !b.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, pair) in b.as_chunks::<2>().0.iter().enumerate() {
        let hex = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(out)
}

struct Top<'a, 'c> {
    ctl: &'a Control<'c>,
    sink: &'a mut dyn FnMut(NcaRow) -> Result<(), Stop>,
}

/// A top-level key: an NCA id or something to skip.
struct NcaKey(Option<[u8; 16]>);

impl<'de> Deserialize<'de> for NcaKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = NcaKey;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an NCA id")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<NcaKey, E> {
                Ok(NcaKey(parse_nca_id(v)))
            }
        }
        d.deserialize_identifier(V)
    }
}

impl<'de> Visitor<'de> for Top<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a map of NCA ids")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        loop {
            self.ctl.check::<A::Error>()?;
            self.ctl.field.set(None);
            let Some(NcaKey(nca)) = map.next_key::<NcaKey>()? else {
                return Ok(());
            };
            let Some(nca) = nca else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            let value = map.next_value_seed(EntrySeed { ctl: self.ctl })?;
            if let Some((title, content_type)) = value {
                (self.sink)(NcaRow {
                    nca,
                    title,
                    content_type,
                })
                .map_err(|stop| self.ctl.stop(stop))?;
            }
        }
    }
}

struct EntrySeed<'a, 'c> {
    ctl: &'a Control<'c>,
}

impl<'de> DeserializeSeed<'de> for EntrySeed<'_, '_> {
    type Value = Option<(u64, u8)>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for EntrySeed<'_, '_> {
    type Value = Option<(u64, u8)>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an NCA entry")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut title = None;
        let mut content_type = None;
        while let Some(field) = map.next_key::<Field>()? {
            match field {
                Field::TitleId => {
                    self.ctl.field.set(Some("titleId"));
                    title = map.next_value::<Lenient>()?.into_str();
                }
                Field::ContentType => {
                    self.ctl.field.set(Some("contentType"));
                    content_type = map.next_value::<Lenient>()?.into_uint();
                }
                Field::Other => {
                    self.ctl.field.set(None);
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        self.ctl.field.set(None);
        let title = title.as_deref().and_then(parse_title_id);
        let content_type = content_type.and_then(|n| u8::try_from(n).ok());
        Ok(title.zip(content_type))
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    TitleId,
    ContentType,
    Other,
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
                    "titleId" => Field::TitleId,
                    "contentType" => Field::ContentType,
                    _ => Field::Other,
                })
            }
        }
        d.deserialize_identifier(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nca_ids() {
        let mut want = [0u8; 16];
        want[0] = 0xab;
        want[15] = 0x01;
        assert_eq!(parse_nca_id("ab000000000000000000000000000001"), Some(want));
        assert_eq!(parse_nca_id("AB000000000000000000000000000001"), Some(want));
        assert_eq!(parse_nca_id("ab00000000000000000000000000001"), None);
        assert_eq!(parse_nca_id("zz000000000000000000000000000001"), None);
        assert_eq!(parse_nca_id("+b000000000000000000000000000001"), None);
    }
}
