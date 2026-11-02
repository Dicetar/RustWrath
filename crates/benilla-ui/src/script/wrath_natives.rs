//! Wrath client-query natives the stock 3.3.5a FrameXML calls at load and the 1.12 interpreter
//! has no verb for. Each answer is the real client's on a machine without the optional hardware
//! the query asks about, or with the single output device benilla mixes through.

use mlua::{Lua, MultiValue, Value};

use super::{CursorPayload, Model};

/// `GetCursorInfo()` (`0x00515200`): the held payload in the shape the 3.3.5a FrameXML reads it.
///
/// - `"item"`, entry id, hyperlink: a picked-up item, and a bar item, which the reference answers
///   from its virtual id (`CGGameUI::m_cursorVirtualID`).
/// - `"money"`, copper: coins off a money frame.
/// - `"spell"`, 1-based slot, `"spell"`/`"pet"`: a spell, looked up in the player's book then the
///   pet's (`GetSpellSlot(id, 0)`/`(id, 1)`); not in either gives slot 0 in the player book.
/// - `"merchant"`, 1-based row: a vendor row.
/// - `"macro"`, 1-based macro index: `GetMacroIndexByID(id) + 1`.
///
/// An empty cursor returns nothing, which is what `local type = GetCursorInfo()` tests. The
/// reference's companion (`"companion"`, slot, `"CRITTER"`/`"MOUNT"`), guild-bank (`"guildbankmoney"`)
/// and equipment-set arms have no host state here: benilla keeps no companion, guild-bank or
/// equipment-set collection, and none of them can reach the cursor.
fn cursor_info(lua: &Lua) -> mlua::Result<MultiValue> {
    let model = lua.app_data_ref::<Model>().expect("model app_data");
    let mut out = MultiValue::new();
    let mut push = |value: Value| out.push_back(value);
    match &model.cursor {
        None => {}
        Some(CursorPayload::Item(item)) => {
            push(Value::String(lua.create_string("item")?));
            push(Value::Number(f64::from(item.item_id)));
            push(match &item.link {
                Some(link) => Value::String(lua.create_string(link)?),
                None => Value::Nil,
            });
        }
        Some(CursorPayload::Money(money)) => {
            push(Value::String(lua.create_string("money")?));
            push(Value::Number(f64::from(money.copper)));
        }
        Some(CursorPayload::Spell(spell)) => {
            let slot = |slots: &[super::SpellSlotView]| {
                slots
                    .iter()
                    .position(|s| s.spell_id == spell.spell_id)
                    .map(|index| index + 1)
            };
            let (slot, book) = match slot(&model.spellbook.slots) {
                Some(slot) => (slot, "spell"),
                // `GetSpellSlot` falls through to the pet's book and flags the answer `"pet"`
                // (0x00515200); a spell in neither book is slot 0 in the player's.
                None => (slot(&model.pet_book.slots).unwrap_or(0), "pet"),
            };
            push(Value::String(lua.create_string("spell")?));
            push(Value::Number(slot as f64));
            if slot == 0 {
                push(Value::String(lua.create_string("spell")?));
            } else {
                push(Value::String(lua.create_string(book)?));
            }
        }
        Some(CursorPayload::Merchant(row)) => {
            push(Value::String(lua.create_string("merchant")?));
            // The reference stores the row 0-based (`trunc(arg) - 1`) and answers it plus one.
            push(Value::Number(f64::from(row.row + 1)));
        }
        Some(CursorPayload::Action(action)) => {
            // `UICURSOR_ACTIONBAR` with an item action: the item's entry id and hyperlink
            // (`CGTooltip::GetItemHyperlinkString(itemId, quality, …)`). The bar slot carries no
            // link or quality here, so the hyperlink is nil rather than a synthesized one.
            push(Value::String(lua.create_string("item")?));
            push(Value::Number(f64::from(action.action)));
            push(Value::Nil);
        }
        Some(CursorPayload::Macro(mac)) => {
            push(Value::String(lua.create_string("macro")?));
            push(Value::Number(
                model.macros.index_of_id(mac.id).unwrap_or(0) as f64,
            ));
        }
        // No `GetCursorInfo` arm in the reference: a stable-pet grab and a pet-bar pickup fall
        // through its switch to `return 0`.
        Some(CursorPayload::StablePet(_)) | Some(CursorPayload::PetAction(_)) => {}
    }
    Ok(out)
}

/// The 3.3.5a `Bindings.xml` `<ModifiedClick>` rows' defaults: SELFCAST ALT, FOCUSCAST NONE,
/// DRESSUP and SOCKETITEM CTRL, every other action SHIFT. An unknown action has no default.
fn modified_click(action: &str) -> Option<&'static str> {
    match action {
        "SELFCAST" => Some("ALT"),
        "FOCUSCAST" => Some("NONE"),
        "DRESSUP" | "SOCKETITEM" => Some("CTRL"),
        "AUTOLOOTTOGGLE" | "CHATLINK" | "COMPAREITEMS" | "OPENALLBAGS" | "PICKUPACTION"
        | "QUESTWATCHTOGGLE" | "SHOWITEMFLYOUT" | "SPLITSTACK" | "STICKYCAMERA"
        | "TOKENWATCHTOGGLE" => Some("SHIFT"),
        _ => None,
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // `IsStereoVideoAvailable` (`VideoOptionsPanels.lua:571`): the stereo-3D path needs glasses
    // driver hardware the real client probes for at boot; benilla has no stereo renderer, so the
    // answer is the one a stock 3.3.5a install gives without that hardware.
    lua.globals().set(
        "IsStereoVideoAvailable",
        lua.create_function(|_, ()| Ok(false))?,
    )?;

    // `UnitIsPossessed` (`PetFrame.lua:186`): no possessed unit exists in benilla, so the stock
    // pet frame keeps its ordinary layout.
    lua.globals().set(
        "UnitIsPossessed",
        lua.create_function(|_, _unit: Option<String>| Ok(false))?,
    )?;

    // `IsReferAFriendLinked` (`UnitPopup.lua:788`) and `GetReadyCheckStatus`
    // (`PartyMemberFrame.lua:289`): benilla has no recruit-a-friend link and runs no ready
    // check, so the stock frames load with those features absent (compatibility entries).
    lua.globals().set(
        "IsReferAFriendLinked",
        lua.create_function(|_, _unit: Option<String>| Ok(false))?,
    )?;
    lua.globals().set(
        "GetReadyCheckStatus",
        lua.create_function(|_, _unit: Option<String>| Ok(Value::Nil))?,
    )?;

    // `HasCompletedAnyAchievement` (`MainMenuBarMicroButtons.lua:103`): the micro menu's
    // achievement button asks whether the account has any; benilla has no achievements, so the
    // answer is false and the button stays in its stock "none" state.
    lua.globals().set(
        "HasCompletedAnyAchievement",
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
    // modifier the 3.3.5a `Bindings.xml` `<ModifiedClick>` rows hold by default. An unknown action
    // answers nil, which the controls panel's `or "NONE"` covers.
    lua.globals().set(
        "GetModifiedClick",
        lua.create_function(|_, action: String| Ok(modified_click(&action)))?,
    )?;

    // `IsModifiedClick` (the stock bindings' release legs, e.g. `CAMERAORSELECTORMOVE`'s
    // `CameraOrSelectOrMoveStop(IsModifiedClick("STICKYCAMERA"))`): whether the action's bound
    // modifier is held right now, 1 or nil. Without it the release leg dies before it can stop the
    // move the press leg started, and the key stays logically down.
    lua.globals().set(
        "IsModifiedClick",
        lua.create_function(|lua, action: Option<String>| -> mlua::Result<mlua::Value> {
            let model = lua.app_data_ref::<super::Model>().expect("model app_data");
            let (shift, ctrl, alt) = model.modifiers;
            let Some(action) = action else {
                return Ok(super::binding_abi::flag(shift || ctrl || alt));
            };
            let Some(want) = modified_click(&action) else {
                return Ok(mlua::Value::Nil);
            };
            let held = match want {
                "SHIFT" => shift,
                "CTRL" => ctrl,
                "ALT" => alt,
                _ => false, // NONE
            };
            Ok(super::binding_abi::flag(held))
        })?,
    )?;

    // `SpellCanTargetItem(spell)` (`SecureTemplates.lua:537`): the secure action button's gate
    // for a spell aimed at the cursor item. The client has no spell-targeting cursor item, so
    // false is the correct answer and lets the ordinary UseAction path run; without this the
    // Battle Shout and Attack key/click handlers die before they reach the action.
    lua.globals().set(
        "SpellCanTargetItem",
        lua.create_function(|_, _spell: Option<mlua::Value>| Ok(false))?,
    )?;
    // `SpellCanTargetItemID()` is the second half of the same secure-handler tail
    // (`SpellCanTargetItem() or SpellCanTargetItemID()`): with no item-targeting spell it must
    // answer false too, or every action-bar click dies on a nil call after the action ran.
    lua.globals().set(
        "SpellCanTargetItemID",
        lua.create_function(|_, _spell: Option<mlua::Value>| Ok(false))?,
    )?;

    // `GetCursorInfo()` (`0x00515200`): the stock `ContainerFrameItemButton_OnClick`
    // (`ContainerFrame.lua:697`) reads the held payload's type before it calls
    // `PickupContainerItem`, so without this the bag click and drag raise and no item ever moves.
    // 1.12.1 has no such global; only the Wrath VM installs this file.
    lua.globals().set(
        "GetCursorInfo",
        lua.create_function(|lua, ()| cursor_info(lua))?,
    )?;

    // 12340 `GameUI_Scripts.cpp` title natives (`0x005165E0`, `0x0051B3B0`, `0x0051B410`,
    // `0x0051B4E0`, `0x00522E30`). `Script_GetCurrentTitle` (`0x0051B3B0`) pushes `0.0` (not nil)
    // when no title is chosen (`m_playerChosenTitle <= 0`), and `PaperDollFrame.lua:2593` indexes
    // and compares numeric title IDs against `GetNumTitles()`.
    lua.globals().set(
        "GetNumTitles",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "GetCurrentTitle",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "IsTitleKnown",
        lua.create_function(|lua, id: mlua::Value| {
            let _ = super::binding_abi::int_arg(lua, id, "Usage: IsTitleKnown(titleMaskID)")?;
            Ok(0)
        })?,
    )?;
    lua.globals().set(
        "GetTitleName",
        lua.create_function(|lua, id: mlua::Value| {
            let _ = super::binding_abi::int_arg(lua, id, "Usage: GetTitleName(titleMaskID)")?;
            Ok(mlua::MultiValue::new())
        })?,
    )?;
    lua.globals().set(
        "SetCurrentTitle",
        lua.create_function(|lua, id: mlua::Value| {
            let _ = super::binding_abi::int_arg(lua, id, "Usage: SetCurrentTitle(titleMaskID)")?;
            Ok(())
        })?,
    )?;
    // `IsPlayerResolutionAvailable()` (`VideoOptionsPanels.lua:435`): the resolution list is
    // host-driven; no per-player resolution check exists here.
    lua.globals().set(
        "IsPlayerResolutionAvailable",
        lua.create_function(|_, _: Option<mlua::Value>| Ok(false))?,
    )?;
    // `IsActiveBattlefieldArena()` (`BattlefieldFrame.lua:365`): no arena here.
    lua.globals().set(
        "IsActiveBattlefieldArena",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    // `GetNumTrackedAchievements()` (`WatchFrame.lua:1306`): the achievement tracker is empty.
    lua.globals().set(
        "GetNumTrackedAchievements",
        lua.create_function(|_, ()| Ok(0))?,
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
    // The pitch controls are the other vehicle-only named handlers in VehicleMenuBar.xml. With
    // no vehicle the frame is never shown; defining the handlers keeps the XML's OnMouseDown/Up
    // assignments loadable instead of raising "named script handler is not a function".
    for name in [
        "VehicleAimUpStart",
        "VehicleAimUpStop",
        "VehicleAimDownStart",
        "VehicleAimDownStop",
    ] {
        lua.globals().set(
            name,
            lua.create_function(|_, _args: mlua::MultiValue| Ok(()))?,
        )?;
    }

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

    // `IsVoiceChatEnabled` (`UnitPopup.lua:706`, the mute row): voice chat is permanently absent
    // (`VoiceChat.xml` is skipped forever), so the answer is the disabled client's.
    lua.globals().set(
        "IsVoiceChatEnabled",
        lua.create_function(|_, ()| Ok(false))?,
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

    super::input_verbs::install_wrath(lua)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn wrath_review_contracts_keep_nil_attributes_refunds_and_action_targets() {
        use crate::script::{ScriptCall, UiScript, UiTarget, UnitState};
        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        script.keep_calls();
        script.set_unit("mouseover", Some(UnitState {
            exists: true, has_object: true, guid: 0x77, ..Default::default()
        }));
        assert!(script.eval::<bool>(r#"
            local frame = CreateFrame('Button')
            frame:SetAttribute('type', 'spell')
            frame:SetAttribute('shift-type*', 'item')
            return frame:GetAttribute(nil, 'type', nil) == 'spell'
               and frame:GetAttribute('shift-', 'type', nil) == 'item'
               and select('#', GetContainerItemPurchaseInfo(0, 1)) == 0
               and select('#', GetContainerItemPurchaseItem(0, 1, 1)) == 0
        "#).unwrap());
        script.run("UseAction(1, 'mouseover', 'LeftButton')").unwrap();
        assert!(matches!(script.take_script_calls().as_slice(),
            [ScriptCall::UseAction(press)] if press.target_guid == Some(0x77)));
        script.run("UseAction(1, 'focus', 'LeftButton')").unwrap();
        assert!(script.take_script_calls().is_empty());
    }
    use super::*;

    /// The micro menu's achievement button reads this before it can update; without an
    /// achievement system the answer the stock client gives is false.
    #[test]
    fn has_completed_any_achievement_is_false() {
        let script =
            crate::script::UiScript::with_target(crate::script::UiTarget::Wrath335).expect("VM");
        assert_eq!(
            script
                .eval::<bool>("return HasCompletedAnyAchievement() and true or false")
                .unwrap(),
            false
        );
    }

    /// The stock bindings' release legs need `IsModifiedClick`: 1 when the action's modifier is
    /// held, nil otherwise, and nil for an unknown action — the shape `CameraOrSelectOrMoveStop`
    /// reads after `CAMERAORSELECTORMOVE`'s press leg.
    #[test]
    fn is_modified_click_reads_the_held_modifier() {
        let mut script = crate::script::UiScript::with_target(crate::script::UiTarget::Wrath335)
            .expect("VM");
        let read = |script: &crate::script::UiScript, action: &str| {
            script
                .eval::<Option<i64>>(&format!("return IsModifiedClick(\"{action}\")"))
                .ok()
                .flatten()
        };
        assert_eq!(read(&script, "STICKYCAMERA"), None, "nothing held");
        assert_eq!(
            script
                .eval::<Option<i64>>("return IsModifiedClick()")
                .unwrap(),
            None,
            "the stock loot/spell buttons call it with no action"
        );
        assert!(
            !script
                .eval::<bool>("return SpellCanTargetItem(6673) and true or false")
                .unwrap(),
            "no cursor item to target"
        );
        assert!(
            !script
                .eval::<bool>("return SpellCanTargetItemID() and true or false")
                .unwrap(),
            "the secure handler's same tail asks this too"
        );
        script.set_modifiers(true, false, false);
        assert_eq!(read(&script, "STICKYCAMERA"), Some(1), "shift held");
        assert_eq!(
            script
                .eval::<Option<i64>>("return IsModifiedClick()")
                .unwrap(),
            Some(1),
            "no-arg reads any held modifier"
        );
        assert_eq!(read(&script, "SELFCAST"), None, "self-cast wants alt");
        script.set_modifiers(false, false, true);
        assert_eq!(read(&script, "SELFCAST"), Some(1), "alt held");
        assert_eq!(read(&script, "STICKYCAMERA"), None, "no shift with alt");
        assert_eq!(read(&script, "NO_SUCH_ACTION"), None, "unknown actions");
        assert_eq!(
            script
                .eval::<Option<String>>("return GetModifiedClick(\"STICKYCAMERA\")")
                .ok()
                .flatten()
                .as_deref(),
            Some("SHIFT"),
            "GetModifiedClick answers the same table"
        );
    }

    /// 3.3.5a `Bindings.xml` maps `JUMP` to `JumpOrAscendStart`/`AscendStop`, `SITORSTAND` to
    /// `SitStandOrDescendStart`/`DescendStop`, and `MOVEANDSTEER` to
    /// `MoveAndSteerStart`/`MoveAndSteerStop`.
    #[test]
    fn wrath_movement_and_posture_bindings_queue_behind_the_movement_gate() {
        use crate::script::calls::ScriptCall;
        use crate::script::input_verbs::{BindingInput, FiredInput, HeldInput, PostureCall};

        let mut script = crate::script::UiScript::with_target(crate::script::UiTarget::Wrath335)
            .expect("VM");
        script.keep_calls();
        script
            .run("JumpOrAscendStart() AscendStop() DescendStop() MoveAndSteerStart() MoveAndSteerStop()")
            .unwrap();
        assert!(
            script.take_binding_input().is_empty(),
            "movement verbs stay gated outside a binding body"
        );

        script
            .run_binding(
                "JumpOrAscendStart() AscendStop() SitStandOrDescendStart() DescendStop() \
                 MoveAndSteerStart() MoveAndSteerStop()",
            )
            .unwrap();
        assert_eq!(
            script.take_binding_input(),
            vec![
                BindingInput::Start(HeldInput::PitchUp),
                BindingInput::Fire(FiredInput::Jump),
                BindingInput::Stop(HeldInput::PitchUp),
                BindingInput::Start(HeldInput::PitchDown),
                BindingInput::Stop(HeldInput::PitchDown),
                BindingInput::Start(HeldInput::CameraOrSelectOrMove),
                BindingInput::Start(HeldInput::TurnOrAction),
                BindingInput::Stop(HeldInput::CameraOrSelectOrMove),
                BindingInput::Stop(HeldInput::TurnOrAction),
            ]
        );
        assert_eq!(
            script.take_calls_like(&ScriptCall::Posture(PostureCall::SitOrStand)),
            1
        );
    }

    /// `GetCursorInfo()` (`0x00515200`) answers each arm the stock 3.3.5a FrameXML branches on:
    /// an empty cursor returns nothing (`local type = GetCursorInfo()` is nil), an item its entry
    /// id and hyperlink, coins their copper, a spell its 1-based slot and the book it came from,
    /// a vendor row and a macro their 1-based indices, and an item lifted off the bar its entry id.
    #[test]
    fn get_cursor_info_answers_each_wrath_arm() {
        use crate::script::{
            CursorAction, CursorItem, CursorMacro, CursorMerchantItem, CursorMoney, CursorPayload,
            CursorSpell, MacroState, MacroView, PetBookState, SpellBookState, SpellSlotView,
            UiScript, UiTarget,
        };

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        // The `local type, money = GetCursorInfo()` read needs a truthful count: nothing held.
        assert_eq!(s.eval::<i64>("return select('#', GetCursorInfo())").unwrap(), 0);

        s.set_cursor_for_test(CursorPayload::Item(CursorItem {
            item_class: None,
            guid: 0x105,
            bar_placeable: true,
            consumable: false,
            bag: 0,
            slot: 1,
            item_id: 117,
            texture: None,
            link: Some("|cffffffff|Hitem:117|h[Tough Jerky]|h|r".into()),
            count: None,
            quality: Some(1),
            equip_slots: Vec::new(),
        }));
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()")
                .unwrap(),
            (
                "item".to_string(),
                117.0,
                "|cffffffff|Hitem:117|h[Tough Jerky]|h|r".to_string()
            )
        );

        s.set_cursor_for_test(CursorPayload::Money(CursorMoney { copper: 1_234 }));
        assert_eq!(
            s.eval::<(String, f64)>("return GetCursorInfo()").unwrap(),
            ("money".to_string(), 1_234.0)
        );

        s.set_spellbook(SpellBookState {
            tabs: Vec::new(),
            slots: vec![
                SpellSlotView {
                    spell_id: 133,
                    name: "Fireball".into(),
                    ..Default::default()
                },
                SpellSlotView {
                    spell_id: 145,
                    name: "Frostbolt".into(),
                    ..Default::default()
                },
            ],
        });
        s.set_cursor_for_test(CursorPayload::Spell(CursorSpell {
            passive: false,
            spell_id: 145,
            texture: None,
        }));
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()")
                .unwrap(),
            ("spell".to_string(), 2.0, "spell".to_string()),
            "the player's book answers its 1-based slot"
        );

        s.set_pet_book(PetBookState {
            token: Some("PET".into()),
            slots: vec![SpellSlotView {
                spell_id: 172,
                name: "Corruption".into(),
                ..Default::default()
            }],
        });
        s.set_cursor_for_test(CursorPayload::Spell(CursorSpell {
            passive: false,
            spell_id: 172,
            texture: None,
        }));
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()")
                .unwrap(),
            ("spell".to_string(), 1.0, "pet".to_string()),
            "the pet's book flags the answer pet"
        );

        s.set_cursor_for_test(CursorPayload::Spell(CursorSpell {
            passive: false,
            spell_id: 999,
            texture: None,
        }));
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()")
                .unwrap(),
            ("spell".to_string(), 0.0, "spell".to_string()),
            "a spell in neither book is slot 0, flagged spell"
        );

        s.set_cursor_for_test(CursorPayload::Merchant(CursorMerchantItem {
            item_id: 159,
            row: 0,
            texture: None,
        }));
        assert_eq!(
            s.eval::<(String, f64)>("return GetCursorInfo()").unwrap(),
            ("merchant".to_string(), 1.0),
            "the vendor row is 0-based and answered 1-based"
        );

        s.set_macros(MacroState::from_scopes(
            vec![MacroView {
                id: 1,
                name: "Buff".into(),
                ..Default::default()
            }],
            Vec::new(),
        ));
        s.set_cursor_for_test(CursorPayload::Macro(CursorMacro {
            id: 1,
            texture: None,
        }));
        assert_eq!(
            s.eval::<(String, f64)>("return GetCursorInfo()").unwrap(),
            ("macro".to_string(), 1.0)
        );

        s.set_cursor_for_test(CursorPayload::Action(CursorAction {
            src_slot: 1,
            kind: crate::script::action::ACTION_KIND_ITEM,
            action: 159,
            texture: None,
            count: 1,
            consumable: true,
            state: None,
        }));
        assert_eq!(
            s.eval::<(String, f64)>("return GetCursorInfo()").unwrap(),
            ("item".to_string(), 159.0),
            "an item lifted off the bar names its entry id"
        );
    }

    /// The stock 3.3.5a bag click (`ContainerFrame.lua:697`) reads `GetCursorInfo()` before it
    /// reaches `PickupContainerItem`; a world click with the item held raises
    /// `DELETE_ITEM_CONFIRM`. The branch shape here is the contract, not the stock file: an empty
    /// cursor and a held item both fall to the pickup leg, coins take the money leg instead.
    #[test]
    fn a_bag_click_reads_get_cursor_info_and_a_world_drop_confirms_the_delete() {
        use crate::script::cursor::{two_item_bag, world_drop_click};
        use crate::script::{CursorMoney, CursorPayload, UiScript, UiTarget, WorldPick};

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.keep_calls();
        s.set_container(0, Some(two_item_bag()));
        // The left-click decision table the stock handler uses: `type == "money"` diverts to
        // `DropCursorMoney`, everything else picks the bag slot up.
        const CLICK: &str = r#"
            local kind = GetCursorInfo()
            if kind == "money" then
                DropCursorMoney()
            else
                PickupContainerItem(0, 1)
            end
            return kind or "none"
        "#;
        assert_eq!(s.eval::<String>(CLICK).unwrap(), "none");
        let held = s.cursor_item().expect("the bag click picked the item up");
        assert_eq!((held.bag, held.slot, held.item_id), (0, 1, 117));

        // Coins on the cursor: the same click must not pick the slot up.
        s.run("ClearCursor()").unwrap();
        s.set_cursor_for_test(CursorPayload::Money(CursorMoney { copper: 10 }));
        assert_eq!(s.eval::<String>(CLICK).unwrap(), "money");
        assert!(
            s.cursor_item().is_none(),
            "the money leg leaves the bag slot alone"
        );

        // The held item's world click: terrain raises the delete confirmation and keeps it held.
        s.set_cursor_for_test(CursorPayload::Item(crate::script::cursor::CursorItem {
            item_class: None,
            guid: 0x105,
            bar_placeable: true,
            consumable: false,
            bag: 0,
            slot: 1,
            item_id: 117,
            texture: None,
            link: Some("|cffffffff|Hitem:117|h[Tough Jerky]|h|r".into()),
            count: None,
            quality: Some(1),
            equip_slots: Vec::new(),
        }));
        s.set_world_pick(WorldPick::Terrain);
        assert!(
            world_drop_click(&mut s.model_mut()),
            "the world click belongs to the payload"
        );
        assert!(s.cursor_item().is_some(), "the popup answer keeps it held");
        let raised: Vec<String> = s
            .model_ref()
            .pending_events
            .iter()
            .map(|(event, _)| event.clone())
            .collect();
        assert_eq!(raised, ["DELETE_ITEM_CONFIRM"]);
        assert_eq!(
            s.eval::<(String, f64)>("return GetCursorInfo()").unwrap(),
            ("item".to_string(), 117.0),
            "the popup still sees the item on the cursor"
        );
    }
}
