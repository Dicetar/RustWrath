//! Resource-bounded JSON boundary shared by offline artifact and placement readers.
//!
//! A streaming structural pass precedes typed deserialization. It rejects duplicate
//! keys (including escaped aliases), excessive nesting and oversized collections before
//! the typed reader allocates its vectors. Only bounded key sets/parser scratch are retained,
//! not a complete value tree. Semantic limits remain the owning type's job.

use std::collections::BTreeSet;
use std::fmt;
use serde::de::{DeserializeOwned, DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, Serialize};
use crate::ValidationError;

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODES: usize = 4_300_000;
const MAX_STRING_BYTES: usize = MAX_BYTES;

struct Budget { nodes: usize, strings: usize }
struct Scan<'a> { budget: &'a mut Budget, depth: usize }
struct RejectElement;
impl<'de> DeserializeSeed<'de> for RejectElement {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(D::Error::custom("JSON array budget exceeded"))
    }
}

impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        if self.depth > 16 || self.budget.nodes == MAX_NODES {
            return Err(D::Error::custom("JSON depth/node budget exceeded"));
        }
        self.budget.nodes += 1;
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<(), E> { Ok(()) }
    fn visit_i64<E: Error>(self, _: i64) -> Result<(), E> { Ok(()) }
    fn visit_u64<E: Error>(self, _: u64) -> Result<(), E> { Ok(()) }
    fn visit_f64<E: Error>(self, v: f64) -> Result<(), E> {
        if v.is_finite() { Ok(()) } else { Err(E::custom("non-finite number")) }
    }
    fn visit_unit<E: Error>(self) -> Result<(), E> { Ok(()) }
    fn visit_str<E: Error>(self, v: &str) -> Result<(), E> {
        if v.len() > 1024 || self.budget.strings + v.len() > MAX_STRING_BYTES {
            return Err(E::custom("JSON string budget exceeded"));
        }
        self.budget.strings += v.len();
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut count = 0;
        loop {
            // Permit reading the end marker at the exact limit, but never another value.
            if count == 1_048_576 {
                seq.next_element_seed(RejectElement)?;
                break;
            }
            if seq.next_element_seed(Scan { budget: &mut *self.budget, depth: self.depth + 1 })?.is_none() {
                break;
            }
            count += 1;
        }
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if key.len() > 128 || keys.len() == 64 || !keys.insert(key.clone()) {
                return Err(A::Error::custom("duplicate/oversized JSON object key"));
            }
            self.budget.strings += key.len();
            if self.budget.strings > MAX_STRING_BYTES {
                return Err(A::Error::custom("JSON string budget exceeded"));
            }
            map.next_value_seed(Scan { budget: &mut *self.budget, depth: self.depth + 1 })?;
        }
        Ok(())
    }
}

fn error(e: impl fmt::Display) -> ValidationError {
    ValidationError::new("json", &e.to_string())
}

/// Decode only after the whole input passes resource and duplicate-key checks.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ValidationError> {
    if bytes.len() > MAX_BYTES { return Err(error("maximum JSON size is 64 MiB")); }
    let mut budget = Budget { nodes: 0, strings: 0 };
    let mut de = serde_json::Deserializer::from_slice(bytes);
    Scan { budget: &mut budget, depth: 0 }.deserialize(&mut de).map_err(error)?;
    de.end().map_err(error)?;
    serde_json::from_slice(bytes).map_err(error)
}

struct LimitedWriter(Vec<u8>);
impl std::io::Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("maximum JSON size is 64 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

/// Encode into a capped buffer. The caller owns any filesystem operations.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ValidationError> {
    let mut writer = LimitedWriter(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(error)?;
    // Apply exactly the same envelope budgets to producer and consumer output.
    let mut budget = Budget { nodes: 0, strings: 0 };
    let mut de = serde_json::Deserializer::from_slice(&writer.0);
    Scan { budget: &mut budget, depth: 0 }.deserialize(&mut de).map_err(error)?;
    de.end().map_err(error)?;
    Ok(writer.0)
}
