//! Bounded JSON decoding shared by local and remote audience transports.
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize,
};
use serde_json::{json, Value};
use std::fmt;
const MAX_BYTES: usize = 4096;

// Reject duplicate keys at every depth before typed deserialization. serde_json's
// ordinary Value parser would overwrite them, hiding malformed relay messages.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with unique keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(json!(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut map = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if map.contains_key(&k) {
                        return Err(de::Error::custom("Duplicate JSON key"));
                    }
                    map.insert(k, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(map.into()))
            }
        }
        d.deserialize_any(V)
    }
}
pub(crate) fn strict_json(bytes: &[u8]) -> Result<Value, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("Relay message exceeds size limit.");
    }
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|_| "Malformed relay JSON.")
}
