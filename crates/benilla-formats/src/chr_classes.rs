//! `ChrClasses.dbc`, the per-class table.
//!
//! **1.12 layout** (17 fields): field 2 damage bonus stat (`GetDamageBonusStat`, `0x48b520`),
//! field 4 pet name token, fields 5-12 `Name_Lang`, field 14 class file string, field 15 class
//! spell family, field 16 relic-slot flag. The reader below preserves every existing accessor.
//!
//! **12340 layout** (60 fields): 0 ID, 2 DisplayPower, 3 PetNameToken, 4-19 Name, 20 mask,
//! 21-36 Name_Female, 37 mask, 38-53 Name_Male, 54 mask, 55 Filename, 56 SpellClassSet,
//! 57 Flags, 58 CinematicSequenceID, 59 Required_Expansion. The column order is the pinned
//! AzerothCore `chrclasses_dbc.sql` generated from the DBC; `DBCfmt.h` corroborates the
//! server-read columns. The name blocks are the 3.3.5 DBC's sixteen physical locale slots in the
//! pinned SQL order (enUS, enGB, koKR, frFR, deDE, enCN, zhCN, enTW, zhTW, esES, esMX, ruRU,
//! ptPT, ptBR, itIT, Unk); the client's nine-locale UI index maps into them through
//! [`Locale::wrath_dbc_column`] (enUS→0, koKR→2, frFR→3, ...). 12340's damage-bonus column
//! does not exist, so [`ChrClasses::damage_bonus_stat`]
//! stays `None` (unavailable) rather than reading DisplayPower. The integer PetNameToken is
//! accepted only when it is a string-table offset yielding exactly "PET" or "DEMON"; otherwise
//! the existing "PET" fallback remains (no guessed enum mapping).
//!
//! Gender labels: the gendered column for the selected locale when it carries one, else the
//! row's base `Name` column in the same locale (the DBC's own default), never another language
//! or the other gender.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Record, RecordSet, Schema, SchemaField, StringRef};

use crate::chain::Chain;
use crate::dbc::{field_count, loc_str, loc_str_index, parse_with_trailing_fields, str_at, u32_at};
use crate::Locale;

const CHR_CLASSES: &str = "DBFilesClient\\ChrClasses.dbc";

/// The 1.12 patch copy's column count.
const CHR_CLASSES_FIELDS: usize = 17;

/// The 12340 column count (pinned `chrclasses_dbc.sql`).
const WRATH_CHR_CLASSES_FIELDS: usize = 60;

/// The client's nine supported 12340 locale columns.
const WRATH_LOCALES: usize = 9;

/// Field 2, read at `rec + 8` by `GetDamageBonusStat` (`0x48b569`).
const DAMAGE_BONUS_STAT_FIELD: usize = 0x8 / 4;

/// Field 5, the first of the eight `Name_Lang` columns (`rec + 0x14`, `0x473368`).
const NAME_FIELD: usize = 0x14 / 4;

/// Field 4, read at `rec + 0x10` by `HasPetSpells` (`0x4b447c`).
const PET_NAME_TOKEN_FIELD: usize = 0x10 / 4;

/// Field 16, read at `row + 0x40` by `UnitHasRelicSlot` (`0x519ebb`).
const RELIC_SLOT_FIELD: usize = 0x40 / 4;

/// Field 14, read at `+0x38` by `GetClassesForRace` (`0x471594`).
const FILE_FIELD: usize = 0x38 / 4;

/// Field 15, read at `+0x3c` by `0x6e6ca0`.
const SPELL_FAMILY_FIELD: usize = 0x3c / 4;

/// 12340: display power type (`Powers`).
const WRATH_DISPLAY_POWER_FIELD: usize = 2;
/// 12340: PetNameToken (`int` column in the pinned dump).
const WRATH_PET_NAME_TOKEN_FIELD: usize = 3;
/// 12340: first base-name column.
const WRATH_NAME_FIELD: usize = 4;
/// 12340: first female-name column.
const WRATH_FEMALE_NAME_FIELD: usize = 21;
/// 12340: first male-name column.
const WRATH_MALE_NAME_FIELD: usize = 38;
/// 12340: Filename.
const WRATH_FILE_FIELD: usize = 55;
/// 12340: SpellClassSet (the 1.12 class spell family).
const WRATH_SPELL_FAMILY_FIELD: usize = 56;
/// 12340: Flags.
const WRATH_FLAGS_FIELD: usize = 57;
/// 12340: CinematicSequenceID.
const WRATH_CINEMATIC_FIELD: usize = 58;
/// 12340: Required_Expansion.
const WRATH_EXPANSION_FIELD: usize = 59;

/// The literal the reference pushes when the player does not resolve (`0x846a40`). Every class but
/// the Warlock carries it too, so an unloaded table degrades invisibly.
pub const PET_NAME_TOKEN_FALLBACK: &str = "PET";

/// One class-label row for the Wrath UI projection: native id, stable file token and the labels at
/// the requested locale. A `None` label means neither the gendered column nor the same-locale base
/// carried a non-empty string; resolved rows therefore always carry labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizedClassLabel {
    pub id: u32,
    pub file: Option<String>,
    pub male: Option<String>,
    pub female: Option<String>,
}

#[derive(Debug, Clone)]
struct ChrClass {
    /// 1.12 `GetDamageBonusStat`; `None` on a 12340 row (the column does not exist).
    damage_bonus_stat: Option<u32>,
    /// 12340 DisplayPower; `None` on a 1.12 row.
    display_power: Option<u32>,
    pet_name_token: Option<String>,
    /// The base class name at the loader's locale, for the existing accessor.
    name: Option<String>,
    file: Option<String>,
    has_relic_slot: bool,
    spell_family: u32,
    flags: Option<u32>,
    cinematic: Option<u32>,
    expansion: Option<u32>,
    /// Per-locale base/female/male names (12340 only; empty on 1.12).
    names: Vec<Option<String>>,
    female_names: Vec<Option<String>>,
    male_names: Vec<Option<String>>,
}

impl ChrClass {
    fn from_vanilla(
        damage_bonus_stat: u32,
        pet_name_token: Option<String>,
        name: Option<String>,
        file: Option<String>,
        has_relic_slot: bool,
        spell_family: u32,
    ) -> Self {
        Self {
            damage_bonus_stat: Some(damage_bonus_stat),
            display_power: None,
            pet_name_token,
            name,
            file,
            has_relic_slot,
            spell_family,
            flags: None,
            cinematic: None,
            expansion: None,
            names: Vec::new(),
            female_names: Vec::new(),
            male_names: Vec::new(),
        }
    }
}

/// `ChrClasses.dbc`'s read columns by class id.
#[derive(Debug, Default, Clone)]
pub struct ChrClasses(HashMap<u32, ChrClass>);

impl ChrClasses {
    /// A table of `(id, name, file)` rows, the other columns at their zero values.
    pub fn from_rows<'a>(rows: impl IntoIterator<Item = (u32, &'a str, &'a str)>) -> Self {
        Self(
            rows.into_iter()
                .map(|(id, name, file)| {
                    let row = ChrClass::from_vanilla(
                        0,
                        None,
                        Some(name.to_string()).filter(|n| !n.is_empty()),
                        Some(file.to_string()).filter(|f| !f.is_empty()),
                        false,
                        0,
                    );
                    (id, row)
                })
                .collect(),
        )
    }

    /// `HasPetSpells`' second return: always a string, never nil, and the literal
    /// [`PET_NAME_TOKEN_FALLBACK`] for a class with no row (`0x4b44a6`).
    pub fn pet_name_token(&self, class: u32) -> &str {
        self.0
            .get(&class)
            .and_then(|c| c.pet_name_token.as_deref())
            .unwrap_or(PET_NAME_TOKEN_FALLBACK)
    }

    /// The class's `Name_Lang` name at the loader's locale, `GetCharacterInfo`'s third return;
    /// `None` for a class with no row or no name, which that binding answers with the empty string
    /// (`0x47336e`).
    pub fn name(&self, class: u32) -> Option<&str> {
        self.0.get(&class)?.name.as_deref()
    }

    /// Every row's id and name, in no order, the empty string for a row with none, as the `/who`
    /// class tag searches them (`0x5aef0d`).
    pub fn names_by_id(&self) -> impl Iterator<Item = (u32, &str)> {
        self.0
            .iter()
            .map(|(id, c)| (*id, c.name.as_deref().unwrap_or_default()))
    }

    /// The class's file string (`"WARRIOR"`), the create screen's second value per class; `None`
    /// for a class with no row or an empty string.
    pub fn file(&self, class: u32) -> Option<&str> {
        self.0.get(&class)?.file.as_deref()
    }

    /// Whether the table has a row for `class`.
    pub fn has_row(&self, class: u32) -> bool {
        self.0.contains_key(&class)
    }

    /// The 1.12 class's damage bonus stat, 0-based (Strength 0, Agility 1); `None` for a class
    /// with no row (where `GetDamageBonusStat` answers 0), and for every 12340 row because that
    /// layout carries no such column (DisplayPower is not it).
    pub fn damage_bonus_stat(&self, class: u32) -> Option<u32> {
        self.0.get(&class)?.damage_bonus_stat
    }

    /// The 12340 class's display power type; `None` on a 1.12 row.
    pub fn display_power(&self, class: u32) -> Option<u32> {
        self.0.get(&class)?.display_power
    }

    /// The 12340 class Flags word; `None` on a 1.12 row (no equivalent column).
    pub fn flags(&self, class: u32) -> Option<u32> {
        self.0.get(&class)?.flags
    }

    /// The 12340 CinematicSequenceID; `None` on a 1.12 row.
    pub fn cinematic_sequence(&self, class: u32) -> Option<u32> {
        self.0.get(&class)?.cinematic
    }

    /// The 12340 Required_Expansion; `None` on a 1.12 row.
    pub fn required_expansion(&self, class: u32) -> Option<u32> {
        self.0.get(&class)?.expansion
    }

    /// Whether the class's INVSLOT 17 is a relic slot, `UnitHasRelicSlot` whole. A class with no
    /// row answers false, as the reference's bound check against `0xc0def8` falls to its nil leg.
    /// The 12340 layout has no evidence-backed equivalent, so every 12340 row answers false.
    pub fn has_relic_slot(&self, class: u32) -> bool {
        self.0.get(&class).is_some_and(|c| c.has_relic_slot)
    }

    /// Whether a unit of `class` may hold the ranged sheathe state: a row whose relic flag is
    /// clear. The sheathe toggle (`0x5eb616`-`0x5eb640`) and setter (`0x611d12`-`0x611d44`) read
    /// the flag as [`Self::has_relic_slot`] does, but a class with no row fails their test too.
    pub fn has_ranged_sheath(&self, class: u32) -> bool {
        self.0.get(&class).is_some_and(|c| !c.has_relic_slot)
    }

    /// Sets `class`'s relic flag, for a table built by [`Self::from_rows`]; a class with no row
    /// is left without one.
    pub fn with_relic_slot(mut self, class: u32) -> Self {
        if let Some(c) = self.0.get_mut(&class) {
            c.has_relic_slot = true;
        }
        self
    }

    /// The class spell family, which the reference caches in `[0xcecaac]` and matches against a
    /// spell's `SpellFamilyName` before a talent modifier applies. A class with no row answers 0,
    /// the global's value from world-enter (`0x6e7150`) until the player resolves, which the
    /// gate's `SpellFamilyName != 0` conjunct makes match nothing. 12340 stores it as
    /// `SpellClassSet`.
    pub fn spell_family(&self, class: u32) -> u32 {
        self.0.get(&class).map_or(0, |c| c.spell_family)
    }

    /// The 12340 class labels for the client's nine-locale table, sorted by native id. `None`
    /// when the table is the 1.12 layout (no per-locale row data) or the index is unsupported.
    /// Male/female use the gendered column when the locale carries one, else the base name in the
    /// same locale. A row with no Filename keeps `file: None` so the UI projection can refuse it.
    pub fn localized_labels(&self, locale_index: usize) -> Option<Vec<LocalizedClassLabel>> {
        if locale_index >= WRATH_LOCALES || self.0.values().any(|c| c.names.is_empty()) {
            return None;
        }
        let mut rows: Vec<LocalizedClassLabel> = self
            .0
            .iter()
            .map(|(id, row)| {
                let non_empty = |value: Option<String>| value.filter(|s| !s.is_empty());
                let base = non_empty(row.names.get(locale_index).cloned().flatten());
                LocalizedClassLabel {
                    id: *id,
                    file: row.file.clone(),
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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn vanilla_schema() -> Schema {
    let mut s = Schema::new("ChrClasses");
    for i in 0..CHR_CLASSES_FIELDS {
        let ty = if i == PET_NAME_TOKEN_FIELD
            || i == FILE_FIELD
            || (NAME_FIELD..NAME_FIELD + 8).contains(&i)
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
    let string = |i: usize| {
        (WRATH_NAME_FIELD..WRATH_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
            || (WRATH_FEMALE_NAME_FIELD..WRATH_FEMALE_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
            || (WRATH_MALE_NAME_FIELD..WRATH_MALE_NAME_FIELD + DBC_LOCALE_SLOTS).contains(&i)
            || i == WRATH_FILE_FIELD
    };
    let mut s = Schema::new("ChrClasses");
    for i in 0..WRATH_CHR_CLASSES_FIELDS {
        let ty = if string(i) {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

fn load_vanilla(bytes: &[u8], locale: Locale) -> Result<ChrClasses> {
    let rs = parse_with_trailing_fields(bytes, vanilla_schema(), "ChrClasses.dbc")?;
    let mut by_id = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        by_id.insert(
            id,
            ChrClass::from_vanilla(
                u32_at(r, DAMAGE_BONUS_STAT_FIELD).unwrap_or(0),
                str_at(&rs, r, PET_NAME_TOKEN_FIELD),
                loc_str(&rs, r, NAME_FIELD, locale),
                str_at(&rs, r, FILE_FIELD),
                u32_at(r, RELIC_SLOT_FIELD).is_some_and(|v| v != 0),
                u32_at(r, SPELL_FAMILY_FIELD).unwrap_or(0),
            ),
        );
    }
    Ok(ChrClasses(by_id))
}

/// The 12340 `PetNameToken` is an integer column in the pinned dump; accept it only when the
/// value is a string-table offset yielding one of the two non-localized keys the client knows.
/// Anything else leaves the token unavailable and the existing "PET" fallback applies.
fn pet_name_token_wrath(rs: &RecordSet, r: &Record) -> Option<String> {
    let offset = u32_at(r, WRATH_PET_NAME_TOKEN_FIELD)?;
    let token = rs.get_string(StringRef(offset)).ok()?;
    matches!(token, "PET" | "DEMON").then(|| token.to_string())
}

fn load_wrath(bytes: &[u8], locale_index: usize) -> Result<ChrClasses> {
    let rs = parse_with_trailing_fields(bytes, wrath_schema(), "ChrClasses.dbc")?;
    let mut by_id = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let columns = (0..WRATH_LOCALES)
            .map(|ui| Locale::wrath_dbc_column(ui).unwrap_or(0))
            .collect::<Vec<_>>();
        let names = columns
            .iter()
            .map(|&column| {
                loc_str_index(&rs, r, WRATH_NAME_FIELD, column).filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>();
        let female_names = columns
            .iter()
            .map(|&column| {
                loc_str_index(&rs, r, WRATH_FEMALE_NAME_FIELD, column).filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>();
        let male_names = columns
            .iter()
            .map(|&column| {
                loc_str_index(&rs, r, WRATH_MALE_NAME_FIELD, column).filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>();
        by_id.insert(
            id,
            ChrClass {
                damage_bonus_stat: None,
                display_power: u32_at(r, WRATH_DISPLAY_POWER_FIELD),
                pet_name_token: pet_name_token_wrath(&rs, r),
                name: names.get(locale_index).cloned().flatten(),
                file: str_at(&rs, r, WRATH_FILE_FIELD),
                has_relic_slot: false,
                spell_family: u32_at(r, WRATH_SPELL_FAMILY_FIELD).unwrap_or(0),
                flags: u32_at(r, WRATH_FLAGS_FIELD),
                cinematic: u32_at(r, WRATH_CINEMATIC_FIELD),
                expansion: u32_at(r, WRATH_EXPANSION_FIELD),
                names,
                female_names,
                male_names,
            },
        );
    }
    Ok(ChrClasses(by_id))
}

/// Load `ChrClasses.dbc`'s read columns from the patch chain, selecting the 1.12 or 12340 layout
/// from the file header. A file shorter than the 1.12 layout, or a 1.12 patch's trailing columns,
/// keep their existing behavior.
pub fn load_chr_classes(chain: &mut Chain) -> Result<ChrClasses> {
    let bytes = chain
        .read_file(CHR_CLASSES)
        .with_context(|| format!("reading {CHR_CLASSES}"))?;
    let fields = field_count(&bytes).context("ChrClasses.dbc header field count")?;
    if fields < CHR_CLASSES_FIELDS {
        bail!(
            "ChrClasses.dbc has {fields} fields; the 1.12 layout needs at least {CHR_CLASSES_FIELDS}"
        );
    }
    if fields >= WRATH_CHR_CLASSES_FIELDS {
        load_wrath(&bytes, chain.locale().wrath_index())
    } else {
        load_vanilla(&bytes, chain.locale())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Option<crate::chain::Chain> {
        let data = crate::wow_data_or_skip!(None);
        Some(crate::open_chain(&data).expect("open chain"))
    }

    /// Field 5 holds the class name, so a one-column slip would blank the spellbook tab silently.
    #[test]
    fn warlocks_pet_is_a_demon_and_everyone_elses_is_a_pet() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        assert!(!t.is_empty());
        assert_eq!(t.pet_name_token(9), "DEMON", "Warlock");
        for (class, who) in [(1, "Warrior"), (3, "Hunter"), (11, "Druid")] {
            assert_eq!(t.pet_name_token(class), "PET", "{who}");
        }
        // 6 and 10 have no row in 1.12; the reference's out-of-range arm answers the literal.
        assert_eq!(t.pet_name_token(6), PET_NAME_TOKEN_FALLBACK);
        assert_eq!(t.pet_name_token(0), PET_NAME_TOKEN_FALLBACK);
    }

    #[test]
    fn libram_totem_and_idol_are_the_three_relic_classes() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, who) in [(2, "Paladin"), (7, "Shaman"), (11, "Druid")] {
            assert!(t.has_relic_slot(class), "{who} carries a relic");
        }
        for (class, who) in [
            (1, "Warrior"),
            (3, "Hunter"),
            (4, "Rogue"),
            (5, "Priest"),
            (8, "Mage"),
            (9, "Warlock"),
        ] {
            assert!(!t.has_relic_slot(class), "{who} wields a ranged weapon");
        }
        // No row: the reference's nil leg.
        assert!(!t.has_relic_slot(6));
        assert!(!t.has_relic_slot(0));
    }

    /// The sheathe class test (`0x5eb616`-`0x5eb640`, `0x611d12`-`0x611d44`): a relic class has
    /// no ranged sheathe state, and neither has a class with no row, where `UnitHasRelicSlot`
    /// answers nil.
    #[test]
    fn a_relic_class_and_a_class_without_a_row_have_no_ranged_sheath() {
        let t = ChrClasses::from_rows([(1, "Warrior", "WARRIOR"), (2, "Paladin", "PALADIN")])
            .with_relic_slot(2);
        assert!(t.has_ranged_sheath(1), "a Warrior draws a bow");
        assert!(
            !t.has_ranged_sheath(2),
            "a Paladin's libram is no ranged weapon"
        );
        assert!(!t.has_ranged_sheath(6), "no row");
        assert!(
            !t.has_relic_slot(6),
            "while UnitHasRelicSlot says no relic slot there"
        );
    }

    /// Field 14 is the class file string the create screen keys its icons by
    /// (`CLASS_ICON_TCOORDS`), one column past the name block's flags.
    #[test]
    fn the_file_string_is_field_14() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, file) in [
            (1, "WARRIOR"),
            (2, "PALADIN"),
            (3, "HUNTER"),
            (4, "ROGUE"),
            (5, "PRIEST"),
            (7, "SHAMAN"),
            (8, "MAGE"),
            (9, "WARLOCK"),
            (11, "DRUID"),
        ] {
            assert_eq!(t.file(class), Some(file), "class {class}");
        }
        assert_eq!(t.file(6), None);
        assert!(t.has_row(1) && !t.has_row(6));
    }

    /// Hunters and Rogues scale their damage off Agility, everyone else off Strength.
    #[test]
    fn hunters_and_rogues_take_their_damage_bonus_from_agility() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, who) in [(3, "Hunter"), (4, "Rogue")] {
            assert_eq!(t.damage_bonus_stat(class), Some(1), "{who}");
        }
        for (class, who) in [
            (1, "Warrior"),
            (2, "Paladin"),
            (5, "Priest"),
            (7, "Shaman"),
            (8, "Mage"),
            (9, "Warlock"),
            (11, "Druid"),
        ] {
            assert_eq!(t.damage_bonus_stat(class), Some(0), "{who}");
        }
        assert_eq!(t.damage_bonus_stat(6), None);
    }

    /// `GetCharacterInfo`'s class is the `Name_Lang` block at field 5, one column per locale; a
    /// class with no row has no name (6 and 10 in 1.12).
    #[test]
    fn class_names_are_the_name_lang_column() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, name) in [
            (1, "Warrior"),
            (2, "Paladin"),
            (3, "Hunter"),
            (4, "Rogue"),
            (5, "Priest"),
            (7, "Shaman"),
            (8, "Mage"),
            (9, "Warlock"),
            (11, "Druid"),
        ] {
            assert_eq!(t.name(class), Some(name), "class {class}");
        }
        assert_eq!((t.name(6), t.name(10), t.name(0)), (None, None, None));
    }

    /// The whole column pins field 15: nine distinct values matching vmangos `SpellFamilyNames`
    /// are a shape no neighbour has (field 14 is the class's file name, field 16 the relic flag).
    #[test]
    fn every_class_row_carries_its_vmangos_spell_family() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, family, who) in [
            (1u32, 4u32, "Warrior"),
            (2, 10, "Paladin"),
            (3, 9, "Hunter"),
            (4, 8, "Rogue"),
            (5, 6, "Priest"),
            (7, 11, "Shaman"),
            (8, 3, "Mage"),
            (9, 5, "Warlock"),
            (11, 7, "Druid"),
        ] {
            assert_eq!(t.spell_family(class), family, "{who}");
        }
        // No row: the reference's zeroed global, which the gate's first conjunct refuses.
        assert_eq!(t.spell_family(6), 0);
        assert_eq!(t.spell_family(0), 0);
    }
}
