//! `SpellShapeshiftForm.dbc`: the per-form rows the action bar, the form gate, the spell tooltip
//! and tracking read.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{field_count, i32_at, loc_str, parse, u32_at};

const SHAPESHIFT_FORM: &str = "DBFilesClient\\SpellShapeshiftForm.dbc";

/// One `SpellShapeshiftForm.dbc` row's consumed fields.
#[derive(Clone, Debug, Default)]
pub struct ShapeshiftForm {
    /// `BonusActionBar` (column 1): the page the action bar flips to, 0 for none.
    pub bonus_bar: u32,
    /// The name (the block at column 2, read at the locale), the tooltip's `SPELL_REQUIRED_FORM` "Requires %s"
    /// (`0x52f10a`-`0x52f2ae`).
    pub name: String,
    /// `flags1` (vanilla column 11, WotLK 19): bit 0 marks a stance, bit 1 blocks cancelling it.
    pub flags: u32,
    /// `creatureType` (vanilla column 12, WotLK 20): the form's creature-type override. The
    /// resolver (`0x605570`) reads it before the creature template or race, so a cat-form druid
    /// tracks as a Beast; `<= 0` is no override, and the template or race answers.
    pub creature_type: i32,
    /// `AttackIconID` (vanilla column 13, WotLK 21) through `SpellIcon.dbc`: the Attack action
    /// shows the form's icon before the main-hand weapon's (`0x4e6870`); `None` uses the weapon.
    pub attack_icon: Option<String>,
}

impl ShapeshiftForm {
    /// A stance (vmangos `SHAPESHIFT_FLAG_STANCE`: warrior stances, Stealth), which the form gate
    /// ([`crate::spells::SpellDisplay::usable_in_form`]) does not count as shapeshifted.
    pub fn is_stance(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Flag bit `0x8` of `flags1`: the form may still talk to NPCs. Without it `0x5f0130` refuses
    /// the interaction (`ERR_CANT_INTERACT_SHAPESHIFTED`) and a change into the form closes the
    /// open window (`0x5de072`).
    pub fn allows_interaction(&self) -> bool {
        self.flags & 0x8 != 0
    }

    /// Clicking the active form's button cancels its aura (`CMSG_CANCEL_AURA`) unless bit `0x2`
    /// makes it a silent no-op (`CastShapeshiftForm` `0x4b4810`, guard at `0x4b4963`); the
    /// warrior stances (`0x7`) set it on 5875.
    pub fn cancelable(&self) -> bool {
        self.flags & 0x2 == 0
    }
}

/// Load `SpellShapeshiftForm.dbc` by form id. The bonus bar is data, not a stance switch:
/// `GetBonusBarOffset` (`0x4e7620`) returns what the `UPDATE_BONUS_ACTIONBAR` handler (`0x4e4fc0`)
/// read from column 1 for the player's form. `flags1` feeds the form gate (`0x612480`).
pub fn load_shapeshift_forms(chain: &mut Chain) -> Result<HashMap<u32, ShapeshiftForm>> {
    let bytes = chain
        .read_file(SHAPESHIFT_FORM)
        .context("reading SpellShapeshiftForm.dbc")?;
    // AzerothCore DBCStructure.h::SpellShapeshiftFormEntry: sixteen name locales shift
    // flags/type/icon to 19/20/21; the row continues with attack speed, models and spells.
    let (columns, flags_column, creature_type_column, attack_icon_column) =
        match field_count(&bytes) {
            Some(14) => (14, 11, 12, 13),
            Some(35) => (35, 19, 20, 21),
            Some(columns) => bail!(
                "unsupported SpellShapeshiftForm.dbc layout: {columns} fields (expected 14 or 35)"
            ),
            None => bail!("parsing SpellShapeshiftForm.dbc header: truncated field count"),
        };
    let mut schema = Schema::new("SpellShapeshiftForm");
    for i in 0..columns {
        match i {
            // The localized name starts at 2 in both supported layouts.
            2 => schema.add_field(SchemaField::new("Name", FieldType::String)),
            _ => schema.add_field(SchemaField::new(format!("F{i}"), FieldType::UInt32)),
        }
    }
    let set = parse(&bytes, schema, "SpellShapeshiftForm.dbc")?;
    let locale = chain.locale();
    // AttackIconID resolves through SpellIcon.dbc like a spell's own icon (`0x4e68af`-`0x4e68da`).
    let icons = crate::dbc::load_spell_icon_map(chain)?;
    let mut map = HashMap::new();
    for r in set.records() {
        if let Some(id) = u32_at(r, 0) {
            map.insert(
                id,
                ShapeshiftForm {
                    bonus_bar: u32_at(r, 1).unwrap_or(0),
                    name: loc_str(&set, r, 2, locale).unwrap_or_default(),
                    flags: u32_at(r, flags_column).unwrap_or(0),
                    creature_type: i32_at(r, creature_type_column).unwrap_or(0),
                    attack_icon: u32_at(r, attack_icon_column)
                        .filter(|&i| i != 0)
                        .and_then(|i| icons.get(&i).cloned()),
                },
            );
        }
    }
    Ok(map)
}
