//! The 3.3.5 companion collections (`CGSpellBook::m_companions[2]`, `SpellBookFrame_Scripts.cpp`)
//! behind the stock `PetPaperDollFrame` mount and pet tabs.
//!
//! The app projects each collection from the learned spells, sorted by name (`SortCompanions`),
//! and fires `COMPANION_UPDATE`. `active` is read live off the player's auras, as the reference
//! asks the player's aura list for the companion's spell.
//!
//! Unsupported branch: type 0 (CRITTER) needs `SummonProperties.dbc` slot 5
//! (`SpellIsCallCompanion`), which is not loaded, so the app pushes an empty critter list.

use super::{calls, host, protected, Model, ScriptCall};
use mlua::{Lua, MultiValue, Value};
use protected::Tag;

/// One learned companion, `GetCompanionInfo`'s row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompanionView {
    /// `EffectMiscValue[0]`: the creature the companion summons or the mount displays.
    pub creature_id: u32,
    /// The creature's name; the app falls back to the spell's name when no creature name is known.
    pub name: String,
    pub spell_id: u32,
    pub icon: Option<String>,
    /// `ActiveIconID`'s texture, shown while the companion is out.
    pub active_icon: Option<String>,
}

/// `GetCompanionInfo`'s `type` token: `"CRITTER"` is 0, `"MOUNT"` is 1, else the usage raises.
pub(super) fn companion_type(lua: &Lua, value: Value, usage: &'static str) -> mlua::Result<usize> {
    let token = super::binding_abi::optional_string(lua, &value)
        .ok_or_else(|| mlua::Error::runtime(usage))?;
    if token.eq_ignore_ascii_case("CRITTER") {
        Ok(0)
    } else if token.eq_ignore_ascii_case("MOUNT") {
        Ok(1)
    } else {
        Err(mlua::Error::runtime(usage))
    }
}

/// The 1-based `index` argument; 0, a non-number or a missing one raises `usage`.
fn index_arg(lua: &Lua, index: Option<Value>, usage: &'static str) -> mlua::Result<usize> {
    let index = index
        .and_then(|index| lua.coerce_number(index).ok().flatten())
        .ok_or_else(|| mlua::Error::runtime(usage))?;
    if !index.is_finite() || index < 1.0 {
        return Err(mlua::Error::runtime(usage));
    }
    Ok(index as usize)
}

fn companion(lua: &Lua, kind: usize, index: usize) -> Option<CompanionView> {
    let model = lua.app_data_ref::<Model>().expect("model");
    model.companions[kind].get(index - 1).cloned()
}

/// The player's auras' spell ids, the `active` test.
fn player_aura_spells(lua: &Lua) -> Vec<u32> {
    let player = lua
        .app_data_ref::<Model>()
        .expect("model")
        .unit_guids
        .player;
    if player == 0 {
        return Vec::new();
    }
    host::with_host(lua, |host| host.unit_auras(player))
        .flatten()
        .map(|auras| auras.into_iter().map(|a| a.spell_id).collect())
        .unwrap_or_default()
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GetNumCompanions",
        lua.create_function(|lua, kind: Value| {
            let kind = companion_type(lua, kind, "Usage: GetNumCompanions(type)")?;
            let model = lua.app_data_ref::<Model>().expect("model");
            Ok(model.companions[kind].len() as i64)
        })?,
    )?;
    g.set(
        "GetCompanionInfo",
        lua.create_function(|lua, (kind, index): (Value, Option<Value>)| {
            const USAGE: &str = "Usage: GetCompanionInfo(type, index)";
            let kind = companion_type(lua, kind, USAGE)?;
            let index = index_arg(lua, index, USAGE)?;
            let Some(c) = companion(lua, kind, index) else {
                return Ok(MultiValue::new());
            };
            let active = player_aura_spells(lua).contains(&c.spell_id);
            let icon = if active {
                c.active_icon.clone().or(c.icon.clone())
            } else {
                c.icon.clone()
            };
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(c.creature_id)),
                Value::String(lua.create_string(&c.name)?),
                Value::Integer(i64::from(c.spell_id)),
                icon.map_or(Ok(Value::Nil), |icon| {
                    lua.create_string(&icon).map(Value::String)
                })?,
                if active {
                    Value::Integer(1)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )?;
    g.set(
        "GetCompanionCooldown",
        lua.create_function(|lua, (kind, index): (Value, Option<Value>)| {
            const USAGE: &str = "Usage: GetCompanionCooldown(type, index)";
            let kind = companion_type(lua, kind, USAGE)?;
            let index = index_arg(lua, index, USAGE)?;
            let now = super::clock::now(lua);
            let cooldown = companion(lua, kind, index).and_then(|c| {
                host::with_host(lua, |host| host.spell_cooldown(c.spell_id, false)).flatten()
            });
            Ok(super::spellbook::cooldown_tuple(cooldown, now))
        })?,
    )?;
    g.set(
        "CallCompanion",
        lua.create_function(|lua, (kind, index): (Value, Option<Value>)| {
            const USAGE: &str = "Usage: CallCompanion(type, index)";
            let kind = companion_type(lua, kind, USAGE)?;
            let index = index_arg(lua, index, USAGE)?;
            let Some(c) = companion(lua, kind, index) else {
                return Ok(());
            };
            calls::submit(
                lua,
                ScriptCall::CastSpell {
                    spell_id: c.spell_id,
                    on_self: false,
                    may_cast: protected::gate(lua, Tag::CAST),
                },
            )
        })?,
    )?;
    g.set(
        "DismissCompanion",
        lua.create_function(|lua, kind: Value| {
            let kind = companion_type(lua, kind, "Usage: DismissCompanion(type)")?;
            let auras = player_aura_spells(lua);
            let active: Vec<u32> = {
                let model = lua.app_data_ref::<Model>().expect("model");
                model.companions[kind]
                    .iter()
                    .map(|c| c.spell_id)
                    .filter(|id| auras.contains(id))
                    .collect()
            };
            for spell_id in active {
                super::aura::cancel_aura(lua, spell_id)?;
            }
            Ok(())
        })?,
    )?;
    g.set(
        "PickupCompanion",
        lua.create_function(|lua, (kind, index): (Value, Option<Value>)| {
            const USAGE: &str = "Usage: PickupCompanion(type, index)";
            let kind = companion_type(lua, kind, USAGE)?;
            let index = index_arg(lua, index, USAGE)?;
            if let Some(c) = companion(lua, kind, index) {
                super::spellbook::pickup_spell_id(lua, c.spell_id, c.icon);
            }
            Ok(())
        })?,
    )?;
    // `SummonRandomCritter()`: a random critter from type 0, which is empty here (see the module).
    g.set(
        "SummonRandomCritter",
        lua.create_function(|lua, ()| {
            let pick = {
                let model = lua.app_data_ref::<Model>().expect("model");
                let critters = &model.companions[0];
                (!critters.is_empty()).then(|| {
                    let at = (super::clock::now(lua) * 1000.0) as usize % critters.len();
                    critters[at].spell_id
                })
            };
            if let Some(spell_id) = pick {
                calls::submit(
                    lua,
                    ScriptCall::CastSpell {
                        spell_id,
                        on_self: false,
                        may_cast: protected::gate(lua, Tag::CAST),
                    },
                )?;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

impl super::UiScript {
    /// Replace the companion collections, `[critters, mounts]`, each sorted by name. The caller
    /// fires `COMPANION_UPDATE` when either changed.
    pub fn set_companions(&mut self, companions: [Vec<CompanionView>; 2]) {
        self.model_mut().companions = companions;
    }
}
