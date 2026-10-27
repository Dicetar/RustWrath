//! `ChrRaces.dbc`, the per-race table, narrowed to the strings the unit getters push and the
//! intro sequence. The reference loads it into a race-indexed record table (`0xc0dee0`, max id
//! `0xc0dee4`).
//!
//! **1.12 layout** (29 fields): name block at field 17 (`+0x44`, eight locale columns), token at
//! field 15 (`+0x3c`), `CinematicSequenceID` at field 16 (`+0x40`).
//!
//! **12340 layout** (69 fields): 0 ID, 1 Flags, 2 FactionID, 4/5 male/female display, 11
//! ClientFilestring, 12 CinematicSequenceID, 14-29 Name, 31-46 Name_Female, 48-63 Name_Male,
//! 65/66 FacialHairCustomization, 67 HairCustomization, 68 Required_Expansion. The column order is
//! the pinned AzerothCore `chrraces_dbc.sql` generated from the DBC. Its name blocks carry the
//! DBC's sixteen physical locale slots; the supported first nine follow [`Locale::wrath_index`]
//! directly (the binary loader copies fields in order and `DetectDBCLang` indexes the first nine
//! by `LocaleConstant`). Generated SQL labels interleave enGB/enCN/enTW and are not a physical
//! remap. The token is still a key, never display text, and is not localized.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{field_count, loc_str, loc_str_index, parse_with_trailing_fields, str_at, u32_at};
use crate::{Chain, Locale};

const CHR_RACES: &str = "DBFilesClient\\ChrRaces.dbc";

/// The 1.12 shipped copy's column count, which `benilla-dbc` enforces as the reference loader does.
const CHR_RACES_FIELDS: usize = 29;

/// Field 15, the file string (`+0x3c`).
const FILE_FIELD: usize = 0x3c / 4;

/// Field 16, `CinematicSequenceID` (`+0x40`).
const INTRO_FIELD: usize = 0x40 / 4;

/// Field 17, the first of the eight `Name_Lang` columns (`+0x44`).
const NAME_FIELD: usize = 0x44 / 4;

/// The 12340 column count (pinned `chrraces_dbc.sql`).
const WRATH_RACES_FIELDS: usize = 69;

/// 12340: ClientFilestring.
const WRATH_FILE_FIELD: usize = 11;
/// 12340: CinematicSequenceID.
const WRATH_INTRO_FIELD: usize = 12;
/// 12340: first base-name column.
const WRATH_NAME_FIELD: usize = 14;
/// 12340: first female-name column.
const WRATH_FEMALE_NAME_FIELD: usize = 31;
/// 12340: first male-name column.
const WRATH_MALE_NAME_FIELD: usize = 48;
/// The client's nine supported 12340 locale columns.
const WRATH_LOCALES: usize = 9;

/// One race's row: its name at the loader's locale, its token, and (12340 only) the gendered
/// per-locale names.
#[derive(Debug, Clone, Default)]
struct Row {
    name: String,
    file: String,
    /// `CinematicSequenceID`.
    intro: u32,
    names: Vec<Option<String>>,
    female_names: Vec<Option<String>>,
    male_names: Vec<Option<String>>,
}

/// One race-label row for the Wrath UI projection, the class-label shape with race ids and the
/// client file token. No consumer requires it yet; it keeps the 12340 gender columns available
/// rather than dropping them at the loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedRaceLabel {
    pub id: u32,
    pub file: Option<String>,
    pub male: Option<String>,
    pub female: Option<String>,
}

/// `ChrRaces.dbc`'s read columns by race id.
#[derive(Debug, Default, Clone)]
pub struct ChrRaces(HashMap<u32, Row>);

impl ChrRaces {
    /// A table of `(id, name, file)` rows.
    pub fn from_rows<'a>(rows: impl IntoIterator<Item = (u32, &'a str, &'a str)>) -> Self {
        Self(
            rows.into_iter()
                .map(|(id, name, file)| {
                    let (name, file) = (name.to_string(), file.to_string());
                    (
                        id,
                        Row {
                            name,
                            file,
                            ..Row::default()
                        },
                    )
                })
                .collect(),
        )
    }

    /// `UnitRace`'s two returns for a race byte: the name and the token. A row pushes whatever its
    /// strings are, an empty one included; a race with no row has none (the two nils, `0x518324`).
    pub fn names(&self, race: u32) -> Option<(&str, &str)> {
        self.0
            .get(&race)
            .map(|r| (r.name.as_str(), r.file.as_str()))
    }

    /// Every row's id and name, in no order, as the `/who` race tag searches them (`0x5aee60`).
    pub fn names_by_id(&self) -> impl Iterator<Item = (u32, &str)> {
        self.0.iter().map(|(id, r)| (*id, r.name.as_str()))
    }

    /// The race row's `CinematicSequenceID`, the intro cinematic the first-login camera looks up
    /// (`0x46c2f8`); a race with no row has none.
    pub fn intro_sequence(&self, race: u32) -> Option<u32> {
        self.0.get(&race).map(|r| r.intro)
    }

    /// The 12340 labels for the client's nine-locale table, sorted by native id. `None` when the
    /// table is the 1.12 layout or the index is unsupported. Male/female use the gendered column
    /// when the locale carries one, else the base name in the same locale.
    pub fn localized_labels(&self, locale_index: usize) -> Option<Vec<LocalizedRaceLabel>> {
        if locale_index >= WRATH_LOCALES || self.0.values().any(|r| r.names.is_empty()) {
            return None;
        }
        let mut rows: Vec<LocalizedRaceLabel> = self
            .0
            .iter()
            .map(|(id, row)| {
                let non_empty = |value: Option<String>| value.filter(|s| !s.is_empty());
                let base = non_empty(row.names.get(locale_index).cloned().flatten());
                LocalizedRaceLabel {
                    id: *id,
                    file: Some(row.file.clone()).filter(|f| !f.is_empty()),
                    male: non_empty(row.male_names.get(locale_index).cloned().flatten())
                        .or_else(|| base.clone()),
                    female: non_empty(row.female_names.get(locale_index).cloned().flatten())
                        .or(base),
                }
            })
            .collect();
        rows.sort_by_key(|row| row.id);
        Some(rows)
    }

    /// A table for tests of what reads the intro column: `race` holds `sequence`.
    #[cfg(test)]
    pub(crate) fn with_intro_sequence(mut self, race: u32, sequence: u32) -> Self {
        self.0.entry(race).or_default().intro = sequence;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn vanilla_schema() -> Schema {
    let mut s = Schema::new("ChrRaces");
    for i in 0..CHR_RACES_FIELDS {
        let ty = if i == FILE_FIELD
            || (NAME_FIELD..NAME_FIELD + 8).contains(&i)
            || (26..=28).contains(&i)
        {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

fn wrath_schema() -> Schema {
    // The DBC's sixteen physical locale slots, not the nine the client exposes.
    const DBC_LOCALE_SLOTS: usize = 16;
    let mut s = Schema::new("ChrRaces");
    for i in 0..WRATH_RACES_FIELDS {
        let ty = if i == WRATH_FILE_FIELD
            || (WRATH_NAME_FIELD..WRATH_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
            || (WRATH_FEMALE_NAME_FIELD..WRATH_FEMALE_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
            || (WRATH_MALE_NAME_FIELD..WRATH_MALE_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
        {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

/// Load `ChrRaces.dbc`'s read columns from the patch chain, names at the chain's locale and
/// layout selected from the file header.
pub fn load_chr_races(chain: &mut Chain) -> Result<ChrRaces> {
    let bytes = chain
        .read_file(CHR_RACES)
        .with_context(|| format!("reading {CHR_RACES}"))?;
    table_from_bytes(&bytes, chain.locale())
}

/// The table a `ChrRaces.dbc` holds, its names read at `locale`.
fn table_from_bytes(bytes: &[u8], locale: Locale) -> Result<ChrRaces> {
    let fields = field_count(bytes).context("ChrRaces.dbc header field count")?;
    if fields < CHR_RACES_FIELDS {
        bail!(
            "ChrRaces.dbc has {fields} fields; the 1.12 layout needs at least {CHR_RACES_FIELDS}"
        );
    }
    if fields >= WRATH_RACES_FIELDS {
        table_from_wrath(bytes, locale.wrath_index())
    } else {
        table_from_vanilla(bytes, locale)
    }
}

fn table_from_vanilla(bytes: &[u8], locale: Locale) -> Result<ChrRaces> {
    let rs = parse_with_trailing_fields(bytes, vanilla_schema(), "ChrRaces.dbc")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            Row {
                name: loc_str(&rs, r, NAME_FIELD, locale).unwrap_or_default(),
                file: str_at(&rs, r, FILE_FIELD).unwrap_or_default(),
                intro: u32_at(r, INTRO_FIELD).unwrap_or(0),
                ..Row::default()
            },
        );
    }
    Ok(ChrRaces(rows))
}

fn table_from_wrath(bytes: &[u8], locale_index: usize) -> Result<ChrRaces> {
    let rs = parse_with_trailing_fields(bytes, wrath_schema(), "ChrRaces.dbc")?;
    let columns = (0..WRATH_LOCALES)
        .map(|ui| Locale::wrath_dbc_column(ui).unwrap_or(0))
        .collect::<Vec<_>>();
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let names = columns
            .iter()
            .map(|&column| loc_str_index(&rs, r, WRATH_NAME_FIELD, column))
            .collect::<Vec<_>>();
        let female_names = columns
            .iter()
            .map(|&column| loc_str_index(&rs, r, WRATH_FEMALE_NAME_FIELD, column))
            .collect::<Vec<_>>();
        let male_names = columns
            .iter()
            .map(|&column| loc_str_index(&rs, r, WRATH_MALE_NAME_FIELD, column))
            .collect::<Vec<_>>();
        rows.insert(
            id,
            Row {
                name: names.get(locale_index).cloned().flatten().unwrap_or_default(),
                file: str_at(&rs, r, WRATH_FILE_FIELD).unwrap_or_default(),
                intro: u32_at(r, WRATH_INTRO_FIELD).unwrap_or(0),
                names,
                female_names,
                male_names,
            },
        );
    }
    Ok(ChrRaces(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two 29-field rows: race 4 with a name per locale, race 9 with its seventh column blank.
    fn synthetic() -> Vec<u8> {
        let names = [
            "Night Elf",
            "ë‚˜ì´íŠ¸ ì—˜í”„",
            "Elfe de la nuit",
            "Nachtelf",
            "æš—å¤œç²¾çµ",
            "å¤œç²¾éˆ",
            "Elfo de la noche",
            "xx",
        ];
        let mut strings = vec![0u8];
        let put = |s: &str, strings: &mut Vec<u8>| {
            let at = strings.len() as u32;
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
            at
        };
        let mut rows: Vec<Vec<u32>> = Vec::new();
        let mut row = vec![0u32; CHR_RACES_FIELDS];
        row[0] = 4;
        row[INTRO_FIELD] = 61;
        row[FILE_FIELD] = put("NightElf", &mut strings);
        for (i, n) in names.iter().enumerate() {
            row[NAME_FIELD + i] = put(n, &mut strings);
        }
        rows.push(row);
        let mut row = vec![0u32; CHR_RACES_FIELDS];
        row[0] = 9;
        row[FILE_FIELD] = put("Goblin", &mut strings);
        for i in 0..7 {
            row[NAME_FIELD + i] = put("Goblin", &mut strings);
        }
        rows.push(row);
        let mut bytes = b"WDBC".to_vec();
        for w in [
            2,
            CHR_RACES_FIELDS as u32,
            4 * CHR_RACES_FIELDS as u32,
            strings.len() as u32,
        ] {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        for f in rows.iter().flatten() {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        bytes.extend_from_slice(&strings);
        bytes
    }

    /// Each locale reads its own name column (`row + 0x44 + 4 * [0xc0e080]`) and the token is the
    /// same string in all of them; a blank column is an empty name on a row that exists, and a
    /// race with no row has nothing.
    #[test]
    fn each_locale_reads_its_own_name_and_the_token_stays() {
        let bytes = synthetic();
        let want = [
            "Night Elf",
            "ë‚˜ì´íŠ¸ ì—˜í”„",
            "Elfe de la nuit",
            "Nachtelf",
            "æš—å¤œç²¾çµ",
            "å¤œç²¾éˆ",
            "Elfo de la noche",
            "xx",
        ];
        for (index, name) in want.iter().enumerate() {
            let locale = Locale::from_index(index).unwrap();
            let table = table_from_bytes(&bytes, locale).unwrap();
            assert_eq!(
                table.names(4),
                Some((*name, "NightElf")),
                "{}",
                locale.name()
            );
        }
        let blank = table_from_bytes(&bytes, Locale::from_index(7).unwrap()).unwrap();
        assert_eq!(blank.names(9), Some(("", "Goblin")));
        assert_eq!(blank.names(3), None);
        assert_eq!(blank.names(0), None);
    }

    /// `CinematicSequenceID` is the row's field 16 (`+0x40`, read at `0x46c2f8`): a row whose column
    /// is 0 holds 0, and a race with no row holds nothing.
    #[test]
    fn a_race_row_holds_its_intro_sequence() {
        let table = table_from_bytes(&synthetic(), Locale::from_index(0).unwrap()).unwrap();
        assert_eq!(table.intro_sequence(4), Some(61));
        assert_eq!(table.intro_sequence(9), Some(0));
        assert_eq!(table.intro_sequence(3), None);
    }

    /// The shipped table at the install's locale: the eight playable races and the Goblin row.
    #[test]
    fn the_shipped_names_and_tokens() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let t = load_chr_races(&mut chain).expect("load ChrRaces.dbc");
        if chain.locale().index() == 0 {
            for (race, name, file) in [
                (1, "Human", "Human"),
                (2, "Orc", "Orc"),
                (3, "Dwarf", "Dwarf"),
                (4, "Night Elf", "NightElf"),
                (5, "Undead", "Scourge"),
                (6, "Tauren", "Tauren"),
                (7, "Gnome", "Gnome"),
                (8, "Troll", "Troll"),
            ] {
                assert_eq!(t.names(race), Some((name, file)), "race {race}");
            }
        }
        assert_eq!(t.names(0), None);
        assert!(t
            .names(1)
            .is_some_and(|(n, f)| !n.is_empty() && !f.is_empty()));
    }
}
