//! `IsInInstance`, `CanShowResetInstances` and `ResetInstances`, the whole Lua side of instance
//! lockouts; the rest arrives as `CHAT_MSG_SYSTEM` lines. The readers answer off state the app
//! pushes from `Map.dbc`.

use mlua::{Lua, MultiValue, Value};

use super::party::PartyState;
use super::Model;
use crate::strings::{fill, global, Arg};

/// `IsInInstance()`'s type word for a `Map.dbc` `InstanceType`: the reference's table at
/// `0x83de58`, and `"none"` from 4 up.
pub fn instance_type_name(instance_type: u32) -> &'static str {
    match instance_type {
        1 => "party",
        2 => "raid",
        3 => "pvp",
        _ => "none",
    }
}

/// The current map's `Map.dbc` facts the Wrath `GetInstanceInfo` answers, pushed by the app at
/// world entry: the instance's name, its `InstanceType` and the player cap the DBC states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstanceInfo {
    /// `MapName_Lang` (`Map.dbc` field 4); nil outside an instance.
    pub name: Option<String>,
    /// `Map.dbc` field 2: 0 none, 1 party, 2 raid, 3 battleground.
    pub instance_type: u32,
    /// The instance's player cap; 0 where the map/DBC does not state one (raid and battleground
    /// caps come from difficulty tables that have no producer yet).
    pub max_players: u32,
}

impl super::UiScript {
    /// Push the current map's `Map.dbc` facts. Sets [`Model::instance_type`] too, so
    /// `IsInInstance` and `GetInstanceInfo` share one source.
    pub fn set_instance_info(&mut self, info: Option<InstanceInfo>) {
        let mut model = self.model_mut();
        let instance_type = info.as_ref().map(|i| i.instance_type);
        if model.instance_type != instance_type {
            model.instance_type = instance_type;
        }
        if model.instance_info != info {
            model.instance_info = info;
        }
    }

    /// Push the current map's `Map.dbc` `InstanceType`, `None` for a map with no row. Deviation:
    /// `IsInInstance()` then answers `nil, "none"`, because the reference returns two results
    /// without pushing either (`0x48a7a9`), a stack bug.
    pub fn set_instance_type(&mut self, instance_type: Option<u32>) {
        let mut model = self.model_mut();
        if model.instance_type != instance_type {
            model.instance_type = instance_type;
        }
    }

    /// Push `CanShowResetInstances()`'s answer; the app owns its terms (the ownership latch, the
    /// last dungeon, its age and the current map).
    pub fn set_can_reset_instances(&mut self, can: bool) {
        let mut model = self.model_mut();
        if model.can_reset_instances != can {
            model.can_reset_instances = can;
        }
    }

    /// The `ResetInstances()` calls a harness's VM kept ([`Self::keep_calls`]).
    pub fn take_reset_instance_asks(&mut self) -> u32 {
        self.take_calls_like(&super::ScriptCall::Instance(InstanceCall::Reset)) as u32
    }
}

/// The lockouts' verb the host applies inside its call ([`super::ScriptCall::Instance`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceCall {
    /// `ResetInstances()` (`0x48a6b0`): `CMSG_RESET_INSTANCES` (`0x48a6e6`).
    Reset,
    /// `SetDungeonDifficulty(n)` (`0x526050`): `MSG_SET_DUNGEON_DIFFICULTY` (3.3.5), one u32, the
    /// 0-based difficulty (`n - 1`); the server answers with the same opcode.
    SetDungeonDifficulty(u32),
    /// `SetRaidDifficulty(n)` (`0x5261A0`): `MSG_SET_RAID_DIFFICULTY` (3.3.5), one u32, 0-based.
    SetRaidDifficulty(u32),
}

/// The 3.3.5a difficulty cells, all 0-based as the wire carries them and zero until a packet
/// writes them, as the reference's statics are: `CGGameUI::m_instanceDifficulty`,
/// `m_dungeonDifficulty` and `m_raidDifficulty`, and `CGPartyInfo::m_dungeonDifficulty`,
/// `m_raidDifficulty` and `m_dynamicDifficulty` (`GameUI.cpp`, `PartyFrame.cpp`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Difficulty {
    /// The instance we stand in (`SMSG_INSTANCE_DIFFICULTY`'s first word): `GetInstanceDifficulty`
    /// and `GetInstanceInfo`'s third value, plus one.
    pub instance: u32,
    /// The group's dynamic (heroic-toggle) difficulty: `SMSG_INSTANCE_DIFFICULTY`'s second word
    /// and `SMSG_GROUP_LIST`'s third difficulty byte.
    pub dynamic: u32,
    /// The player's own dungeon setting (the echo's `player` word, or our own
    /// `SetDungeonDifficulty`).
    pub dungeon: u32,
    /// The player's own raid setting.
    pub raid: u32,
    /// The group's dungeon setting (the echo's `in_group` word, or `SMSG_GROUP_LIST`).
    pub party_dungeon: u32,
    /// The group's raid setting.
    pub party_raid: u32,
    /// Deviation: the `(dungeon, raid)` pair [`Self::current_dungeon`] and [`Self::current_raid`]
    /// answered after the last difficulty write. `SMSG_GROUP_LIST` reads both before it rebuilds
    /// the roster (`Player_C.cpp:11624`) and compares after (`0x5248E0`); here the roster reaches
    /// the model before the list's bytes do, so the "before" is this pair.
    pub announced: (u32, u32),
}

impl Difficulty {
    /// `GetCurrentDungeonDifficulty` (`0x5138D0`): the group's while in a party that is not a
    /// raid, else the player's own.
    pub fn current_dungeon(&self, party: &PartyState) -> u32 {
        if in_party(party) && party.raid.is_empty() {
            self.party_dungeon
        } else {
            self.dungeon
        }
    }

    /// `GetCurrentRaidDifficulty` (`0x513900`): the group's while in a raid, else the player's.
    pub fn current_raid(&self, party: &PartyState) -> u32 {
        if party.raid.is_empty() {
            self.raid
        } else {
            self.party_raid
        }
    }

    /// Record what the getters now answer, the next group list's "before".
    fn settle(&mut self, party: &PartyState) {
        self.announced = (self.current_dungeon(party), self.current_raid(party));
    }
}

/// `CGPartyInfo::m_members[0]`: a filled first party slot.
fn in_party(party: &PartyState) -> bool {
    party.members.first().is_some_and(|m| m.guid != 0)
}

impl super::UiScript {
    /// `SMSG_INSTANCE_DIFFICULTY` (`GameUI.cpp:6682`): the instance's difficulty and the group's
    /// dynamic one, two stores and no line. AzerothCore sends it after every map add
    /// (`Player.cpp:11685`).
    pub fn set_instance_difficulty(&mut self, difficulty: u32, dynamic: u32) {
        let mut model = self.model_mut();
        model.difficulty.instance = difficulty;
        model.difficulty.dynamic = dynamic;
    }

    /// The server's `MSG_SET_DUNGEON_DIFFICULTY` (`GameUI.cpp:6671`): `difficulty`, then whether
    /// it is the player's own setting and whether it is the group's.
    pub fn apply_dungeon_difficulty(&mut self, difficulty: u32, player: bool, in_group: bool) {
        set_dungeon_difficulty(self.lua(), difficulty, player, in_group);
    }

    /// The server's `MSG_SET_RAID_DIFFICULTY` (`GameUI.cpp:6796`), shaped as the dungeon echo.
    pub fn apply_raid_difficulty(&mut self, difficulty: u32, player: bool, in_group: bool) {
        set_raid_difficulty(self.lua(), difficulty, player, in_group);
    }

    /// `SMSG_GROUP_LIST`'s difficulty bytes `(dungeon, raid, dynamic)`, `None` for the empty
    /// list that leaves a group (`Player_C.cpp:11797`), which keeps the cells and lets the getters
    /// fall back to the player's own. Then `CheckInstanceDifficultyMessages` (`0x5248E0`).
    pub fn apply_group_difficulty(&mut self, listed: Option<(u8, u8, u8)>) {
        let (raid_changed, dungeon_changed) = {
            let mut guard = self.model_mut();
            let m = &mut *guard;
            let (dungeon_before, raid_before) = m.difficulty.announced;
            if let Some((dungeon, raid, dynamic)) = listed {
                m.difficulty.party_dungeon = u32::from(dungeon);
                m.difficulty.party_raid = u32::from(raid);
                m.difficulty.dynamic = u32::from(dynamic);
            }
            m.difficulty.settle(&m.party);
            let (dungeon_now, raid_now) = m.difficulty.announced;
            (raid_before != raid_now, dungeon_before != dungeon_now)
        };
        // The reference's order: the raid line, then the dungeon line.
        if raid_changed {
            show_raid_difficulty_changed(self.lua());
        }
        if dungeon_changed {
            show_dungeon_difficulty_changed(self.lua());
        }
    }

    /// The difficulty cells, for a host that reads them.
    pub fn difficulty(&self) -> Difficulty {
        self.model_ref().difficulty
    }

    /// Seat cells a previous VM held: the reference's are engine statics a `/reload` keeps, so a
    /// fresh VM starts from them, not from zero. No line.
    pub fn restore_difficulty(&mut self, difficulty: Difficulty) {
        self.model_mut().difficulty = difficulty;
    }
}

/// `CGGameUI::SetDungeonDifficulty` (`0x525530`): store into the player's cell, the group's or
/// both, and print the change line when the effective difficulty moved.
fn set_dungeon_difficulty(lua: &Lua, difficulty: u32, player: bool, in_group: bool) {
    let changed = {
        let mut guard = lua.app_data_mut::<Model>().expect("model app_data");
        let m = &mut *guard;
        let previous = m.difficulty.current_dungeon(&m.party);
        if player {
            m.difficulty.dungeon = difficulty;
        }
        if in_group {
            m.difficulty.party_dungeon = difficulty;
        }
        m.difficulty.settle(&m.party);
        previous != m.difficulty.current_dungeon(&m.party)
    };
    if changed {
        show_dungeon_difficulty_changed(lua);
    }
}

/// `CGGameUI::SetRaidDifficulty` (`0x5255A0`): as the dungeon store, then the raid line if the raid
/// difficulty moved and the dungeon line if the dungeon one did.
///
/// Not wired: on a dynamic-difficulty map (`Map.dbc` flag `0x100`, Icecrown Citadel and the Ruby
/// Sanctum) a group write is `CGPartyInfo::ChangeRaidDifficulty`, which sends
/// `CMSG_CHANGEPLAYER_DIFFICULTY` instead of storing. The map catalog carries no flags, so every
/// map takes the store.
fn set_raid_difficulty(lua: &Lua, difficulty: u32, player: bool, in_group: bool) {
    let (raid_changed, dungeon_changed) = {
        let mut guard = lua.app_data_mut::<Model>().expect("model app_data");
        let m = &mut *guard;
        let raid_before = m.difficulty.current_raid(&m.party);
        let dungeon_before = m.difficulty.current_dungeon(&m.party);
        if player {
            m.difficulty.raid = difficulty;
        }
        if in_group {
            m.difficulty.party_raid = difficulty;
        }
        m.difficulty.settle(&m.party);
        (
            raid_before != m.difficulty.current_raid(&m.party),
            dungeon_before != m.difficulty.current_dungeon(&m.party),
        )
    };
    if raid_changed {
        show_raid_difficulty_changed(lua);
    }
    if dungeon_changed {
        show_dungeon_difficulty_changed(lua);
    }
}

/// `ShowDungeonDifficultyChanged` (`0x524720`): `ERR_DUNGEON_DIFFICULTY_CHANGED_S` with
/// `DUNGEON_DIFFICULTY<n>` ("Normal", "Heroic"), `n` the effective difficulty plus one.
fn show_dungeon_difficulty_changed(lua: &Lua) {
    let current = {
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        m.difficulty.current_dungeon(&m.party)
    };
    show_difficulty_line(
        lua,
        "ERR_DUNGEON_DIFFICULTY_CHANGED_S",
        &format!("DUNGEON_DIFFICULTY{}", current + 1),
    );
}

/// `SendRaidDifficultyMessage` (`0x524780`): `ERR_RAID_DIFFICULTY_CHANGED_S` with
/// `RAID_DIFFICULTY<n>` ("10 Player" .. "25 Player (Heroic)").
///
/// Deviation: inside a raid instance the reference names the instance's own `MapDifficulty.dbc`
/// row (its `difficultystring`, for `m_instanceDifficulty`), which no catalog here loads; the
/// stock rows name the same four `RAID_DIFFICULTY` texts, so the instance's difficulty picks one.
/// The dynamic-map branch (`ERR_RAID_LOCKOUT_CHANGED_S` and `ERR_PLAYER_DIFFICULTY_CHANGED_S`) is
/// not wired, as [`set_raid_difficulty`] says.
fn show_raid_difficulty_changed(lua: &Lua) {
    let n = {
        let m = lua.app_data_ref::<Model>().expect("model app_data");
        if m.instance_type == Some(2) {
            m.difficulty.instance
        } else {
            m.difficulty.current_raid(&m.party)
        }
    };
    show_difficulty_line(
        lua,
        "ERR_RAID_DIFFICULTY_CHANGED_S",
        &format!("RAID_DIFFICULTY{}", n + 1),
    );
}

/// `DisplayError` on a chat-kind row whose `%s` is `FrameScript_GetText(token)`: a system chat line
/// at once, nothing when the VM lacks the template.
fn show_difficulty_line(lua: &Lua, key: &'static str, token: &str) {
    let Some(template) = global(lua, key) else {
        return;
    };
    let name = global(lua, token).unwrap_or_else(|| token.to_string());
    let text = fill(&template, &[Arg::S(&name)]);
    if !text.is_empty() {
        super::tick::show_chat_line_into(lua, key, text);
    }
}

/// `SetDungeonDifficulty(n)` (`0x526050`): a non-number raises its usage; `n - 1` above 1 is
/// ignored; a group member who does not lead sees `ERR_NOT_LEADER`. Otherwise both cells take the
/// difficulty at once, the change line prints, and the request goes out.
pub(super) fn set_dungeon_difficulty_verb(lua: &Lua, value: Value) -> mlua::Result<()> {
    let n = super::binding_abi::float_arg(lua, value, "Usage: SetDungeonDifficulty(difficulty)")?;
    // `static_cast<uint32_t>(static_cast<int64_t>(n - 1.0))`.
    let difficulty = (n - 1.0) as i64 as u32;
    if difficulty > 1 {
        return Ok(());
    }
    if refused_not_leader(lua) {
        return super::calls::display_error(lua, "ERR_NOT_LEADER");
    }
    let changed = {
        let mut guard = lua.app_data_mut::<Model>().expect("model app_data");
        let m = &mut *guard;
        let previous = m.difficulty.current_dungeon(&m.party);
        m.difficulty.dungeon = difficulty;
        m.difficulty.party_dungeon = difficulty;
        m.difficulty.settle(&m.party);
        previous != difficulty
    };
    if changed {
        show_dungeon_difficulty_changed(lua);
    }
    super::calls::submit(
        lua,
        super::ScriptCall::Instance(InstanceCall::SetDungeonDifficulty(difficulty)),
    )
}

/// `SetRaidDifficulty(n)` (`0x5261A0`): as the dungeon verb with four difficulties, storing through
/// [`set_raid_difficulty`] into both cells before the request goes out.
pub(super) fn set_raid_difficulty_verb(lua: &Lua, value: Value) -> mlua::Result<()> {
    let n = super::binding_abi::float_arg(lua, value, "Usage: SetRaidDifficulty(difficulty)")?;
    let difficulty = (n - 1.0) as i64 as u32;
    if difficulty > 3 {
        return Ok(());
    }
    if refused_not_leader(lua) {
        return super::calls::display_error(lua, "ERR_NOT_LEADER");
    }
    set_raid_difficulty(lua, difficulty, true, true);
    super::calls::submit(
        lua,
        super::ScriptCall::Instance(InstanceCall::SetRaidDifficulty(difficulty)),
    )
}

/// The setters' leader gate: in a party or raid (`m_members[0] || m_numMembers`) and the cached
/// leader is not the active player.
fn refused_not_leader(lua: &Lua) -> bool {
    let m = lua.app_data_ref::<Model>().expect("model app_data");
    (in_party(&m.party) || !m.party.raid.is_empty()) && m.party.leader_guid != m.party.own_guid
}

/// Register the three lockout globals.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // `IsInInstance()` (`0x48a750`): 1 if `InstanceType` is nonzero, else nil; then the type word.
    g.set(
        "IsInInstance",
        lua.create_function(|lua, ()| {
            let ty = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                model.instance_type
            };
            let name = instance_type_name(ty.unwrap_or(0));
            let inside = match ty {
                Some(t) if t != 0 => Value::Number(1.0),
                _ => Value::Nil,
            };
            Ok(MultiValue::from_vec(vec![
                inside,
                Value::String(lua.create_string(name)?),
            ]))
        })?,
    )?;

    // `CanShowResetInstances()`: 1 or nil, the gate on the SELF menu's reset row.
    g.set(
        "CanShowResetInstances",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(if model.can_reset_instances {
                Value::Number(1.0)
            } else {
                Value::Nil
            })
        })?,
    )?;

    // `ResetInstances()`: the `CONFIRM_RESET_INSTANCES` dialog's Yes.
    g.set(
        "ResetInstances",
        lua.create_function(|lua, ()| {
            super::calls::submit(lua, super::ScriptCall::Instance(InstanceCall::Reset))
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::InstanceCall;
    use crate::script::{ScriptCall, UiScript, UiTarget};

    /// The unit popup's Heroic is `SetDungeonDifficulty(2)`. It went out as 2, which AzerothCore
    /// refuses for a dungeon (`MiscHandler.cpp:1268`), and the getters never moved. It goes out
    /// 0-based, reads back 1-based at once, and the server's packets land in the same cells.
    #[test]
    fn heroic_goes_out_zero_based_and_the_server_packets_land() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.keep_calls();
        s.run("SetDungeonDifficulty(2)").unwrap();
        assert_eq!(
            s.take_calls_like(&ScriptCall::Instance(InstanceCall::SetDungeonDifficulty(1))),
            1
        );
        assert_eq!(
            s.eval::<(i64, i64)>("return GetDungeonDifficulty()")
                .unwrap(),
            (2, 2)
        );
        // The server's echo puts the player back on Normal; then a heroic instance's packet.
        s.apply_dungeon_difficulty(0, true, false);
        assert_eq!(
            s.eval::<(i64, i64)>("return GetDungeonDifficulty()")
                .unwrap(),
            (1, 1)
        );
        s.set_instance_difficulty(1, 0);
        assert_eq!(s.eval::<i64>("return GetInstanceDifficulty()").unwrap(), 2);
    }
}
