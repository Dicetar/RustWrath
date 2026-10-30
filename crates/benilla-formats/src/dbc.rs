//! Shared helpers for the typed DBC loaders. A DBC carries no column types, so each loader supplies
//! a [`Schema`] whose field count must match the file header, and reads fields by index.

use std::collections::HashMap;
use std::io::Cursor;

use anyhow::{anyhow, Context, Result};
use benilla_dbc::{DbcParser, FieldType, Record, RecordSet, Schema, SchemaField, StringRef, Value};

use crate::{Chain, Locale};

/// Parse a DBC with `schema`; `what` names the file in errors.
pub(crate) fn parse(bytes: &[u8], schema: Schema, what: &str) -> Result<RecordSet> {
    let parser = DbcParser::parse(&mut Cursor::new(bytes))
        .map_err(|e| anyhow!("parsing {what} header: {e}"))?;
    let parser = parser
        .with_schema(schema)
        .map_err(|e| anyhow!("applying {what} schema (field-count mismatch?): {e}"))?;
    parser
        .parse_records()
        .map_err(|e| anyhow!("parsing {what} records: {e}"))
}

/// Parse a DBC whose 3.3.5 rows extend a 1.12 schema with trailing columns. The leading columns
/// keep their 1.12 meaning; the extra columns are read as opaque slots. This is deliberately
/// opt-in per loader, so an ordinary schema mismatch still fails loudly.
pub(crate) fn parse_with_trailing_fields(
    bytes: &[u8],
    mut schema: Schema,
    what: &str,
) -> Result<RecordSet> {
    let fields = bytes
        .get(8..12)
        .and_then(|raw| raw.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| anyhow!("parsing {what} header: truncated field count"))?
        as usize;
    while schema.fields.len() < fields {
        let index = schema.fields.len();
        schema.add_field(SchemaField::new(format!("Unknown{index}"), FieldType::UInt32));
    }
    parse(bytes, schema, what)
}

/// The 4-byte field count a DBC header declares, for a loader that has a known 3.3.5 layout.
pub(crate) fn field_count(bytes: &[u8]) -> Option<usize> {
    Some(
        u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?) as usize,
    )
}

/// An unsigned field, from either int variant: the schema tag only picks the decoding.
pub(crate) fn u32_at(r: &Record, i: usize) -> Option<u32> {
    match r.get_value(i)? {
        Value::UInt32(v) => Some(*v),
        Value::Int32(v) => Some(*v as u32),
        _ => None,
    }
}

pub(crate) fn f32_at(r: &Record, i: usize) -> Option<f32> {
    match r.get_value(i)? {
        Value::Float32(v) => Some(*v),
        _ => None,
    }
}

/// A signed field; either int variant reads, bit for bit.
pub(crate) fn i32_at(r: &Record, i: usize) -> Option<i32> {
    match r.get_value(i)? {
        Value::Int32(v) => Some(*v),
        Value::UInt32(v) => Some(*v as i32),
        _ => None,
    }
}

/// A string field from the string block; an empty string reads as `None`.
pub(crate) fn str_at(rs: &RecordSet, r: &Record, i: usize) -> Option<String> {
    match r.get_value(i)? {
        Value::StringRef(StringRef(off)) => {
            let s = rs.get_string(StringRef(*off)).ok()?;
            (!s.is_empty()).then(|| s.to_string())
        }
        _ => None,
    }
}

/// A localized string: of the eight offset columns from `first`, the locale's
/// (`record + first·4 + locale·4`, benilla-re `dbc#localized-strings`); `None` when it is empty.
/// The column's raw dword is the offset, as the client reads it, whatever the schema typed it: the
/// other locales' columns are often left untyped.
pub(crate) fn loc_str(rs: &RecordSet, r: &Record, first: usize, locale: Locale) -> Option<String> {
    loc_str_raw(rs, r, first, locale).filter(|s| !s.is_empty())
}

/// A localized string column by raw locale index, for a layout with more columns than the
/// vanilla eight-name table (the 12340 nine-locale block). The same offset semantics as
/// [`loc_str_raw`].
pub(crate) fn loc_str_index(
    rs: &RecordSet,
    r: &Record,
    first: usize,
    index: usize,
) -> Option<String> {
    let off = match r.get_value(first + index)? {
        Value::StringRef(StringRef(off)) | Value::UInt32(off) => *off,
        Value::Int32(off) => *off as u32,
        Value::Float32(f) => f.to_bits(),
    };
    rs.get_string(StringRef(off)).ok().map(|s| s.to_string())
}

/// [`loc_str`] for a reader that tells an empty name from no column: `Some("")` is a name the
/// file leaves blank, `None` a column or offset that cannot be read.
pub(crate) fn loc_str_raw(
    rs: &RecordSet,
    r: &Record,
    first: usize,
    locale: Locale,
) -> Option<String> {
    let off = match r.get_value(first + locale.index())? {
        Value::StringRef(StringRef(off)) | Value::UInt32(off) => *off,
        Value::Int32(off) => *off as u32,
        Value::Float32(f) => f.to_bits(),
    };
    rs.get_string(StringRef(off)).ok().map(|s| s.to_string())
}

/// `SpellIcon.dbc`: id → extensionless texture path (`Interface\Icons\…`).
pub(crate) fn load_spell_icon_map(chain: &mut Chain) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file("DBFilesClient\\SpellIcon.dbc")
        .context("reading SpellIcon.dbc")?;
    let mut schema = Schema::new("SpellIcon");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("TextureFilename", FieldType::String));
    let set = parse(&bytes, schema, "SpellIcon.dbc")?;
    let mut icons = HashMap::new();
    for r in set.records() {
        if let (Some(id), Some(path)) = (u32_at(r, 0), str_at(&set, r, 1)) {
            icons.insert(id, path);
        }
    }
    Ok(icons)
}

/// An id → name schema of `cols` four-byte columns (a string column is one offset); the other
/// locale slots and the locale flag mask stay anonymous `cN` columns.
pub(crate) fn id_name_schema(what: &str, name_col: usize, cols: usize) -> Schema {
    let mut s = Schema::new(what);
    for i in 0..cols {
        if i == name_col {
            s.add_field(SchemaField::new("Name", FieldType::String));
        } else {
            s.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
        }
    }
    s
}

/// Read an id → localized name DBC into a map, the name block's first column at `name_col`, read at
/// the chain's locale; a row whose name is empty is skipped (a hole, not a name).
pub(crate) fn load_id_name_table(
    chain: &mut Chain,
    file: &str,
    name_col: usize,
    cols: usize,
    what: &str,
) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file(file)
        .with_context(|| format!("reading {file}"))?;
    // The selected profile may be a later build whose table carries trailing columns (12340's
    // AreaTable has 36 fields against 1.12's 25). The name block starts at the same column; read
    // the file's own count, never fewer than the layout the caller requires.
    let fields = field_count(&bytes).with_context(|| format!("{what}: {file} field count"))?;
    anyhow::ensure!(
        fields >= cols,
        "{what}: {file} has {fields} fields, needs at least {cols}"
    );
    let rs = parse(&bytes, id_name_schema(what, name_col, fields), what)?;
    let locale = chain.locale();
    let mut out = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        if let (Some(id), Some(name)) = (u32_at(r, 0), loc_str(&rs, r, name_col, locale)) {
            out.insert(id, name);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shipped table has `record_size = 4 × field_count` but the two the client reads by
    /// their own byte-packed layouts, `CharBaseInfo` (2 fields in 2 bytes) and `CharStartOutfit`
    /// (41 in 152): a schema of 4-byte slots cannot describe those, and the reader refuses it
    /// (benilla-re `dbc#the-loader`, `dbc_schemas.tsv`).
    #[test]
    fn only_the_two_byte_packed_tables_are_refused_a_four_byte_schema() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let mut packed = Vec::new();
        let mut checked = 0usize;
        let tables: std::collections::BTreeSet<String> = chain
            .list()
            .expect("list the chain")
            .into_iter()
            .map(|entry| entry.name.to_ascii_lowercase().replace('/', "\\"))
            .filter(|name| name.starts_with("dbfilesclient\\") && name.ends_with(".dbc"))
            .collect();
        for path in tables {
            // A patch can delete a table it lists (`CharacterCreateCameras`).
            if !chain.contains(&path) {
                continue;
            }
            let bytes = chain
                .read_file(&path)
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            let Ok(parser) = DbcParser::parse(&mut Cursor::new(&bytes[..])) else {
                continue;
            };
            let header = *parser.header();
            if header.record_count == 0 {
                continue;
            }
            let mut schema = Schema::new("Slots");
            for i in 0..header.field_count {
                schema.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
            }
            let read = parser.with_schema(schema).unwrap().parse_records();
            checked += 1;
            if header.record_size == 4 * header.field_count {
                assert!(read.is_ok(), "{path}: {:?}", read.err());
            } else {
                assert!(read.is_err(), "{path} read through 4-byte slots");
                packed.push((path, header.field_count, header.record_size));
            }
        }
        packed.sort();
        assert!(checked > 100, "only {checked} tables checked");
        assert_eq!(
            packed,
            [
                ("dbfilesclient\\charbaseinfo.dbc".to_string(), 2, 2),
                ("dbfilesclient\\charstartoutfit.dbc".to_string(), 41, 152),
            ]
        );
    }

    /// A one-record WDBC: the id, eight string offsets (one per locale, column 2 blank) and the
    /// flags word, the shape of a `Name_Lang` table such as `Languages.dbc`.
    fn localized_table() -> (RecordSet, [&'static str; 8]) {
        let names = [
            "Common",
            "공용어",
            "Commun",
            "Gemeinsprache",
            "通用语",
            "通用語",
            "Común",
            "",
        ];
        let mut strings = vec![0u8];
        let mut fields = vec![7u32];
        for name in names {
            if name.is_empty() {
                fields.push(0);
            } else {
                fields.push(strings.len() as u32);
                strings.extend_from_slice(name.as_bytes());
                strings.push(0);
            }
        }
        fields.push(0x3f_007e);
        let mut bytes = b"WDBC".to_vec();
        for w in [
            1,
            fields.len() as u32,
            4 * fields.len() as u32,
            strings.len() as u32,
        ] {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        for f in &fields {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        bytes.extend_from_slice(&strings);
        // Only the first column typed as a string, as the loaders type it.
        let schema = id_name_schema("Languages", 1, fields.len());
        (parse(&bytes, schema, "Languages").unwrap(), names)
    }

    /// `record + first·4 + locale·4`: each locale reads its own column, typed or not.
    #[test]
    fn a_localized_string_reads_the_locales_column() {
        let (rs, names) = localized_table();
        let r = &rs.records()[0];
        for (index, name) in names.iter().enumerate() {
            let locale = Locale::from_index(index).unwrap();
            let expected = (!name.is_empty()).then(|| name.to_string());
            assert_eq!(loc_str(&rs, r, 1, locale), expected, "{}", locale.name());
        }
        let gb = Locale::from_language_country(b"en", b"GB");
        assert_eq!(loc_str(&rs, r, 1, gb).as_deref(), Some("Common"));
    }

    /// The chain's locale reaches the loaders: the install's own column is filled, and another
    /// locale's, empty in a single-language install, reads nothing.
    #[test]
    fn the_loaders_read_the_chains_locale() {
        let data = crate::wow_data_or_skip!();
        let mut own = crate::open_chain(&data).expect("open chain");
        let index = own.locale().index();
        let other = Locale::from_index((index + 1) % 8).unwrap();
        let mut other_chain = crate::open_chain(&data)
            .expect("open chain")
            .with_locale(other);

        let names = |chain: &mut Chain| {
            crate::load_languages(chain)
                .expect("Languages.dbc")
                .names()
                .count()
        };
        assert!(
            names(&mut own) > 0,
            "{} names the languages",
            own.locale().name()
        );
        assert_eq!(
            names(&mut other_chain),
            0,
            "{} is not this install's",
            other.name()
        );

        let own_maps = crate::load_map_catalog(&mut own).expect("Map.dbc");
        let other_maps = crate::load_map_catalog(&mut other_chain).expect("Map.dbc");
        assert!(own_maps.name(0).is_some());
        assert_eq!(other_maps.name(0), None);
    }
}
