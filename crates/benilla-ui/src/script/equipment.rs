//! The 12340 equipment manager's natives (`WoW/Source/Ui/EquipmentManager_Scripts.cpp`) over the
//! client's set list (`CGEquipmentManager::s_equipmentSets`), which the app owns and pushes here
//! ([`UiScript::set_equipment_sets`]). The getters read that copy; the verbs that change a set
//! or swap gear go to the app as [`EquipmentSetCall`]s, applied inside the verb, which writes the
//! copy back before it signals. The ignored-slot marks (`m_ignoredSlots[19]`, `0xC1E908`) are
//! the VM's: only the natives below write them, and a save carries them.
//!
//! `PickupEquipmentSet` and `PickupEquipmentSetByName` (`0x5AF2C0`, `0x5AF250`) put a set on the
//! cursor (`CGGameUI::SetCursorEquipmentSet`, `0x520DC0`) as
//! [`super::cursor::CursorPayload::EquipmentSet`], which the action bar stores as
//! `0x20000000 | id` ([`super::action::ACTION_KIND_EQUIPMENT_SET`]); the bar's set queries read
//! the list here live. Not modelled: `CanPerformAction(14)`, the tainted-call refusal of the
//! by-name pickup, which the bar's own pickup and place verbs do not model either.

use mlua::{Lua, Table, Value};

use super::{Model, ScriptValue, UiScript};

/// `EQUIPMENT_SET::m_items[19]`.
pub const EQUIPMENT_SET_SLOTS: usize = 19;

/// `CanCreateSet` (`0x5AF460`): ten sets at most.
pub const MAX_EQUIPMENT_SETS: usize = 10;

/// The ignored-slot marker guid (`SaveSet`, `0x5ADE50`).
const IGNORED: u64 = 1;

/// `s_equipmentSetIconPath` (`0xACFB74`), the directory `GetEquipmentSetInfo` puts in front.
const ICON_PATH: &str = "Interface\\Icons\\";

/// The icon a save falls back to (`Script_SaveEquipmentSet`, `0x5AF9C0`).
pub const DEFAULT_SET_ICON: &str = "INV_Misc_QuestionMark";

/// One set as the client lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSetView {
    /// `m_id`, 0..=9.
    pub id: u32,
    pub name: String,
    /// The icon file name, without its directory.
    pub icon: String,
    /// One raw item guid per slot: 0 empty, 1 ignored.
    pub items: [u64; EQUIPMENT_SET_SLOTS],
}

/// The app's sets as the VM reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentSetsState {
    /// `m_loaded` (`0xC1E955`): the login list arrived; `SaveEquipmentSet` does nothing before.
    pub loaded: bool,
    /// The list in the client's order.
    pub sets: Vec<EquipmentSetView>,
    /// `m_swapPending` (`0xC1E956`): a use was sent and its result has not come.
    pub swap_pending: bool,
}

/// Where a save takes its icon: a macro-icon file name, or the item worn in equipment slot
/// `n` (0-based), whose inventory art the app reads (`0x5AF9C0`'s negative index).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSetIcon {
    Named(String),
    Slot(u8),
}

/// A set verb for the app, which owns the list and the sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSetCall {
    /// `SaveEquipmentSet` (`0x5AF9C0`): save over the named set, or create it
    /// (`SaveSetByName` `0x5AF8E0`, `CreateEquipmentSet` `0x5AF7E0`), with the ignored marks.
    Save {
        name: String,
        icon: EquipmentSetIcon,
        ignored: [bool; EQUIPMENT_SET_SLOTS],
    },
    /// `DeleteEquipmentSet` (`0x5AE800` → `0x5AE260`).
    Delete { name: String },
    /// `UseEquipmentSet` (`0x5AF380` → `0x5ADA20`) past the VM's gates (the CVar and the
    /// cursor): the per-slot locations, the send, the pending swap.
    Use { name: String },
    /// `RenameEquipmentSet` (`0x5AF910` → `ModifyEquipmentSet` `0x5AF400`), which saves.
    Rename {
        from: String,
        to: String,
        ignored: [bool; EQUIPMENT_SET_SLOTS],
    },
}

/// Where an item a set names is now, as the location getter reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentItemPlace {
    /// Worn, equipment slot 0..=18.
    Equipped(u8),
    /// The backpack, slot 0..=15.
    Backpack(u8),
    /// An equipped bag 0..=3, slot 0-based.
    Bag { bag: u8, slot: u8 },
    /// The bank vault 0..=27, reported only while a banker is open.
    Bank(u8),
    /// A bank bag 0..=6, slot 0-based, reported only while a banker is open.
    BankBag { bag: u8, slot: u8 },
    /// The vendor buyback list.
    Buyback,
    /// Anywhere else the getter does not look (the keyring, the bank while closed).
    Elsewhere,
}

/// One held item a set names: its entry and where it is. `None` from the host is an item the
/// object manager does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentItemView {
    pub entry: u32,
    pub place: EquipmentItemPlace,
}

impl EquipmentItemPlace {
    /// The packed location `GetEquipmentSetLocations` (`0x5AE860`) pushes, `None` for -1.
    fn location(self) -> Option<i64> {
        let (slot, base) = match self {
            Self::Equipped(i) => (i64::from(i), 0x10_0001),
            // Player-array slot 23 + k, plus 0x2FFFEA.
            Self::Backpack(k) => (i64::from(k), 0x30_0001),
            Self::Bag { bag, slot } => {
                (i64::from(slot), ((i64::from(bag) + 1) << 8 | 0x30_0000) + 1)
            }
            // Player-array slot 39 + j, plus 0x400001.
            Self::Bank(j) => (i64::from(j) + 39, 0x40_0001),
            Self::BankBag { bag, slot } => {
                (i64::from(slot), ((i64::from(bag) + 1) | 0x6000) << 8 | 1)
            }
            Self::Buyback | Self::Elsewhere => return None,
        };
        Some(base + slot)
    }
}

impl UiScript {
    /// Push the app's set list, its load latch and its pending swap.
    pub fn set_equipment_sets(&mut self, state: EquipmentSetsState) {
        self.model_mut().equipment_sets = state;
    }

    /// The set list as last pushed.
    pub fn equipment_sets(&self) -> EquipmentSetsState {
        self.model_ref().equipment_sets.clone()
    }

    /// 12340 `CGActionBar::UpdateEquipmentSet` (`0x5AAA00`), which `SaveSet` (`0x5ADE50`) runs for
    /// a set the server has numbered, past its copy and before its send: each slot holding set `id`,
    /// lowest first, signals `ACTIONBAR_SLOT_CHANGED`. The usable refresh it also runs is the app's
    /// next state push.
    pub fn signal_equipment_set_slots(&mut self, id: u32) {
        let mut held: Vec<u32> = self
            .model_ref()
            .actions
            .iter()
            .filter(|(_, s)| super::action::equipment_set_slot_id(s.kind, s.action) == Some(id))
            .map(|(&slot, _)| slot)
            .collect();
        held.sort_unstable();
        for slot in held {
            self.fire_event(
                "ACTIONBAR_SLOT_CHANGED",
                vec![ScriptValue::Int(i64::from(slot))],
            );
        }
    }

    /// The ignored-slot marks, for a harness.
    pub fn equipment_ignored_slots(&self) -> [bool; EQUIPMENT_SET_SLOTS] {
        self.model_ref().equipment_ignored
    }
}

fn model(lua: &Lua) -> mlua::AppDataRefMut<'_, Model> {
    lua.app_data_mut::<Model>().expect("model app_data")
}

fn set_by_name<'a>(state: &'a EquipmentSetsState, name: &str) -> Option<&'a EquipmentSetView> {
    state.sets.iter().find(|s| s.name == name)
}

/// `static_cast<uint32_t>(static_cast<int64_t>(lua_tonumber(L, 1))) - 1`, the slot test of the
/// four ignore verbs: a non-number reads 0, so slot -1 wraps past 18.
fn slot_arg(lua: &Lua, v: Value, usage: &'static str) -> mlua::Result<usize> {
    let n = lua.coerce_number(v)?.unwrap_or(0.0);
    let slot = (super::binding_abi::ftol(n) as u32).wrapping_sub(1);
    if slot > 18 {
        return Err(mlua::Error::runtime(usage));
    }
    Ok(slot as usize)
}

/// `lua_isstring(L, 1)`, then its text: a string or a number, else the usage error.
fn name_arg(lua: &Lua, v: Value, usage: &'static str) -> mlua::Result<String> {
    super::binding_abi::string_arg(lua, v, usage)
}

/// `ScrubString(source, dest, 32, 128, 1)` (`0x7E1830`): pipes stripped, the text cut at a
/// newline, and cut where it reaches 32 characters; a cut or a newline fails the scrub.
pub(crate) fn scrub_set_name(source: &str) -> Option<String> {
    let mut out: String = source.chars().filter(|&c| c != '|').collect();
    let mut valid = true;
    if let Some(at) = out.find(['\r', '\n']) {
        out.truncate(at);
        valid = false;
    }
    if let Some((at, _)) = out.char_indices().nth(31) {
        out.truncate(at);
        return None;
    }
    valid.then_some(out)
}

/// `CGEquipmentManager::CanUseEquipmentSets` (`0x5AD6B0`): no swap pending and the
/// `equipmentManager` CVar's integer nonzero. Deviation: the action bar's usable refresh it runs
/// on a change of the answer (`CGActionBar::UpdateUsable`) is not driven from here.
fn can_use(model: &Model) -> bool {
    !model.equipment_sets.swap_pending
        && model
            .cvars
            .get("equipmentmanager")
            .is_some_and(|c| super::binding_abi::s_str_to_int(&c.value) != 0)
}

/// `CGEquipmentManager::GetFullIconPath` (`0x5AE6B0`) of a set's stored icon: the inventory icon
/// directory and the file name, `None` for an empty icon, which formats the empty string.
pub(crate) fn full_icon_path(icon: &str) -> Option<String> {
    (!icon.is_empty()).then(|| format!("{ICON_PATH}{icon}"))
}

/// `GetFullIconPath` of set `id`, the empty string for a set gone or an empty icon: what the
/// bar's texture getter answers for a set slot (`0x5A7A90`).
pub(crate) fn set_icon_path(model: &Model, id: u32) -> String {
    model
        .equipment_sets
        .sets
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| full_icon_path(&s.icon))
        .unwrap_or_default()
}

/// `CGEquipmentManager::SetContainsLockedItems` (`0x5AD940`): a held item of `set` carries the
/// client lock and is not on the buyback list.
fn contains_locked_items(lua: &Lua, set: &EquipmentSetView) -> bool {
    let locks = model(lua).item_locks.clone();
    set.items.iter().any(|&guid| {
        guid > IGNORED
            && locks.contains(&guid)
            && super::host::with_host(lua, |host| host.equipment_item(guid))
                .flatten()
                .is_some_and(|item| item.place != EquipmentItemPlace::Buyback)
    })
}

/// `CGEquipmentManager::SetUsesLockedSlots` (`0x5AE040`): an item worn in a slot the set does not
/// ignore carries the client lock.
fn uses_locked_slots(model: &Model, set: &EquipmentSetView) -> bool {
    set.items.iter().enumerate().any(|(slot, &guid)| {
        guid != IGNORED
            && model
                .inventory_slots
                .get(slot + 1)
                .and_then(Option::as_ref)
                .is_some_and(|worn| super::cursor::item_locked(model, worn.guid))
    })
}

/// The set gates of 12340 `CGActionBar::UpdateUsableAction` (`0x5A9E20`, lines 333-349) for a
/// slot holding set `id`: the manager usable (`CanUseEquipmentSets(false)`), the set listed, none
/// of its items locked and none of its worn slots locked. The control and stun legs that follow
/// are the app's, in the slot's pushed state.
pub(crate) fn set_usable_on_bar(lua: &Lua, id: u32) -> bool {
    let set = {
        let m = model(lua);
        if !can_use(&m) {
            return false;
        }
        let Some(set) = m.equipment_sets.sets.iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        if uses_locked_slots(&m, &set) {
            return false;
        }
        set
    };
    !contains_locked_items(lua, &set)
}

/// `SetCursorEquipmentSet(id)` (`0x520DC0`) from a Lua verb: refused before its clear for an id
/// no set holds, else the clear and the set ([`super::cursor::set_cursor`]).
fn pick_up_set(lua: &Lua, id: u32) {
    let known = model(lua).equipment_sets.sets.iter().any(|s| s.id == id);
    if known {
        super::cursor::set_cursor(lua, true, |m| super::cursor::equipment_set_payload(m, id));
    }
}

fn signal_error(lua: &Lua, key: &str) {
    let text: String = lua.globals().get(key).unwrap_or_else(|_| key.to_string());
    super::tick::fire_event_into(lua, "UI_ERROR_MESSAGE", vec![ScriptValue::Str(text)]);
}

fn fill(lua: &Lua, table: Option<Table>) -> mlua::Result<Table> {
    match table {
        Some(t) => Ok(t),
        None => lua.create_table(),
    }
}

fn table_arg(v: &Value) -> Option<Table> {
    match v {
        Value::Table(t) => Some(t.clone()),
        _ => None,
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `EquipmentManagerIgnoreSlotForSave(slot)` (`0x5AD770`).
    g.set(
        "EquipmentManagerIgnoreSlotForSave",
        lua.create_function(|lua, slot: Value| {
            let slot = slot_arg(lua, slot, "Usage: EquipmentManagerIgnoreSlotForSave(slot)")?;
            model(lua).equipment_ignored[slot] = true;
            Ok(())
        })?,
    )?;
    // `EquipmentManagerUnignoreSlotForSave(slot)` (`0x5AD830`); its usage text names another verb.
    g.set(
        "EquipmentManagerUnignoreSlotForSave",
        lua.create_function(|lua, slot: Value| {
            let slot = slot_arg(lua, slot, "Usage: UpdatePendingEquipmentSetSlot(slot)")?;
            model(lua).equipment_ignored[slot] = false;
            Ok(())
        })?,
    )?;
    // `EquipmentManagerIsSlotIgnoredForSave(slot)` (`0x5AD7C0`): a boolean.
    g.set(
        "EquipmentManagerIsSlotIgnoredForSave",
        lua.create_function(|lua, slot: Value| {
            let slot = slot_arg(lua, slot, "Usage: EquipmentSetIsSlotIgnoredForSave(slot)")?;
            Ok(model(lua).equipment_ignored[slot])
        })?,
    )?;
    // `EquipmentManagerClearIgnoredSlotsForSave()` (`0x5AD820` → `0x5AD700`).
    g.set(
        "EquipmentManagerClearIgnoredSlotsForSave",
        lua.create_function(|lua, ()| {
            model(lua).equipment_ignored = [false; EQUIPMENT_SET_SLOTS];
            Ok(())
        })?,
    )?;
    // `CanUseEquipmentSets()` (`0x5AD880`): 1 or 0, a number.
    g.set(
        "CanUseEquipmentSets",
        lua.create_function(|lua, ()| Ok(i64::from(can_use(&model(lua)))))?,
    )?;
    // `GetNumEquipmentSets()` (`0x5AF0D0`).
    g.set(
        "GetNumEquipmentSets",
        lua.create_function(|lua, ()| Ok(model(lua).equipment_sets.sets.len() as i64))?,
    )?;
    // `GetEquipmentSetInfo(index)` (`0x5AF120`): name, `Interface\Icons\<icon>`, id; an index
    // outside 1..=10 raises, a free row answers nothing.
    g.set(
        "GetEquipmentSetInfo",
        lua.create_function(|lua, index: Value| {
            let n = lua.coerce_number(index)?.unwrap_or(0.0);
            let row = super::binding_abi::ftol(n).wrapping_sub(1) as u32;
            if row > 9 {
                return Err(mlua::Error::runtime("Usage: GetEquipmentSetInfo(setIndex)"));
            }
            let set = model(lua).equipment_sets.sets.get(row as usize).cloned();
            let Some(set) = set else {
                return Ok(mlua::MultiValue::new());
            };
            Ok(mlua::MultiValue::from_vec(vec![
                Value::String(lua.create_string(&set.name)?),
                Value::String(lua.create_string(format!("{ICON_PATH}{}", set.icon))?),
                Value::Integer(i64::from(set.id)),
            ]))
        })?,
    )?;
    // `GetEquipmentSetInfoByName(name)` (`0x5AF1D0`): the icon as stored (no directory) and the id.
    g.set(
        "GetEquipmentSetInfoByName",
        lua.create_function(|lua, name: Value| {
            let name = name_arg(lua, name, r#"Usage: GetEquipmentSetInfoByName("setName")"#)?;
            let set = set_by_name(&model(lua).equipment_sets, &name).cloned();
            let Some(set) = set.filter(|_| !name.is_empty()) else {
                return Ok(mlua::MultiValue::new());
            };
            Ok(mlua::MultiValue::from_vec(vec![
                Value::String(lua.create_string(&set.icon)?),
                Value::Integer(i64::from(set.id)),
            ]))
        })?,
    )?;
    // `GetEquipmentSetItemIDs(name [, table])` (`0x5AEF90`): per slot 1..=19 the item entry, 0
    // for an empty slot (the guid's low word zero), 1 for an ignored one, -1 for an item the
    // object manager does not hold.
    g.set(
        "GetEquipmentSetItemIDs",
        lua.create_function(|lua, (name, table): (Value, Value)| {
            let name = name_arg(
                lua,
                name,
                r#"Usage: GetEquipmentSetItemIDs("setName" [, returnTable])"#,
            )?;
            let set = set_by_name(&model(lua).equipment_sets, &name).cloned();
            let Some(set) = set else {
                return Ok(Value::Nil);
            };
            let out = fill(lua, table_arg(&table))?;
            for (slot, &guid) in set.items.iter().enumerate() {
                let value = if guid as u32 == 0 {
                    0
                } else if guid <= IGNORED {
                    guid as i64
                } else {
                    super::host::with_host(lua, |host| host.equipment_item(guid))
                        .flatten()
                        .map_or(-1, |item| i64::from(item.entry))
                };
                out.raw_set(slot as i64 + 1, value)?;
            }
            Ok(Value::Table(out))
        })?,
    )?;
    // `GetEquipmentSetLocations(name [, table])` (`0x5AE860`): per slot the packed location of
    // its item (`EquipmentManager_UnpackLocation`'s input), 0 empty, 1 ignored, -1 not found.
    g.set(
        "GetEquipmentSetLocations",
        lua.create_function(|lua, (name, table): (Value, Value)| {
            let usage = r#"Usage: GetEquipmentSetLocations("setName" [, returnTable])"#;
            if !matches!(table, Value::Nil | Value::Table(_)) {
                return Err(mlua::Error::runtime(usage));
            }
            let name = name_arg(lua, name, usage)?;
            let (set, player) = {
                let m = model(lua);
                (
                    set_by_name(&m.equipment_sets, &name).cloned(),
                    m.unit_guids.player,
                )
            };
            let Some(set) = set.filter(|_| player != 0) else {
                return Ok(Value::Nil);
            };
            let out = fill(lua, table_arg(&table))?;
            for (slot, &guid) in set.items.iter().enumerate() {
                let value = if guid <= IGNORED {
                    guid as i64
                } else {
                    super::host::with_host(lua, |host| host.equipment_item(guid))
                        .flatten()
                        .and_then(|item| item.place.location())
                        .unwrap_or(-1)
                };
                out.raw_set(slot as i64 + 1, value)?;
            }
            Ok(Value::Table(out))
        })?,
    )?;
    // `EquipmentSetContainsLockedItems(name)` (`0x5AF320` → `0x5AD940`): true when a held item
    // of the set carries the client lock (`m_itemFlags` bit 0) and is not on the buyback list.
    g.set(
        "EquipmentSetContainsLockedItems",
        lua.create_function(|lua, name: Value| {
            let name = name_arg(
                lua,
                name,
                r#"Usage: EquipmentSetContainsLockedItems("setName")"#,
            )?;
            let set = set_by_name(&model(lua).equipment_sets, &name).cloned();
            let Some(set) = set else {
                return Ok(mlua::MultiValue::new());
            };
            let locked = contains_locked_items(lua, &set);
            // `0x5AF320` pushes `true` or returns nothing.
            Ok(if locked {
                mlua::MultiValue::from_vec(vec![Value::Boolean(true)])
            } else {
                mlua::MultiValue::new()
            })
        })?,
    )?;
    // `SaveEquipmentSet(name, iconIndex)` (`0x5AF9C0`): nothing before the login list; the
    // name scrubbed; the icon a macro icon (`iconIndex` >= 0) or a worn slot's art (< 0); a new
    // name past ten sets raises.
    g.set(
        "SaveEquipmentSet",
        lua.create_function(|lua, (name, icon): (Value, Value)| {
            if !model(lua).equipment_sets.loaded {
                return Ok(());
            }
            let usage = r#"Usage: SaveEquipmentSet("setName", iconIndex)"#;
            let index = match icon {
                Value::Integer(i) => i as f64,
                Value::Number(n) => n,
                Value::String(ref s) => s
                    .to_str()
                    .ok()
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .ok_or_else(|| mlua::Error::runtime(usage))?,
                _ => return Err(mlua::Error::runtime(usage)),
            };
            let name = name_arg(lua, name, usage)?;
            let name = scrub_set_name(&name)
                .ok_or_else(|| mlua::Error::runtime("Invalid string for setName"))?;
            let index = super::binding_abi::ftol(index);
            let icon = if index >= 0 {
                super::macros::ensure_macro_icons(lua);
                let named = super::macros::icon_at(&model(lua), f64::from(index));
                EquipmentSetIcon::Named(named.unwrap_or_default())
            } else {
                match index.unsigned_abs() - 1 {
                    slot @ 0..=18 => EquipmentSetIcon::Slot(slot as u8),
                    _ => EquipmentSetIcon::Named(String::new()),
                }
            };
            let (exists, count, ignored) = {
                let m = model(lua);
                (
                    set_by_name(&m.equipment_sets, &name).is_some(),
                    m.equipment_sets.sets.len(),
                    m.equipment_ignored,
                )
            };
            if !exists && count >= MAX_EQUIPMENT_SETS {
                return Err(mlua::Error::runtime(format!(
                    "Too many sets! You can only have {MAX_EQUIPMENT_SETS} sets"
                )));
            }
            super::calls::submit(
                lua,
                super::ScriptCall::EquipmentSet(EquipmentSetCall::Save {
                    name,
                    icon,
                    ignored,
                }),
            )
        })?,
    )?;
    // `DeleteEquipmentSet(name)` (`0x5AE800`).
    g.set(
        "DeleteEquipmentSet",
        lua.create_function(|lua, name: Value| {
            let name = name_arg(lua, name, r#"Usage: DeleteEquipmentSet("setName")"#)?;
            let id = set_by_name(&model(lua).equipment_sets, &name).map(|s| s.id);
            let Some(id) = id.filter(|_| !name.is_empty()) else {
                return Ok(());
            };
            // `0x5AE260` empties the bar's slots of the set first (`RemoveEquipmentSet`,
            // `0x5AAE20`), each sent and signalled, then drops the set and sends the delete.
            super::tick::verb(lua, |m| super::cursor::clear_equipment_set_slots(m, id));
            super::calls::submit(
                lua,
                super::ScriptCall::EquipmentSet(EquipmentSetCall::Delete { name }),
            )
        })?,
    )?;
    // `RenameEquipmentSet(old, new)` (`0x5AF910`).
    g.set(
        "RenameEquipmentSet",
        lua.create_function(|lua, (from, to): (Value, Value)| {
            let usage = r#"Usage: RenameEquipmentSet("oldName", "newName")"#;
            let from = name_arg(lua, from, usage)?;
            let to = name_arg(lua, to, usage)?;
            let to = scrub_set_name(&to)
                .ok_or_else(|| mlua::Error::runtime("Invalid string for setName"))?;
            let (exists, ignored) = {
                let m = model(lua);
                (
                    set_by_name(&m.equipment_sets, &from).is_some(),
                    m.equipment_ignored,
                )
            };
            if !exists {
                return Ok(());
            }
            super::calls::submit(
                lua,
                super::ScriptCall::EquipmentSet(EquipmentSetCall::Rename { from, to, ignored }),
            )
        })?,
    )?;
    // `UseEquipmentSet(name)` (`0x5AF380` → `0x5ADA20`): true once the swap is sent, nil when
    // refused. The gates in the VM: the CVar (silent while a swap is pending, else
    // `EQUIPMENT_MANAGER_IS_DISABLED`) and an item on the cursor; the rest is the app's.
    g.set(
        "UseEquipmentSet",
        lua.create_function(|lua, name: Value| {
            let name = name_arg(lua, name, r#"Usage: UseEquipmentSet("setName")"#)?;
            let (exists, player, usable, pending, cursor_item) = {
                let m = model(lua);
                (
                    set_by_name(&m.equipment_sets, &name).is_some(),
                    m.unit_guids.player != 0,
                    can_use(&m),
                    m.equipment_sets.swap_pending,
                    matches!(m.cursor, Some(super::cursor::CursorPayload::Item(_))),
                )
            };
            if !exists {
                return Ok(mlua::MultiValue::new());
            }
            let refused = Ok(mlua::MultiValue::from_vec(vec![Value::Nil]));
            if !player {
                return refused;
            }
            if !usable {
                if !pending {
                    signal_error(lua, "EQUIPMENT_MANAGER_IS_DISABLED");
                }
                return refused;
            }
            if cursor_item {
                signal_error(lua, "SPELL_FAILED_CANT_DO_THAT_RIGHT_NOW");
                return refused;
            }
            super::calls::submit(
                lua,
                super::ScriptCall::EquipmentSet(EquipmentSetCall::Use { name }),
            )?;
            let sent = model(lua).equipment_sets.swap_pending;
            Ok(mlua::MultiValue::from_vec(vec![if sent {
                Value::Boolean(true)
            } else {
                Value::Nil
            }]))
        })?,
    )?;
    // `PickupEquipmentSetByName(name)` (`0x5AF250`): a non-empty name of a listed set goes on the
    // cursor; anything else does nothing.
    g.set(
        "PickupEquipmentSetByName",
        lua.create_function(|lua, name: Value| {
            let name = name_arg(lua, name, r#"Usage: PickupEquipmentSetByName("setName")"#)?;
            let id = set_by_name(&model(lua).equipment_sets, &name).map(|s| s.id);
            if let Some(id) = id.filter(|_| !name.is_empty()) {
                pick_up_set(lua, id);
            }
            Ok(())
        })?,
    )?;
    // `PickupEquipmentSet(index)` (`0x5AF2C0`): the set at 1-based list `index`
    // (`GetSetByIndex(trunc(index) - 1)`), nothing for a free row.
    g.set(
        "PickupEquipmentSet",
        lua.create_function(|lua, index: Value| {
            let n = super::binding_abi::is_number(lua, index)
                .ok_or_else(|| mlua::Error::runtime("Usage: PickupEquipmentSet(setIndex)"))?;
            let row = i64::from(super::binding_abi::ftol(n)) - 1;
            let id = usize::try_from(row)
                .ok()
                .and_then(|row| model(lua).equipment_sets.sets.get(row).map(|s| s.id));
            if let Some(id) = id {
                pick_up_set(lua, id);
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ScrubString(.., 32, .., 1)`: pipes go, a newline or a 32nd character fails the name.
    #[test]
    fn equipment_set_names_scrub_as_the_client_does() {
        assert_eq!(scrub_set_name("Tank|cff").as_deref(), Some("Tankcff"));
        assert_eq!(scrub_set_name("Two\nLines"), None);
        assert_eq!(scrub_set_name(&"x".repeat(31)).map(|s| s.len()), Some(31));
        assert_eq!(scrub_set_name(&"x".repeat(32)), None);
    }
}
