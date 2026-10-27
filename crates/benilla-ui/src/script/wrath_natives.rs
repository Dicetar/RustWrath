//! Wrath client-query natives the stock 3.3.5a FrameXML calls at load and the 1.12 interpreter
//! has no verb for. Each answer is the real client's on a machine without the optional hardware
//! the query asks about, or with the single output device benilla mixes through.

use mlua::{Lua, ObjectLike};

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // `IsStereoVideoAvailable` (`VideoOptionsPanels.lua:571`): the stereo-3D path needs glasses
    // driver hardware the real client probes for at boot; benilla has no stereo renderer, so the
    // answer is the one a stock 3.3.5a install gives without that hardware.
    lua.globals().set(
        "IsStereoVideoAvailable",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `Sound_GameSystem_*` (`AudioOptionsPanels.lua:135`, `:167`): the client enumerates the Miles
    // output devices, and a stock install registers its one DirectSound output as the primary
    // driver (`Sound_OutputDriverName` defaults to "Primary Sound Driver"). benilla's single kira
    // output stands in for that device.
    lua.globals().set(
        "Sound_GameSystem_GetNumOutputDrivers",
        lua.create_function(|_, ()| Ok(1))?,
    )?;
    lua.globals().set(
        "Sound_GameSystem_GetOutputDriverNameByIndex",
        lua.create_function(|_, index: i64| {
            Ok((index == 0).then_some("Primary Sound Driver"))
        })?,
    )?;

    // `Sound_ChatSystem_*` (`AudioOptionsPanels.lua:600`, `:791`): the voice-chat panels enumerate
    // the Steam voice system's devices. The stock client with voice chat disabled (the default,
    // `VoiceChatMode` 0) has none; its output panel expects that empty list.
    lua.globals().set(
        "Sound_ChatSystem_GetNumInputDrivers",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "Sound_ChatSystem_GetNumOutputDrivers",
        lua.create_function(|_, ()| Ok(0))?,
    )?;

    // `GetModifiedClick` (`InterfaceOptionsPanels.xml:225`, `InterfaceOptionsPanels.lua:134`): the
    // modifier the 3.3.5a `Bindings.xml` `<ModifiedClick>` rows hold by default — SELFCAST ALT,
    // FOCUSCAST NONE, AUTOLOOTTOGGLE SHIFT, every other action SHIFT except DRESSUP and SOCKETITEM
    // CTRL. An unknown action answers nil, which the controls panel's `or "NONE"` covers.
    lua.globals().set(
        "GetModifiedClick",
        lua.create_function(|_, action: String| {
            Ok(match action.as_str() {
                "SELFCAST" => Some("ALT"),
                "FOCUSCAST" => Some("NONE"),
                "DRESSUP" | "SOCKETITEM" => Some("CTRL"),
                "AUTOLOOTTOGGLE" | "CHATLINK" | "COMPAREITEMS" | "OPENALLBAGS" | "PICKUPACTION"
                | "QUESTWATCHTOGGLE" | "SHOWITEMFLYOUT" | "SPLITSTACK" | "STICKYCAMERA"
                | "TOKENWATCHTOGGLE" => Some("SHIFT"),
                _ => None,
            })
        })?,
    )?;

    // `BNFeaturesEnabled` (`InterfaceOptionsPanels.lua:820`, `:857`, `:1610`): Battle.net features
    // come with a Battle.net account connection benilla does not have, and the stock panels take
    // the same branch here as the real client on an account without them.
    lua.globals().set(
        "BNFeaturesEnabled",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    // `BNFeaturesEnabledAndConnected` (`InterfaceOptionsPanels.lua:868`): the same answer, with no
    // Battle.net connection to be connected, so the profanity-filter sync leaves the local value.
    lua.globals().set(
        "BNFeaturesEnabledAndConnected",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `GetExistingLocales` (`InterfaceOptionsPanels.lua:1754`): the locale data streams the install
    // carries. benilla reads one install's single locale, so the list is this client's own, as the
    // real client's is on a mono-locale install — the languages panel then never registers.
    lua.globals().set(
        "GetExistingLocales",
        lua.create_function(|lua, ()| Ok(super::client::locale_name(lua)))?,
    )?;

    // `VehicleExit` (`MainMenuBar.xml`, the vehicle leave button's `function=`): 3.3.5a registers
    // it as a frame-script global at boot. It acts on the vehicle the player is in, and that button
    // exists only inside one (`MainMenuBar.lua` shows it on `UNIT_ENTERED_VEHICLE`); benilla has no
    // vehicles, so the stock paths never reach an action here.
    lua.globals().set(
        "VehicleExit",
        lua.create_function(|_, _args: mlua::MultiValue| Ok(()))?,
    )?;

    // `GetNumTrackingTypes`/`GetTrackingInfo` (`Minimap.lua:427`, `:430`): the tracking types come
    // from the player's known tracking spells (aura TRACK_CREATURES/TRACK_RESOURCES); the host has
    // not pushed those masks with the spellbook yet, so the catalogue the dropdown enumerates is
    // the empty one a character without tracking spells sees.
    lua.globals().set(
        "GetNumTrackingTypes",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "GetTrackingInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::MultiValue::new()))?,
    )?;

    // `GetWorldPVPQueueStatus(index)` (`BattlefieldFrame.lua:696`): the world-PvP queue's state per
    // slot, `"none"`/`"queued"`/`"confirm"`. benilla has no world-PvP queue, so every slot answers
    // the status the stock client shows for an empty queue.
    lua.globals().set(
        "GetWorldPVPQueueStatus",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let status = lua.create_string("none")?;
            Ok(mlua::MultiValue::from_vec(vec![
                mlua::Value::String(status),
                mlua::Value::Nil,
                mlua::Value::Nil,
            ]))
        })?,
    )?;

    // `GetLFGProposal` (`UIParent.lua:3571`, the minimap LFG dropdown's load): ten values, the
    // first "a proposal exists". benilla has no dungeon-finder queue, so there is no proposal.
    lua.globals().set(
        "GetLFGProposal",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    // `GetLFGInfoServer` and `GetLFGRoleUpdate` (`UIParent.lua:3572`, `:3573`): the server-side
    // LFG state — party membership, the queue, the role check. With no dungeon-finder queue the
    // answers are the idle ones: no party, no queue, no role check, no slots.
    lua.globals().set(
        "GetLFGInfoServer",
        lua.create_function(|_, ()| {
            Ok((
                false,
                false,
                false,
                false,
                false,
                mlua::Value::Nil,
                0i64,
            ))
        })?,
    )?;
    lua.globals().set(
        "GetLFGRoleUpdate",
        lua.create_function(|_, ()| Ok((false, mlua::Value::Nil, mlua::Value::Nil)))?,
    )?;

    // `IsListedInLFR` and `IsPartyLFG` (`UIParent.lua:3583`, `:3585`, `GetLFGMode`'s listing and
    // party tests): no raid-finder listing and no dungeon-finder party, as benilla has neither.
    lua.globals().set(
        "IsListedInLFR",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "IsPartyLFG",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // Voice chat is permanently absent: the interface's `VoiceChat.xml` is skipped forever
    // (`reference_ui::WRATH_SKIPPED_FILES` in the app). The stock files still call the voice verbs
    // that file defined — PlayerFrame's speaker OnUpdate, the chat menu's toggle, the audio test's
    // loopback — and on a voice-disabled client their real behaviour is nothing.
    for name in [
        "VoiceChat_OnUpdate",
        "VoiceChat_Update",
        "VoiceChat_Toggle",
        "VoiceChat_PlayLoopbackSound",
        "VoiceChat_RecordLoopbackSound",
        "VoiceChat_StopRecordingLoopbackSound",
    ] {
        lua.globals().set(
            name,
            lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
        )?;
    }

    // `UnitInBattleground` (`UnitPopup.lua:479`, the unit-popup branch): whether the unit is in a
    // battleground. The Wrath unit view carries no battleground membership yet, and a player
    // outside one is the state this host has, so the answer is the empty one.
    lua.globals().set(
        "UnitInBattleground",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;

    // `GetDungeonDifficulty`/`GetRaidDifficulty` (`UnitPopup.lua:700`, `:703`): the session's
    // dungeon and raid difficulty ids. The host carries no difficulty selection yet, so both
    // answer the default normal tier the 3.3.5a client boots at, `1`.
    lua.globals().set(
        "GetDungeonDifficulty",
        lua.create_function(|_, ()| Ok(1i64))?,
    )?;
    lua.globals().set(
        "GetRaidDifficulty",
        lua.create_function(|_, ()| Ok(1i64))?,
    )?;

    // `IsVoiceChatEnabled` (`UnitPopup.lua:706`, the mute row): voice chat is permanently absent
    // (`VoiceChat.xml` is skipped forever), so the answer is the disabled client's.
    lua.globals().set(
        "IsVoiceChatEnabled",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `GetOptOutOfLoot` (`UnitPopup.lua:238`, the loot-roll opt-out row): whether the player
    // turned off automatic greed rolls. The host carries no opt-out setting, so the row shows
    // unchecked — a fresh client's state.
    lua.globals().set(
        "GetOptOutOfLoot",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `UnitThreatSituation` (`UnitFrame.lua:434`, the target frame's threat colour): 1..3 by
    // threat state, nil with none. No threat state reaches the unit view yet, so the answer is
    // the nil a client outside any threat context reads.
    lua.globals().set(
        "UnitThreatSituation",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::Value::Nil))?,
    )?;

    // `GetInstanceInfo` (`UnitPopup.lua:245`, `Minimap.lua:486`): name, type, difficulty id,
    // difficulty name, max players, player difficulty, dynamic flag, instance id. The
    // bootstrap host is in the open world, which the stock client answers with type `"none"`.
    lua.globals().set(
        "GetInstanceInfo",
        lua.create_function(|lua, ()| {
            Ok(mlua::MultiValue::from_vec(vec![
                mlua::Value::Nil,
                mlua::Value::String(lua.create_string("none")?),
                mlua::Value::Nil,
                mlua::Value::Nil,
                mlua::Value::Integer(0),
                mlua::Value::Nil,
                mlua::Value::Boolean(false),
                mlua::Value::Integer(0),
            ]))
        })?,
    )?;

    // `GetSummonFriendCooldown` (`UnitPopup.lua:263`): the recruit-a-friend summon's `(start,
    // duration)` on the `GetTime` clock; the arithmetic there needs both. No summon has been
    // cast, so the pair is the zero the stock client reads when the ability is ready.
    lua.globals().set(
        "GetSummonFriendCooldown",
        lua.create_function(|_, ()| Ok((0i64, 0i64)))?,
    )?;

    // `IsThreatWarningEnabled` (`UnitFrame.lua:437`): the `threatWarning` CVar's setting — the
    // stock client boots with `3`, always show — read as the enabled flag, the always-tier
    // answer. The per-context tiers arrive with the threat view.
    lua.globals().set(
        "IsThreatWarningEnabled",
        lua.create_function(|_, ()| Ok(true))?,
    )?;

    // `UnitHasVehicleUI` (`PlayerFrame.lua:433`, the vehicle swap): whether the unit's vehicle UI
    // is up. benilla has no vehicles, so the answer is the un-vehicled one.
    lua.globals().set(
        "UnitHasVehicleUI",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;

    // `CanHearthAndResurrectFromArea` (`BattlefieldFrame.lua:758`): world-PvP zones hold a player
    // from hearthing or resurrecting; benilla has none, so the answer is the unheld one.
    lua.globals().set(
        "CanHearthAndResurrectFromArea",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `CalendarGetDate` (`GameTime.lua:247`, the clock frame's load): weekday, month, day, year in
    // the calendar's time zone. The calendar's date is server data the host pushes after login, so
    // before that every field is zero, as the stock clock frame reads it at load.
    lua.globals().set(
        "CalendarGetDate",
        lua.create_function(|_, ()| Ok((0i64, 0i64, 0i64, 0i64)))?,
    )?;

    // `BNGetMaxPlayersInConversation` (`BNConversations.lua:3`, the chunk's top level): the
    // service's cap on members in one Battle.net conversation. benilla has no conversation
    // service (`BNFeaturesEnabled` false), and the only consumer is the conversation-invite
    // dialog, so the cap the live service reported is not reproduced here.
    lua.globals().set(
        "BNGetMaxPlayersInConversation",
        lua.create_function(|_, ()| Ok(10i64))?,
    )?;

    // `BNGetNumFriends` (`FloatingChatFrame.xml`'s friends micro button): total and online RealID
    // friends. No Battle.net service has none of either.
    lua.globals().set(
        "BNGetNumFriends",
        lua.create_function(|_, ()| Ok((0i64, 0i64)))?,
    )?;

    // `GetChatWindowSavedDimensions`/`GetChatWindowSavedPosition` (`FloatingChatFrame.lua:1128`,
    // `:1133`, the window restore): the geometry the chat cache holds for a window. benilla's
    // chat cache carries the 1.12 record, which has no saved geometry, so the stock defaults
    // stand — the same nil a fresh install's empty record answers.
    lua.globals().set(
        "GetChatWindowSavedDimensions",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::MultiValue::new()))?,
    )?;
    lua.globals().set(
        "GetChatWindowSavedPosition",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::MultiValue::new()))?,
    )?;

    // `CooldownFrame_SetTimer(cooldown, start, duration, enable)` (`ActionButton.lua:348`): the
    // 1.12-compatible verb the wrath FrameXML still calls, with the 1.12 `Cooldown.lua` behaviour —
    // the start and duration land on the object and an active cooldown shows it.
    lua.globals().set(
        "CooldownFrame_SetTimer",
        lua.create_function(|_, (this, start, duration, enable): (mlua::Table, f64, f64, f64)| {
            this.set("start", start)?;
            this.set("duration", duration)?;
            if start > 0.0 && duration > 0.0 && enable > 0.0 {
                this.call_method::<()>("Show", ())?;
            } else {
                this.call_method::<()>("Hide", ())?;
            }
            Ok(())
        })?,
    )?;
    Ok(())
}
