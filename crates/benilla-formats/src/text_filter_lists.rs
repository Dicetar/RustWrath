//! `ChatProfanity.dbc` and `SpamMessages.dbc`, the lists behind 1.12's `profanityFilter` and
//! `spamFilter`, and `NamesProfanity.dbc` and `NamesReserved.dbc`, which the character name check
//! reads with `ChatProfanity`: an id and a plain-string pattern, one table for every language. The
//! reference compiles each row once at startup as a PCRE (`0x402b7f` → `0x6c91a0`; `0x71fba0` →
//! `pcre_compile` `0x720250`, CASELESS | UTF8 | NO_UTF8_CHECK) and skips one that fails, which no
//! shipped row does; `benilla_app::text_filter` compiles and applies them. All four come through
//! the patch chain: `dbc.MPQ` has 1512 `ChatProfanity` rows to `patch.MPQ`'s 2289, 943
//! `NamesProfanity` rows to `patch-2.MPQ`'s 1535, and no `SpamMessages`, and the names carry no
//! locale (`0x858348`, `0x859a84`, `0x859410`, `0x859458`), so a locale build overrides them at the
//! archive.

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::{parse, str_at, u32_at};

const CHAT_PROFANITY: &str = "DBFilesClient\\ChatProfanity.dbc";
const SPAM_MESSAGES: &str = "DBFilesClient\\SpamMessages.dbc";
const NAMES_PROFANITY: &str = "DBFilesClient\\NamesProfanity.dbc";
const NAMES_RESERVED: &str = "DBFilesClient\\NamesReserved.dbc";

/// One row: its id, which a failed compile names as the reference's error does, and its raw
/// PCRE pattern.
#[derive(Clone, Debug)]
pub struct FilterPattern {
    pub id: u32,
    pub pattern: String,
}

fn schema(name: &str, fields: Option<usize>) -> Schema {
    let mut s = Schema::new(name);
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Pattern", FieldType::String));
    // 1.12 (and Wrath's profanity/name lists) carry a trailing Language uint32; Wrath 3.3.5a's
    // `SpamMessages.dbc` dropped it and has 2 fields (`[ID, Pattern]`).
    if fields != Some(2) {
        s.add_field(SchemaField::new("Language", FieldType::UInt32));
    }
    s.set_key_field("ID");
    s
}

fn parse_rows(bytes: &[u8], name: &str) -> Result<Vec<FilterPattern>> {
    let fields = crate::dbc::field_count(bytes);
    let rs = parse(bytes, schema(name, fields), name)?;
    let mut out = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        // File order is the list order, which masking depends on: patterns apply in list order,
        // not text order. Never sort.
        let (Some(id), Some(pattern)) = (u32_at(r, 0), str_at(&rs, r, 1)) else {
            continue;
        };
        if pattern.is_empty() {
            continue;
        }
        out.push(FilterPattern { id, pattern });
    }
    Ok(out)
}

fn load(chain: &mut Chain, path: &str, name: &str) -> Result<Vec<FilterPattern>> {
    let bytes = chain
        .read_file(path)
        .with_context(|| format!("reading {name}.dbc"))?;
    parse_rows(&bytes, name)
}

/// `ChatProfanity.dbc`, the masker's list, in file order.
pub fn load_chat_profanity(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, CHAT_PROFANITY, "ChatProfanity")
}

/// `SpamMessages.dbc`, the spam predicate's list, in file order. The reference then scans a
/// second list the server can push (SMSG `0x332`, handler `0x49e6e0`); vmangos never sends one.
pub fn load_spam_messages(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, SPAM_MESSAGES, "SpamMessages")
}

/// `NamesProfanity.dbc`, the name check's first list (`0x6c9910`), in file order.
pub fn load_names_profanity(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, NAMES_PROFANITY, "NamesProfanity")
}

/// `NamesReserved.dbc`, the name check's third list (`0x6c99e8`), in file order.
pub fn load_names_reserved(chain: &mut Chain) -> Result<Vec<FilterPattern>> {
    load(chain, NAMES_RESERVED, "NamesReserved")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_filter_dbc(rows: &[(u32, &str, Option<u32>)], three_fields: bool) -> Vec<u8> {
        let field_count = if three_fields { 3u32 } else { 2u32 };
        let mut strings = vec![0u8];
        let mut records = Vec::new();
        for (id, pattern, lang) in rows {
            let str_off = if pattern.is_empty() {
                0u32
            } else {
                let off = strings.len() as u32;
                strings.extend_from_slice(pattern.as_bytes());
                strings.push(0);
                off
            };
            records.extend_from_slice(&id.to_le_bytes());
            records.extend_from_slice(&str_off.to_le_bytes());
            if three_fields {
                records.extend_from_slice(&lang.unwrap_or(0).to_le_bytes());
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"WDBC");
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        out.extend_from_slice(&field_count.to_le_bytes());
        out.extend_from_slice(&(field_count * 4).to_le_bytes());
        out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        out.extend_from_slice(&records);
        out.extend_from_slice(&strings);
        out
    }

    #[test]
    fn two_field_wrath_and_three_field_donor_filter_dbcs_both_parse() {
        let two_field = synthetic_filter_dbc(&[(10, r"gold\s*4\s*u", None), (11, "", None)], false);
        let parsed_two = parse_rows(&two_field, "SpamMessages").expect("2-field SpamMessages");
        assert_eq!(parsed_two.len(), 1);
        assert_eq!(parsed_two[0].id, 10);
        assert_eq!(parsed_two[0].pattern, r"gold\s*4\s*u");

        let three_field =
            synthetic_filter_dbc(&[(42, r"\<twat\>", Some(0))], true);
        let parsed_three = parse_rows(&three_field, "SpamMessages").expect("3-field SpamMessages");
        assert_eq!(parsed_three.len(), 1);
        assert_eq!(parsed_three[0].id, 42);
        assert_eq!(parsed_three[0].pattern, r"\<twat\>");
    }

    /// The row counts the reference's PCRE compiles, which only the patched archives give.
    #[test]
    fn the_lists_come_off_the_priority_walk_at_their_patched_sizes() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let is_donor = crate::is_donor_install(&data);

        let profanity = load_chat_profanity(&mut chain).expect("load ChatProfanity");
        if is_donor {
            assert_eq!(
                profanity.len(),
                2289,
                "2289 is patch.MPQ's copy; 1512 would mean we read dbc.MPQ"
            );
        } else {
            assert!(!profanity.is_empty(), "Wrath ChatProfanity has rows");
        }
        assert!(
            profanity.iter().any(|p| p.pattern == r"\<twat\>"),
            "the ASCII half uses the \\< \\> word-boundary anchors"
        );
        assert!(
            profanity.iter().any(|p| !p.pattern.is_ascii()),
            "the list is multi-language in one table"
        );

        let names = load_names_profanity(&mut chain).expect("load NamesProfanity");
        let reserved = load_names_reserved(&mut chain).expect("load NamesReserved");
        let spam = load_spam_messages(&mut chain).expect("load SpamMessages");
        if is_donor {
            assert_eq!(
                names.len(),
                1535,
                "1535 is patch-2.MPQ's copy; 943 would mean we read dbc.MPQ"
            );
            assert_eq!(reserved.len(), 2288, "patch.MPQ's copy");
            assert_eq!(spam.len(), 28, "dbc.MPQ has no SpamMessages at all");
            assert!(
                spam.iter().all(|p| p.pattern.contains(r"\s*")),
                "every shipped spam row spaces its letters to defeat spacing"
            );
        } else {
            assert!(!names.is_empty(), "Wrath NamesProfanity has rows");
            assert!(!reserved.is_empty(), "Wrath NamesReserved has rows");
            assert!(!spam.is_empty(), "Wrath SpamMessages has rows");
        }
    }
}
