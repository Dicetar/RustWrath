//! The 3.3.5 minimap tracking menu: `GetNumTrackingTypes`, `GetTrackingInfo` and `SetTracking`
//! (12340 `MinimapFrame_Scripts.cpp`), the natives the stock `MiniMapTrackingDropDown_Initialize`
//! (`Minimap.lua:424`) enumerates.
//!
//! The menu lists the player's tracking spells first ([`Model::tracking_spells`], the reference's
//! `s_trackingSpells`, pushed by the host from the spellbook), then the town-service rows of
//! [`TRACKING_TYPES`] the player's class may use. Choosing a row stores its name in the
//! `minimapTrackedInfo` CVar, as `CGMinimapFrame::LoadTrackingInfo` does: that CVar is the single
//! record of the active row, so it survives the session the way the reference's does, and the
//! host's minimap and quest markers read it through [`tracked_info`].

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// What a [`TrackingType`] row matches (`TrackingTypeInfo::m_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackingKind {
    /// Type 1: a unit whose `UNIT_NPC_FLAGS` share a bit with [`TrackingType::flags`]
    /// (`CGUnit_C::IsTracked`).
    NpcFlag,
    /// Type 2: a GameObject whose type id equals [`TrackingType::flags`]
    /// (`CGGameObject_C::IsTracked`).
    GameObjectType,
    /// Type 3: no object; the low-level quest statuses 2-4 show (`IsTrackingTrivialQuests`).
    TrivialQuests,
}

/// One row of the reference's `s_trackingTypes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingType {
    pub kind: TrackingKind,
    /// NPC-flag mask for [`TrackingKind::NpcFlag`], GameObject type id for
    /// [`TrackingKind::GameObjectType`].
    pub flags: u32,
    /// The global string naming the row, also the `minimapTrackedInfo` value.
    pub name: &'static str,
    /// The file under `Interface\Minimap\Tracking`.
    pub texture: &'static str,
    /// `1 << class` bits allowed to use the row; 0 for every class.
    pub class_mask: u32,
}

const fn row(
    kind: TrackingKind,
    flags: u32,
    name: &'static str,
    texture: &'static str,
    class_mask: u32,
) -> TrackingType {
    TrackingType {
        kind,
        flags,
        name,
        texture,
        class_mask,
    }
}

use TrackingKind::{GameObjectType, NpcFlag, TrivialQuests};

/// 12340 `CGMinimapFrame::s_trackingTypes`, in menu order.
pub const TRACKING_TYPES: [TrackingType; 15] = [
    row(NpcFlag, 0x1000, "MINIMAP_TRACKING_REPAIR", "Repair", 0),
    row(NpcFlag, 0x200, "MINIMAP_TRACKING_VENDOR_FOOD", "Food", 0),
    // Rogues.
    row(NpcFlag, 0x400, "MINIMAP_TRACKING_VENDOR_POISON", "Poisons", 0x10),
    // Warriors, hunters and rogues.
    row(NpcFlag, 0x100, "MINIMAP_TRACKING_VENDOR_AMMO", "Ammunition", 0x1a),
    row(NpcFlag, 0x800, "MINIMAP_TRACKING_VENDOR_REAGENT", "Reagents", 0),
    row(NpcFlag, 0x10000, "MINIMAP_TRACKING_INNKEEPER", "Innkeeper", 0),
    row(NpcFlag, 0x2000, "MINIMAP_TRACKING_FLIGHTMASTER", "FlightMaster", 0),
    // Hunters.
    row(NpcFlag, 0x400000, "MINIMAP_TRACKING_STABLEMASTER", "StableMaster", 0x8),
    row(NpcFlag, 0x100000, "MINIMAP_TRACKING_BATTLEMASTER", "BattleMaster", 0),
    row(NpcFlag, 0x20, "MINIMAP_TRACKING_TRAINER_CLASS", "Class", 0),
    row(NpcFlag, 0x40, "MINIMAP_TRACKING_TRAINER_PROFESSION", "Profession", 0),
    row(NpcFlag, 0x200000, "MINIMAP_TRACKING_AUCTIONEER", "Auctioneer", 0),
    row(NpcFlag, 0x20000, "MINIMAP_TRACKING_BANKER", "Banker", 0),
    // GAMEOBJECT_TYPE_MAILBOX.
    row(GameObjectType, 19, "MINIMAP_TRACKING_MAILBOX", "Mailbox", 0),
    row(TrivialQuests, 0, "MINIMAP_TRACKING_TRIVIAL_QUESTS", "TrivialQuests", 0),
];

/// The CVar `LoadTrackingInfo` writes the active row's name to (empty for none).
pub const TRACKED_INFO_CVAR: &str = "minimapTrackedInfo";

const TRACKING_DIR: &str = "Interface\\Minimap\\Tracking";

/// The row `minimapTrackedInfo` names, matched case-blind as the reference's load does; `None`
/// for an empty or unknown value.
pub fn tracked_info(cvar_value: &str) -> Option<&'static TrackingType> {
    if cvar_value.is_empty() {
        return None;
    }
    TRACKING_TYPES
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(cvar_value))
}

/// The class id (`UNIT_FIELD_BYTES_0` byte 1) of a `ChrClasses` file token.
fn class_id(class_file: &str) -> Option<u8> {
    Some(match class_file {
        "WARRIOR" => 1,
        "PALADIN" => 2,
        "HUNTER" => 3,
        "ROGUE" => 4,
        "PRIEST" => 5,
        "DEATHKNIGHT" => 6,
        "SHAMAN" => 7,
        "MAGE" => 8,
        "WARLOCK" => 9,
        "DRUID" => 11,
        _ => return None,
    })
}

/// The rows the player's class may use, in table order (`CountAvailableTrackingTypes`,
/// `GetTrackingTypeByIndex`). With no player class only the all-class rows pass, as the
/// reference's zero mask does.
pub fn available(class_file: Option<&str>) -> impl Iterator<Item = &'static TrackingType> {
    let class_mask = class_file
        .and_then(class_id)
        .map_or(0, |c| 1u32 << c);
    TRACKING_TYPES
        .iter()
        .filter(move |t| t.class_mask == 0 || t.class_mask & class_mask != 0)
}

fn player_class(model: &Model) -> Option<String> {
    model.unit("player").and_then(|u| u.class_file.clone())
}

/// The active row, read back from its CVar.
pub(crate) fn active_info(model: &Model) -> Option<&'static TrackingType> {
    model
        .cvars
        .get(&TRACKED_INFO_CVAR.to_ascii_lowercase())
        .and_then(|slot| tracked_info(&slot.value))
}

/// `LoadTrackingInfo(info)`: the CVar takes the row's name (or ""), and
/// `MINIMAP_UPDATE_TRACKING` is raised.
pub(crate) fn load_tracking_info(model: &mut Model, info: Option<&'static TrackingType>) {
    super::cvars::set_from_engine(
        model,
        TRACKED_INFO_CVAR,
        info.map_or(String::new(), |t| t.name.to_string()),
    );
    model.raise("MINIMAP_UPDATE_TRACKING", Vec::new());
}

/// `GetTrackingTexture` on 3.3.5: the active row's texture, else the tracking spell's icon, else
/// `Interface\Minimap\Tracking\None`.
pub(crate) fn tracking_texture(model: &Model) -> String {
    if let Some(info) = active_info(model) {
        return format!("{TRACKING_DIR}\\{}", info.texture);
    }
    model
        .tracking
        .as_ref()
        .and_then(|t| t.icon.clone())
        .unwrap_or_else(|| format!("{TRACKING_DIR}\\None"))
}

/// The 1-based menu index as `Script_GetTrackingInfo` truncates it, made 0-based.
fn menu_index(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i - 1),
        Value::Number(n) => Some(*n as i64 - 1),
        Value::String(s) => s.to_str().ok()?.trim().parse::<f64>().ok().map(|n| n as i64 - 1),
        _ => None,
    }
}

/// One menu row: a tracking spell by its spellbook slot, or a table row.
enum Entry {
    Spell(u32),
    Info(&'static TrackingType),
}

fn entry(model: &Model, index: i64) -> Option<Entry> {
    let index = usize::try_from(index).ok()?;
    let spells = model.tracking_spells.len();
    if index < spells {
        return Some(Entry::Spell(model.tracking_spells[index]));
    }
    let class = player_class(model);
    available(class.as_deref())
        .nth(index - spells)
        .map(Entry::Info)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    g.set(
        "GetNumTrackingTypes",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let class = player_class(&model);
            Ok((model.tracking_spells.len() + available(class.as_deref()).count()) as i64)
        })?,
    )?;

    // GetTrackingInfo(index) -> name, texture, active, category ("spell" or "other"); nothing for
    // an index past the menu.
    g.set(
        "GetTrackingInfo",
        lua.create_function(|lua, index: Value| {
            let Some(index) = menu_index(&index) else {
                return Ok(MultiValue::new());
            };
            let (name, texture, active, category) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                match entry(&model, index) {
                    None => return Ok(MultiValue::new()),
                    Some(Entry::Info(info)) => (
                        Some(super::glue_core::global_text(lua, info.name)),
                        Some(format!("{TRACKING_DIR}\\{}", info.texture)),
                        active_info(&model) == Some(info),
                        "other",
                    ),
                    Some(Entry::Spell(spell_id)) => {
                        let tracking = model.tracking.as_ref().filter(|t| t.spell_id == spell_id);
                        let slot = model
                            .spellbook
                            .slots
                            .iter()
                            .find(|s| s.spell_id == spell_id);
                        // The active spell answers its active icon, which the aura push carries.
                        let texture = tracking
                            .and_then(|t| t.icon.clone())
                            .or_else(|| slot.and_then(|s| s.texture.clone()));
                        let name = slot.map(|s| s.name.clone()).or_else(|| {
                            tracking.and_then(|t| t.name.clone())
                        });
                        (name, texture, tracking.is_some(), "spell")
                    }
                }
            };
            let text = |s: Option<String>| -> mlua::Result<Value> {
                Ok(match s {
                    Some(s) => Value::String(lua.create_string(&s)?),
                    None => Value::Nil,
                })
            };
            Ok(MultiValue::from_vec(vec![
                text(name)?,
                text(texture)?,
                if active { Value::Number(1.0) } else { Value::Nil },
                Value::String(lua.create_string(category)?),
            ]))
        })?,
    )?;

    // SetTracking([index]): no index clears both the tracking spell (its aura is cancelled) and
    // the row; a spell index casts the spell; a row index cancels any tracking spell and makes
    // the row active.
    g.set(
        "SetTracking",
        lua.create_function(|lua, index: Option<Value>| {
            let index = index.as_ref().and_then(menu_index);
            let (chosen, spell) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let chosen = index.and_then(|i| entry(&model, i));
                (chosen, model.tracking.as_ref().map(|t| t.spell_id))
            };
            match chosen {
                Some(Entry::Spell(spell_id)) => super::calls::submit(
                    lua,
                    super::ScriptCall::CastSpell {
                        spell_id,
                        on_self: false,
                        may_cast: true,
                    },
                ),
                // An index past the menu does nothing.
                None if index.is_some() => Ok(()),
                info => {
                    if let Some(spell_id) = spell {
                        super::aura::cancel_aura(lua, spell_id)?;
                    }
                    let info = match info {
                        Some(Entry::Info(info)) => Some(info),
                        _ => None,
                    };
                    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                    load_tracking_info(&mut model, info);
                    Ok(())
                }
            }
        })?,
    )?;

    Ok(())
}

impl super::UiScript {
    /// Push the player's tracking spells, the spellbook's spells with a tracking aura in book
    /// order (`s_trackingSpells`), which head the tracking menu.
    pub fn set_tracking_spells(&mut self, spell_ids: Vec<u32>) {
        self.model_mut().tracking_spells = spell_ids;
    }

    /// The active tracking-menu row, from `minimapTrackedInfo`.
    pub fn minimap_tracked_info(&self) -> Option<&'static TrackingType> {
        active_info(&self.model_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A warrior's menu is the twelve all-class rows plus Ammunition; a hunter's adds the stable
    /// master and loses nothing; a rogue gets poisons; no row names an unknown value.
    #[test]
    fn the_menu_rows_follow_the_class_mask() {
        let names = |class: &str| -> Vec<&str> { available(Some(class)).map(|t| t.name).collect() };
        let warrior = names("WARRIOR");
        assert_eq!(warrior.len(), 13);
        assert!(warrior.contains(&"MINIMAP_TRACKING_VENDOR_AMMO"));
        assert!(!warrior.contains(&"MINIMAP_TRACKING_VENDOR_POISON"));
        assert!(!warrior.contains(&"MINIMAP_TRACKING_STABLEMASTER"));
        assert!(names("HUNTER").contains(&"MINIMAP_TRACKING_STABLEMASTER"));
        assert!(names("ROGUE").contains(&"MINIMAP_TRACKING_VENDOR_POISON"));
        assert_eq!(available(None).count(), 12);
        assert_eq!(
            tracked_info("minimap_tracking_trivial_quests").map(|t| t.kind),
            Some(TrackingKind::TrivialQuests)
        );
        assert_eq!(tracked_info(""), None);
    }
}
