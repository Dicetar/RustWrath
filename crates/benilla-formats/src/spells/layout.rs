//! Explicit on-disk Spell.dbc layouts. WotLK columns follow AzerothCore's
//! DBCStructure.h::SpellEntry; the legacy indices belong to this catalog's vanilla model.

use anyhow::{bail, Result};
use benilla_dbc::{FieldType, Record, RecordSet, Schema, SchemaField};

use crate::dbc::{self, field_count, parse};
use crate::Locale;

#[derive(Clone, Copy)]
pub(super) enum SpellLayout {
    Vanilla,
    Wrath,
}

pub(super) fn parse_spells(bytes: &[u8]) -> Result<(RecordSet, SpellLayout)> {
    let layout = match field_count(bytes) {
        Some(173) => SpellLayout::Vanilla,
        Some(234) => SpellLayout::Wrath,
        Some(columns) => bail!(
            "unsupported Spell.dbc layout: {columns} fields (expected 173 or 234)"
        ),
        None => bail!("parsing Spell.dbc header: truncated field count"),
    };
    let schema = match layout {
        SpellLayout::Vanilla => super::spell_schema(),
        SpellLayout::Wrath => wrath_schema(),
    };
    Ok((parse(bytes, schema, "Spell.dbc")?, layout))
}

fn wrath_schema() -> Schema {
    let mut schema = Schema::new("Spell");
    for column in 0..234 {
        let ty = match column {
            47 | 77..=79 | 101..=103 | 119..=121 | 216..=218 | 229..=231 => FieldType::Float32,
            136..=151 | 153..=168 | 170..=185 | 187..=202 => FieldType::String,
            _ => FieldType::UInt32,
        };
        schema.add_field(SchemaField::new(format!("F{column}"), ty));
    }
    schema
}

impl SpellLayout {
    /// A legacy model field's actual disk column. Removed fields have no source column;
    /// in particular, WotLK SpellPriority is not vanilla CastUI.
    fn column(self, legacy: usize) -> Option<usize> {
        match self {
            Self::Vanilla => Some(legacy),
            Self::Wrath => match legacy {
                0 => Some(0),
                1 => Some(225), // SchoolMask; converted to the legacy primary-school index below.
                2 => Some(1),
                3 => None, // CastUI was removed.
                4..=10 => Some(legacy - 2),
                11 => Some(12), // Low shapeshift-mask word.
                12 => Some(14), // Low excluded-shapeshift-mask word.
                13..=15 => Some(legacy + 3),
                16..=17 => Some(legacy + 4),
                18..=66 => Some(legacy + 10),
                67..=72 => None, // BaseDice and DicePerLevel were removed.
                73..=108 => Some(legacy + 4),
                109..=114 => Some(legacy + 7),
                115..=127 => Some(legacy + 16),
                128 => Some(152), // Name locale flags follow sixteen locales.
                129..=136 => Some(legacy + 24),
                137 => Some(169),
                138..=145 => Some(legacy + 32),
                146 => Some(186),
                147..=154 => Some(legacy + 40),
                155 => Some(203),
                156..=162 => Some(legacy + 48),
                163..=172 => Some(legacy + 49),
                _ => None,
            },
        }
    }

    pub(super) fn uint(self, record: &Record, legacy: usize) -> Option<u32> {
        if matches!(self, Self::Wrath) && legacy == super::COL_SCHOOL {
            // Existing single-school consumers use a primary index. Keep the entire mask in
            // SpellCatalog::school_mask rather than treating a mask as an index or losing it.
            return self
                .school_mask(record)
                .map(|mask| if mask == 0 { 0 } else { mask.trailing_zeros() });
        }
        dbc::u32_at(record, self.column(legacy)?)
    }

    pub(super) fn int(self, record: &Record, legacy: usize) -> Option<i32> {
        if matches!(self, Self::Wrath) {
            if (super::COL_EFFECT_BASE_DICE_1..super::COL_EFFECT_BASE_DICE_1 + 3).contains(&legacy) {
                // SpellEffectInfo::CalcValue: a zero DieSides adds no roll; otherwise the
                // roll includes 1. WotLK has no authored BaseDice or DicePerLevel columns.
                let effect = legacy - super::COL_EFFECT_BASE_DICE_1;
                return dbc::i32_at(record, 74 + effect).map(|sides| i32::from(sides != 0));
            }
            if (super::COL_EFFECT_DICE_PER_LEVEL_1..super::COL_EFFECT_DICE_PER_LEVEL_1 + 3)
                .contains(&legacy)
            {
                return Some(0);
            }
        }
        dbc::i32_at(record, self.column(legacy)?)
    }

    pub(super) fn float(self, record: &Record, legacy: usize) -> Option<f32> {
        dbc::f32_at(record, self.column(legacy)?)
    }

    pub(super) fn text(
        self,
        set: &RecordSet,
        record: &Record,
        legacy: usize,
        locale: Locale,
    ) -> Option<String> {
        dbc::loc_str(set, record, self.column(legacy)?, locale)
    }

    pub(super) fn school_mask(self, record: &Record) -> Option<u32> {
        match self {
            Self::Vanilla => 1u32.checked_shl(dbc::u32_at(record, super::COL_SCHOOL)?),
            Self::Wrath => dbc::u32_at(record, 225),
        }
    }

    pub(super) fn family_flags_high(self, record: &Record) -> u32 {
        match self {
            Self::Vanilla => 0,
            Self::Wrath => dbc::u32_at(record, 211).unwrap_or(0),
        }
    }
}
