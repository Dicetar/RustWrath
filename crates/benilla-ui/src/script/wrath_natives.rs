//! Wrath client-query natives the stock 3.3.5a FrameXML calls at load and the 1.12 interpreter
//! has no verb for. Device queries describe supported local hardware. Some imported startup
//! bindings below remain compatibility fallbacks rather than projections of live gameplay
//! state; registering them does not establish support for those subsystems.

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
            // Keep the source's full instance link only when it names this exact entry; a
            // substring test would accept `|Hitem:1170...` for item 117. A link that names
            // another item (or none) is rebuilt from the authoritative template cache.
            let link = item
                .link
                .clone()
                .filter(|link| {
                    super::item_stats::wrath::ItemLink::parse(link)
                        .is_some_and(|parsed| parsed.id == item.item_id)
                })
                .or_else(|| {
                    model.item_templates.get(&item.item_id).map(|template| {
                        super::item_stats::item_link_for_target(
                            model.target,
                            item.item_id,
                            &template.name,
                            template.quality,
                        )
                    })
                });
            let Some(link) = link else { return Ok(out) };
            push(Value::String(lua.create_string("item")?));
            push(Value::Number(f64::from(item.item_id)));
            push(Value::String(lua.create_string(&link)?));
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
            // 12340 0x00515200: an uncached virtual item returns no values.
            let Some(template) = model.item_templates.get(&action.action) else { return Ok(out) };
            let link = super::item_stats::item_link_for_target(model.target, action.action, &template.name, template.quality);
            push(Value::String(lua.create_string("item")?));
            push(Value::Number(f64::from(action.action)));
            push(Value::String(lua.create_string(&link)?));
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

/// `GetLFGRandomCooldownExpiration` (`0x558060`) and `GetLFGDeserterExpiration` (`0x5580E0`): the
/// active player's native aura end time for `spell_id` in seconds on the `GetTime()` clock, or no
/// values with no player, no observed aura list, or no such aura. Both reference bindings walk
/// `CGUnit_C::GetAuraIndex` (`0x7289C0`, the raw slot array), which the scoped `UnitAura` query
/// exposes; the aura-bar display filter does not apply, so the hidden random-dungeon cooldown 71328
/// is found. A match with no timer answers the reference's zero end time, not no values.
fn lfg_aura_expiration(lua: &Lua, spell_id: u32) -> mlua::Result<MultiValue> {
    let player = lua
        .app_data_ref::<Model>()
        .expect("model app_data")
        .unit_guids
        .player;
    if player == 0 {
        return Ok(MultiValue::new());
    }
    let auras = super::host::with_host(lua, |host| host.unit_auras(player));
    let Some(auras) = auras.flatten() else {
        return Ok(MultiValue::new());
    };
    // `unit_auras` is in ascending slot order, so the first match is the reference's
    // `GetAuraIndex` slot.
    Ok(auras
        .iter()
        .find(|aura| aura.spell_id == spell_id)
        .map_or_else(MultiValue::new, |aura| {
            MultiValue::from_vec(vec![Value::Number(aura.expiration.unwrap_or(0.0))])
        }))
}

/// `UnitHasLFGRandomCooldown("unit")` (`0x00555760`) and `UnitHasLFGDeserter("unit")`
/// (`0x00555840`): whether the named unit carries the random-dungeon cooldown (71328) or
/// deserter (71041) aura. The reference consults the unit object's `HasAuraBySpellId`, falling
/// back to the `CGPartyMemberState` repository's aura list; benilla's native aura list (the same
/// `unit_auras` owner `UnitAura` reads) covers the player and party members the host projects.
/// A `None` list, a guid of zero, or no host answers no values, matching the reference's
/// `guid == 0` / `state == nullptr` early returns; the party-state fallback has no separate
/// owner here, so it is not synthesized.
fn lfg_party_aura(lua: &Lua, token: Value, spell_id: u32, usage: &'static str) -> mlua::Result<MultiValue> {
    let token = crate::script::binding_abi::string_arg(lua, token, usage)?;
    let guid = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        model.unit_guids.guid_of(&token)?.unwrap_or(0)
    };
    if guid == 0 {
        return Ok(MultiValue::new());
    }
    let auras = super::host::with_host(lua, |host| host.unit_auras(guid));
    let Some(auras) = auras.flatten() else {
        return Ok(MultiValue::new());
    };
    Ok(MultiValue::from_vec(vec![Value::Boolean(
        auras.iter().any(|aura| aura.spell_id == spell_id),
    )]))
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

    // `GetNumTitles()` / `GetCurrentTitle()` / `IsTitleKnown()` / `GetTitleName()`
    // (`GameUI_Scripts.cpp:2563`, `:2970`, `:2986`). `GetNumTitles` is the CharTitles.dbc
    // record-id ceiling, not the number this character knows; the app seeds the ceiling from the
    // loaded table. The known mask and chosen id come from the 3.3.5 player descriptor, and
    // `GetTitleName` needs the localized CharTitles name table, which is not projected yet: a
    // known title is reported known but its name is nil rather than invented.
    lua.globals().set(
        "GetNumTitles",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.title_count)
        })?,
    )?;
    lua.globals().set(
        "GetCurrentTitle",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.unit("player").map_or(0, |u| u.chosen_title))
        })?,
    )?;
    lua.globals().set(
        "IsTitleKnown",
        lua.create_function(|lua, mask: Value| {
            let mask = crate::script::binding_abi::number_arg(
                lua,
                mask,
                "Usage: IsTitleKnown(titleMaskID)",
            )? as u32;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let known = model
                .unit("player")
                .is_some_and(|u| {
                    let bit = usize::try_from(mask).ok();
                    bit.is_some_and(|bit| {
                        u.known_titles
                            .get(bit / 32)
                            .is_some_and(|word| word & (1 << (bit % 32)) != 0)
                    })
                });
            // The reference pushes the number 0 or 1, not a boolean or nil.
            Ok(Value::Integer(i64::from(known)))
        })?,
    )?;
    lua.globals().set(
        "GetTitleName",
        lua.create_function(|lua, mask: Value| {
            crate::script::binding_abi::number_arg(
                lua,
                mask,
                "Usage: GetTitleName(titleMaskID)",
            )?;
            // The CharTitles name table is app-side (`QuestTitleNames`); only the quest-text
            // path consumes it today. Nil is the reference's own no-record answer, never a
            // fabricated title.
            Ok(mlua::Value::Nil)
        })?,
    )?;
    // `GetNumLanguages()` (`ChatFrame_Scripts.cpp:644`) vs the 1.12 spelling
    // `GetNumLaguages` (`0x49fb30`): both read the same known-language list.
    lua.globals().set(
        "GetNumLanguages",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.known_languages.len() as i64)
        })?,
    )?;
    // `GetCombatRating(index)` (`ScriptEvents_Scripts.cpp:1992`): 1-based, 1..25, and the real
    // 3.3.5 `PLAYER_FIELD_COMBAT_RATING_1` word for the active player. An unprojected window is
    // zero, the same value the descriptor carries before the field arrives.
    lua.globals().set(
        "GetCombatRating",
        lua.create_function(|lua, index: Value| {
            let index = crate::script::binding_abi::number_arg(
                lua,
                index,
                "Usage: GetCombatRating(ratingIndex)",
            )?
            .wrapping_sub(1) as u32;
            if index >= 25 {
                return Err(mlua::Error::runtime("ratingIndex is in the range 1 .. 25"));
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model
                .unit("player")
                .and_then(|u| u.combat_ratings.get(index as usize).copied())
                .unwrap_or(0))
        })?,
    )?;
    // `CanExitVehicle()` (`InputControl_Scripts.cpp:500`): one number when the active vehicle
    // seat carries flag 0x2000000, otherwise nil. benilla has no vehicle seat model, and no
    // donor-era content creates one, so nil is the real client's answer without that seat.
    lua.globals().set(
        "CanExitVehicle",
        lua.create_function(|_, ()| Ok(mlua::Value::Nil))?,
    )?;
    // `IsPetAttackAction(index)` (`PetInfo_Scripts.cpp:274`): the pet action at `index` is the
    // command word type 7 action 2 (Attack). The bar's packed words are the server's.
    lua.globals().set(
        "IsPetAttackAction",
        lua.create_function(|lua, index: Value| {
            let index = crate::script::binding_abi::number_arg(
                lua,
                index,
                "Usage: IsPetAttackAction(index)",
            )?
            .wrapping_sub(1) as u32;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let attack = model.pet_bar.has_ui
                && model
                    .pet_bar
                    .slots
                    .get(index as usize)
                    .is_some_and(|slot| {
                        slot.view.packed & 0x3F00_0000 == 0x0700_0000
                            && slot.view.packed & 0x00FF_FFFF == 2
                    });
            // `lua_pushboolean`: a boolean, not the 1/nil shape.
            Ok(Value::Boolean(attack))
        })?,
    )?;
    // `UnitIsAFK` / `UnitIsDND` (`ScriptEvents_Scripts.cpp:4490`, `:2104`): `PLAYER_FLAGS` bits
    // 0x2 and 0x4, 1/nil as the reference pushes them. A token the client does not hold answers
    // nil (no party-state repository is projected here).
    for (verb, bit) in [("UnitIsAFK", 0x2u32), ("UnitIsDND", 0x4u32)] {
        lua.globals().set(
            verb,
            lua.create_function(move |lua, token: Value| {
                let token = crate::script::binding_abi::string_arg(
                    lua,
                    token,
                    if bit == 0x2 {
                        "Usage: UnitIsAFK(\"unit\")"
                    } else {
                        "Usage: UnitIsDND(\"unit\")"
                    },
                )?;
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                Ok(match model.unit(&token) {
                    Some(unit) if unit.player_flags & bit != 0 => {
                        mlua::Value::Number(1.0)
                    }
                    Some(_) => mlua::Value::Nil,
                    None => mlua::Value::Nil,
                })
            })?,
        )?;
    }
    // `GetLFGQueuedList([table])` (`LFGInfo_Scripts.cpp:544`): the caller's table, cleared and
    // filled with the queued dungeon slots. benilla has no dungeon-finder queue, so the
    // reference's empty queue is an empty table (or the caller's table, cleared).
    lua.globals().set(
        "GetLFGQueuedList",
        lua.create_function(|lua, arg: Option<Value>| match arg {
            Some(Value::Table(table)) => {
                table.clear()?;
                Ok(Value::Table(table))
            }
            Some(Value::Nil) | None => Ok(Value::Table(lua.create_table()?)),
            Some(_) => Err(mlua::Error::runtime("Usage: GetLFDQueuedList([table])")),
        })?,
    )?;
    // The queue/arena/battleground list queries are empty until their subsystems populate them;
    // these are the reference's own empty shapes, not fabricated success.
    lua.globals().set(
        "GetNumArenaOpponents",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "GetNumBattlegroundTypes",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "GetArenaTeam",
        lua.create_function(|lua, team: Value| {
            crate::script::binding_abi::number_arg(lua, team, "Usage: GetArenaTeam(team)")?;
            // No team: nil, then the reference's twenty-one zero values.
            let mut out = mlua::MultiValue::new();
            out.push_back(Value::Nil);
            for _ in 0..21 {
                out.push_back(Value::Number(0.0));
            }
            Ok(out)
        })?,
    )?;

    // `GetNumCompanions(type)` / `GetCompanionInfo(type, index)` (`SpellBookFrame_Scripts.cpp:369`,
    // `:206`): the 3.3.5 companion collections are built from summon-critter spells. benilla
    // does not project that collection; the character-controlled companion list is empty, which
    // is the reference's answer for a character with none. The usage gates still raise.
    lua.globals().set(
        "GetNumCompanions",
        lua.create_function(|lua, kind: Value| {
            companion_type(lua, kind, "Usage: GetNumCompanions(type)")?;
            Ok(0)
        })?,
    )?;
    lua.globals().set(
        "GetCompanionInfo",
        lua.create_function(|lua, (kind, index): (Value, Option<Value>)| {
            companion_type(lua, kind, "Usage: GetCompanionInfo(type, index)")?;
            let index = index
                .and_then(|index| lua.coerce_number(index).ok().flatten())
                .ok_or_else(|| mlua::Error::runtime("Usage: GetCompanionInfo(type, index)"))?;
            if index == 0.0 {
                return Err(mlua::Error::runtime("Usage: GetCompanionInfo(type, index)"));
            }
            // No companions projected: every valid 1-based index is past the collection.
            Ok(mlua::MultiValue::new())
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
    // `GetTrackedAchievements()` (`AchievementInfo_Scripts.cpp:147`) pushes one id per tracked
    // achievement and returns that count. `WatchFrame_DisplayTrackedAchievements` passes the
    // result straight to `select("#", ...)`; with no tracker state here the reference's empty
    // list is zero values, never a nil argument. `GetNumTrackedAchievements` is the same empty
    // count.
    lua.globals().set(
        "GetTrackedAchievements",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::MultiValue::new()))?,
    )?;
    lua.globals().set(
        "GetNumTrackedAchievements",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0))?,
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

    // `SetChatWindowUninteractable` (`FloatingChatFrame.lua:982`): whether a chat window receives clicks.
    lua.globals().set(
        "SetChatWindowUninteractable",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;

    // `GetExpansionLevel` (`LFDFrame.lua:1`) & `GetAccountExpansionLevel` (`ReputationFrame.lua:25`):
    // 0 = Classic, 1 = The Burning Crusade, 2 = Wrath of the Lich King.
    lua.globals().set(
        "GetExpansionLevel",
        lua.create_function(|_, _: mlua::MultiValue| Ok(2))?,
    )?;
    lua.globals().set(
        "GetAccountExpansionLevel",
        lua.create_function(|lua, _: mlua::MultiValue| {
            Ok(lua.app_data_ref::<Model>().expect("model").glue.account_expansion)
        })?,
    )?;

    // `GetAttackPowerForStat` (`PaperDollFrame.lua:272`): attack power breakdown for a given stat.
    lua.globals().set(
        "GetAttackPowerForStat",
        lua.create_function(|_, _: mlua::MultiValue| Ok((0i64, 0i64, 0i64)))?,
    )?;

    // `GetMirrorTimerInfo` (`MirrorTimer.lua:82`): breath/fatigue mirror timer info.
    lua.globals().set(
        "GetMirrorTimerInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok(("UNKNOWN", 0i64, 0i64, 0i64, 0i64, "")))?,
    )?;

    // `CalendarGetNumPendingInvites` (`GameTime.lua:157`): pending calendar invites count.
    lua.globals().set(
        "CalendarGetNumPendingInvites",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;

    // `IsXPUserDisabled` (`MainMenuBar.lua:344`): whether XP gain is locked at NPC.
    lua.globals().set(
        "IsXPUserDisabled",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;

    // `IsPossessBarVisible` (`BonusActionBarFrame.lua:221`): mind control / vehicle possess bar.
    lua.globals().set(
        "IsPossessBarVisible",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;

    // `BNGetNumBlocked` (`FriendsFrame.lua:452`): blocked Battle.net contacts count.
    lua.globals().set(
        "BNGetNumBlocked",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;

    // `GetNumVoiceSessions` (`ChannelFrame.lua:972`) & `IsVoiceChatAllowedByServer` (`AudioOptionsFrame.lua:41`).
    lua.globals().set(
        "GetNumVoiceSessions",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;
    lua.globals().set(
        "IsVoiceChatAllowedByServer",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;

    // `GetTotemInfo` (`TotemFrame.lua:34`, `MultiCastActionBarFrame.lua:313`): haveTotem, name, startTime, duration, icon.
    lua.globals().set(
        "GetTotemInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok((false, "", 0i64, 0i64, "")))?,
    )?;

    // `GetRuneType` (`RuneFrame.lua:71`): Death Knight rune types (1=Blood, 2=Unholy, 3=Frost, 4=Death).
    lua.globals().set(
        "GetRuneType",
        lua.create_function(|_, slot: Option<usize>| {
            let rune_type = match slot.unwrap_or(1) {
                1 | 2 => 1,
                3 | 4 => 2,
                5 | 6 => 3,
                _ => 1,
            };
            Ok(rune_type)
        })?,
    )?;

    // `GetHonorCurrency` (`PVPFrame.lua:515`): currency tab honor points.
    lua.globals().set(
        "GetHonorCurrency",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;

    // `GetLFGRoles` (`LFGFrame.lua:251`) & `UnitGroupRolesAssigned` (`PlayerFrame.lua:250`): LFG roles.
    lua.globals().set(
        "GetLFGRoles",
        lua.create_function(|_, _: mlua::MultiValue| Ok((false, false, false, false)))?,
    )?;
    lua.globals().set(
        "UnitGroupRolesAssigned",
        lua.create_function(|_, _: mlua::MultiValue| Ok("NONE"))?,
    )?;

    // `GetNumMapDebugObjects` (`WorldMapFrame.lua:402`): debug map pins.
    lua.globals().set(
        "GetNumMapDebugObjects",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0i64))?,
    )?;

    // `GetSelectedDisplayChannel` (`ChannelFrame.lua:46`, `:51`): selected chat channel index.
    lua.globals().set(
        "GetSelectedDisplayChannel",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::Value::Nil))?,
    )?;

    // `IsLoggedIn` (AceAddon-3.0.lua:626): indicates character is in-world.
    lua.globals().set(
        "IsLoggedIn",
        lua.create_function(|_, _: mlua::MultiValue| Ok(true))?,
    )?;

    // `GetPlayerFacing` (Astrolabe-0.4.lua:370): player yaw orientation in radians.
    lua.globals().set(
        "GetPlayerFacing",
        lua.create_function(|_, _: mlua::MultiValue| Ok(0.0f64))?,
    )?;

    // Named script handlers referenced in FrameXML / LFRFrame XML.
    lua.globals().set(
        "QueryGuildEventLog",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;
    lua.globals().set(
        "RefreshLFGList",
        lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
    )?;

    // Voice chat is permanently absent, but ChannelFrame still calls these display/voice verbs.
    // `GetActiveVoiceChannel` answers nil and `GetVoiceSessionInfo` answers nil. The real
    // `GetChannelDisplayInfo` (and `GetNumDisplayChannels`) are installed by `channel::install`.
    lua.globals().set(
        "GetActiveVoiceChannel",
        lua.create_function(|_, ()| Ok(mlua::Value::Nil))?,
    )?;
    lua.globals().set(
        "GetVoiceSessionInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok(mlua::Value::Nil))?,
    )?;

    // Possess bar: benilla has no possessed unit, so `GetPossessInfo` answers three nils, the
    // stock BonusActionBarFrame's empty bar.
    lua.globals().set(
        "GetPossessInfo",
        lua.create_function(|lua, index: mlua::Value| {
            // Reference gate: lua_tonumber(L, 1) == 0.0 (0x5D5820), read as a number before
            // truncation.
            let n = crate::script::binding_abi::float_arg(
                lua,
                index,
                "Usage: GetPossessInfo(index)",
            )?;
            if n == 0.0 {
                return Err(mlua::Error::runtime("Usage: GetPossessInfo(index)"));
            }
            Ok((
                mlua::Value::Nil,
                mlua::Value::Nil,
                mlua::Value::Nil,
            ))
        })?,
    )?;

    // LFG/dungeon finder is not wired: the stock frames see an empty dungeon list and no
    // backfill. `RequestLFDPlayerLockInfo` returns false because no
    // CMSG_LFG_PLAYER_LOCK_INFO_REQUEST is sent (reference 0x5569D0 returns true only
    // after sending it). `GetAvailableRoles` still follows the character's class, as the
    // reference reads ChrClasses.dbc.
    lua.globals().set(
        "CanPartyLFGBackfill",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "GetNumRandomDungeons",
        lua.create_function(|_, ()| Ok(0i64))?,
    )?;
    // `GetLFGRandomCooldownExpiration` (0x558060, aura 71328) and `GetLFGDeserterExpiration`
    // (0x5580E0, aura 71041): the player's native aura end times, the fallback pair stock
    // `LFDQueueFrameRandomCooldownFrame_Update` (`LFDFrame.lua:955`, `:962`) reads.
    lua.globals().set(
        "GetLFGRandomCooldownExpiration",
        lua.create_function(|lua, _: MultiValue| lfg_aura_expiration(lua, 71328))?,
    )?;
    lua.globals().set(
        "GetLFGDeserterExpiration",
        lua.create_function(|lua, _: MultiValue| lfg_aura_expiration(lua, 71041))?,
    )?;
    lua.globals().set(
        "UnitHasLFGRandomCooldown",
        lua.create_function(|lua, token: Value| lfg_party_aura(lua, token, 71328, "Usage: UnitHasLFGRandomCooldown(\"unit\")"))?,
    )?;
    lua.globals().set(
        "UnitHasLFGDeserter",
        lua.create_function(|lua, token: Value| lfg_party_aura(lua, token, 71041, "Usage: UnitHasLFGDeserter(\"unit\")"))?,
    )?;
    lua.globals().set(
        "RequestLFDPlayerLockInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok(false))?,
    )?;
    lua.globals().set(
        "GetAvailableRoles",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let class_id = lua
                .app_data_ref::<super::Model>()
                .map(|m| m.player_req.class_id)
                .unwrap_or(0);
            let roles = match class_id {
                1 | 6 => 0x0b_u8,
                2 | 11 => 0x0f_u8,
                5 | 7 => 0x0d_u8,
                3 | 4 | 8 | 9 => 0x09_u8,
                _ => 0_u8,
            };
            Ok((
                roles & 0x02 != 0,
                roles & 0x04 != 0,
                roles & 0x08 != 0,
            ))
        })?,
    )?;

    super::input_verbs::install_wrath(lua)?;
    install_multiclass(lua)?;

    Ok(())
}

/// WMC1 v3 (`MulticlassRequest` / `MulticlassPoll`) bindings shared by the Wrath in-game
/// (`WM_Multiclass.lua`) and glue (`multiclass_glue.lua`) Lua environments.
pub(super) fn install_multiclass(lua: &Lua) -> mlua::Result<()> {
    const MAX_MULTICLASS_QUEUE: usize = 16;

    lua.globals().set(
        "MulticlassRequest",
        lua.create_function(
            |lua, (op, arg0, arg1, guid): (u16, Option<u32>, Option<u32>, Option<u64>)| {
                let mut model = lua.app_data_mut::<super::Model>().expect("model");
                // The serial is allocated then advanced, so the first request is 1 (the
                // initialised value) rather than 2. `max(1)` keeps a wrapped 0 out of the wire.
                let serial = model.multiclass_next_serial.max(1);
                model.multiclass_next_serial = serial.wrapping_add(1).max(1);
                let char_guid = guid.unwrap_or_else(|| {
                    if model.unit_guids.player != 0 {
                        model.unit_guids.player
                    } else {
                        model.glue.roster.selected_guid().unwrap_or(0)
                    }
                });
                if model.multiclass_requests.len() >= MAX_MULTICLASS_QUEUE {
                    model.multiclass_requests.pop_front();
                }
                model.multiclass_requests.push_back((
                    op,
                    serial,
                    char_guid,
                    arg0.unwrap_or(0),
                    arg1.unwrap_or(0),
                ));
                Ok((true, serial))
            },
        )?,
    )?;

    lua.globals().set(
        "MulticlassPoll",
        lua.create_function(|lua, wanted_serial: Option<u32>| {
            let reply = {
                let mut model = lua.app_data_mut::<super::Model>().expect("model");
                match wanted_serial {
                    Some(target) => model
                        .multiclass_replies
                        .iter()
                        .position(|(_, _, serial, _)| *serial == target)
                        .and_then(|idx| model.multiclass_replies.remove(idx)),
                    None => model.multiclass_replies.pop_front(),
                }
            };
            let mut out = MultiValue::new();
            if let Some((op, status, serial, payload)) = reply {
                out.push_back(Value::Integer(i64::from(op)));
                out.push_back(Value::Integer(i64::from(status)));
                out.push_back(Value::Integer(i64::from(serial)));
                out.push_back(Value::String(lua.create_string(&payload)?));
            }
            Ok(out)
        })?,
    )?;

    Ok(())
}

/// The imported map placeholder requires the frame factory and widget method tables.
/// It does not implement quest-blob geometry or panning.
pub(super) fn install_frame_fallbacks(lua: &Lua) -> mlua::Result<()> {
    lua.load(r#"
        if not WorldMapBlobFrame then
            local f = CreateFrame("Frame", "WorldMapBlobFrame")
            if f then
                function f:DrawBlob() end
                function f:EnablePanning() end
            end
        end
    "#).exec()
}

/// `GetLuaCompanionType` (`SpellBookFrame_Scripts.cpp:165`): `lua_tolstring` then a
/// case-insensitive match against `CRITTER` (0) and `MOUNT` (1). A non-string or an unknown
/// token is the binding's usage error.
fn companion_type(lua: &Lua, value: Value, usage: &'static str) -> mlua::Result<u32> {
    let token = crate::script::binding_abi::optional_string(lua, &value)
        .ok_or_else(|| mlua::Error::runtime(usage))?;
    if token.eq_ignore_ascii_case("CRITTER") {
        Ok(0)
    } else if token.eq_ignore_ascii_case("MOUNT") {
        Ok(1)
    } else {
        Err(mlua::Error::runtime(usage))
    }
}

impl super::UiScript {
    /// Seed `GetNumTitles`: `CharTitles.dbc`'s record-id ceiling plus one, as
    /// `g_charTitlesDB.m_maxID + 1` (`0x005165E0`).
    pub fn set_title_count(&mut self, count: u32) {
        self.model_mut().title_count = count;
    }

    /// Drain queued `MulticlassRequest(op, arg0, arg1)` entries as `(op, serial, character_guid, arg0, arg1)`.
    pub fn take_multiclass_requests(&mut self) -> Vec<(u16, u32, u64, u32, u32)> {
        self.model_mut().multiclass_requests.drain(..).collect()
    }

    /// Enqueue a validated `SMSG_MULTICLASS_REPLY` (`0x51A`) frame `(op, status, serial, body)` for `MulticlassPoll`.
    pub fn push_multiclass_reply(&mut self, op: u16, status: u16, serial: u32, payload: Vec<u8>) {
        let mut model = self.model_mut();
        if model.multiclass_replies.len() >= 16 {
            model.multiclass_replies.pop_front();
        }
        model.multiclass_replies.push_back((op, status, serial, payload));
    }

    /// Clear queued multiclass requests and replies across world/character transitions.
    pub fn clear_multiclass_state(&mut self) {
        let mut model = self.model_mut();
        model.multiclass_requests.clear();
        model.multiclass_replies.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::script::{PetActionView, PetStats, UiScript, UiTarget, UnitState};

    #[test]
    fn wrath_ui_contracts_read_real_player_pet_and_title_state() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let mut ratings = [0u32; 25];
        ratings[4] = 37;
        s.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                has_object: true,
                combat_ratings: ratings,
                known_titles: [0x2, 0, 0],
                chosen_title: 1,
                player_flags: 0x6,
                ..Default::default()
            }),
        );
        assert_eq!(s.eval::<u32>("return GetCombatRating(5)").unwrap(), 37);
        assert_eq!(s.eval::<u32>("return GetCombatRating(1)").unwrap(), 0);
        assert!(s.run("GetCombatRating(0)").is_err());
        assert!(s.run("GetCombatRating(26)").is_err());
        assert_eq!(s.eval::<u32>("return GetCurrentTitle()").unwrap(), 1);
        assert_eq!(s.eval::<bool>("return IsTitleKnown(1) == 1").unwrap(), true);
        assert_eq!(s.eval::<bool>("return IsTitleKnown(2) == 0").unwrap(), true);
        assert_eq!(s.eval::<bool>("return UnitIsAFK('player') == 1").unwrap(), true);
        assert_eq!(s.eval::<bool>("return UnitIsDND('player') == 1").unwrap(), true);
        s.set_title_count(193);
        assert_eq!(s.eval::<u32>("return GetNumTitles()").unwrap(), 193);
        s.set_known_languages(vec!["Common".into(), "Dwarvish".into()]);
        assert_eq!(s.eval::<u32>("return GetNumLanguages()").unwrap(), 2);
        assert_eq!(s.eval::<u32>("return GetNumLaguages()").unwrap(), 2);
        assert_eq!(s.eval::<bool>("return CanExitVehicle() == nil").unwrap(), true);

        // `IsPetAttackAction` reads the server's packed command word, not a guess from the slot.
        s.set_pet_stats(true, PetStats::default());
        s.set_pet_actions(
            true,
            true,
            true,
            vec![
                PetActionView {
                    packed: 0x0700_0002,
                    ..Default::default()
                },
                PetActionView {
                    packed: 0x0700_0001,
                    ..Default::default()
                },
            ],
        );
        assert_eq!(s.eval::<bool>("return IsPetAttackAction(1)").unwrap(), true);
        assert_eq!(
            s.eval::<bool>("return not IsPetAttackAction(2)").unwrap(),
            true
        );
        assert_eq!(
            s.eval::<bool>("return not IsPetAttackAction(3)").unwrap(),
            true
        );

        // Companion usage gates match the reference; with no collection the in-range answer is
        // the empty list.
        assert_eq!(s.eval::<u32>("return GetNumCompanions('CRITTER')").unwrap(), 0);
        assert_eq!(
            s.eval::<i64>("return select('#', GetCompanionInfo('MOUNT', 1))").unwrap(),
            0
        );
        assert!(s.run("GetNumCompanions('PET')").is_err());
        assert!(s.run("GetCompanionInfo('CRITTER', 0)").is_err());
        assert_eq!(
            s.eval::<i64>("return select('#', GetTrackedAchievements())").unwrap(),
            0
        );
        assert_eq!(
            s.eval::<String>("return type(GetLFGQueuedList())").unwrap(),
            "table"
        );
        assert_eq!(
            s.eval::<bool>(
                "local t = { stale = 1 }; GetLFGQueuedList(t); return t.stale == nil"
            )
            .unwrap(),
            true
        );
        assert_eq!(s.eval::<u32>("return GetNumArenaOpponents()").unwrap(), 0);
        assert_eq!(
            s.eval::<u32>("return GetNumBattlegroundTypes()").unwrap(),
            0
        );
        assert_eq!(
            s.eval::<i64>("return select('#', GetArenaTeam(1))").unwrap(),
            22
        );
        assert_eq!(
            s.eval::<bool>("return GetArenaTeam(1) == nil").unwrap(),
            true
        );
        assert!(s.run("GetArenaTeam('x')").is_err());
    }

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
        // A link that names a different entry must not ride along on a prefix match.
        s.set_cursor_for_test(CursorPayload::Item(CursorItem {
            item_class: None,
            guid: 0x105,
            bar_placeable: true,
            consumable: false,
            bag: 0,
            slot: 1,
            item_id: 117,
            texture: None,
            link: Some("|cffffffff|Hitem:1170|h[Wrong Jerky]|h|r".into()),
            count: None,
            quality: Some(1),
            equip_slots: Vec::new(),
        }));
        assert_eq!(s.eval::<i64>("return select('#', GetCursorInfo())").unwrap(), 0);
        s.set_cursor_for_test(CursorPayload::Item(CursorItem {
            item_class: None, guid: 0x105, bar_placeable: true, consumable: false,
            bag: 0, slot: 1, item_id: 117, texture: None, link: None,
            count: None, quality: None, equip_slots: Vec::new(),
        }));
        assert_eq!(s.eval::<i64>("return select('#', GetCursorInfo())").unwrap(), 0);
        s.set_item_template(117, crate::script::ItemTemplateView {
            name: "Tough Jerky".into(), quality: 1, ..Default::default()
        });
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()").unwrap(),
            ("item".into(), 117.0, s.template_item_link(117, "Tough Jerky", 1))
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
        assert_eq!(s.eval::<i64>("return select('#', GetCursorInfo())").unwrap(), 0);
        s.set_item_template(159, crate::script::ItemTemplateView {
            name: "Water".into(), quality: 1, ..Default::default()
        });
        assert_eq!(
            s.eval::<(String, f64, String)>("return GetCursorInfo()").unwrap(),
            ("item".to_string(), 159.0, s.template_item_link(159, "Water", 1)),
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

    #[test]
    fn wrath_framexml_startup_natives_and_roles_are_defined() {
        use crate::script::{UiScript, UiTarget};
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_glue_account_expansion(1);
        assert_eq!(s.eval::<i64>("return GetExpansionLevel()").unwrap(), 2);
        assert_eq!(s.eval::<i64>("return GetAccountExpansionLevel()").unwrap(), 1);
        assert_eq!(s.eval::<String>("return UnitGroupRolesAssigned('player')").unwrap(), "NONE");
        assert_eq!(s.eval::<i64>("return GetNumTrackedAchievements(false)").unwrap(), 0);
        assert_eq!(s.eval::<i64>("return GetRuneType(1)").unwrap(), 1);
        assert_eq!(s.eval::<i64>("return GetRuneType(3)").unwrap(), 2);
        assert_eq!(s.eval::<i64>("return GetRuneType(5)").unwrap(), 3);
        assert_eq!(s.eval::<bool>("return IsXPUserDisabled()").unwrap(), false);
        assert_eq!(s.eval::<bool>("return IsLoggedIn()").unwrap(), true);
    }

    /// `GetLFGRandomCooldownExpiration`/`GetLFGDeserterExpiration`: `LFDFrame.lua:955`/`:962`
    /// read the player's native aura end time, or no values when the player, the observed aura
    /// list or the aura itself is absent.
    #[test]
    fn wrath_lfg_cooldown_expirations_read_the_native_player_aura() {
        use crate::script::host;

        const PLAYER: u64 = 0x10;

        struct AuraHost {
            guid: u64,
            auras: Option<Vec<crate::script::WrathAuraView>>,
            asked: Vec<u64>,
        }

        impl crate::script::CallHost for AuraHost {
            fn apply(
                &mut self,
                _script: &mut UiScript,
                _call: crate::script::ScriptCall,
            ) -> Option<crate::script::CallTail> {
                None
            }

            fn unit_auras(&mut self, guid: u64) -> Option<Vec<crate::script::WrathAuraView>> {
                self.asked.push(guid);
                if guid != self.guid {
                    return None;
                }
                self.auras.clone()
            }
        }

        fn aura(slot: u8, spell_id: u32, expiration: Option<f64>) -> crate::script::WrathAuraView {
            crate::script::WrathAuraView {
                slot,
                spell_id,
                name: String::new(),
                rank: None,
                icon: None,
                count: 1,
                level: 80,
                dispel: None,
                helpful: Some(false),
                caster: None,
                duration: 0.0,
                expiration,
                cancelable: false,
                stealable: None,
                consolidate: None,
            }
        }

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        // No player guid: no values, and the host is never asked.
        assert_eq!(s.arity("GetLFGDeserterExpiration()").unwrap(), 0);
        assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 0);

        s.seat_player_guid(PLAYER);
        // No host lent: no values.
        assert_eq!(s.arity("GetLFGDeserterExpiration()").unwrap(), 0);

        let mut host = AuraHost {
            guid: PLAYER,
            auras: None,
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            // The generation's aura packet has not arrived: unavailable, not observed-empty.
            assert_eq!(s.arity("GetLFGDeserterExpiration()").unwrap(), 0);
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 0);
        });

        // The random-dungeon cooldown, hidden from the bar, is still in the native list; the
        // unrelated aura after it is not a match.
        host.auras = Some(vec![aura(33, 71328, Some(1_234.5)), aura(34, 1243, Some(99.0))]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 1);
            assert_eq!(
                s.eval::<f64>("return GetLFGRandomCooldownExpiration()").unwrap(),
                1_234.5
            );
            assert_eq!(
                s.arity("GetLFGDeserterExpiration()").unwrap(),
                0,
                "each binding reads its own spell"
            );
        });

        // The deserter, and an unrelated-only list: no values.
        host.auras = Some(vec![aura(32, 71041, Some(1_500.0))]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<f64>("return GetLFGDeserterExpiration()").unwrap(),
                1_500.0
            );
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 0);
        });
        host.auras = Some(vec![aura(32, 1243, Some(30.0))]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("GetLFGDeserterExpiration()").unwrap(), 0);
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 0);
        });

        // A match with no timer is one zero end time, which the caller treats as expired.
        host.auras = Some(vec![aura(33, 71328, None)]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 1);
            assert_eq!(
                s.eval::<f64>("return GetLFGRandomCooldownExpiration()").unwrap(),
                0.0
            );
        });

        // The list removed (another object generation): no values again.
        host.auras = None;
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("GetLFGDeserterExpiration()").unwrap(), 0);
            assert_eq!(s.arity("GetLFGRandomCooldownExpiration()").unwrap(), 0);
        });
        assert!(
            host.asked.iter().all(|&guid| guid == PLAYER),
            "the host is asked for the player's guid only"
        );
    }

    /// `UnitHasLFGRandomCooldown("unit")` / `UnitHasLFGDeserter("unit")`: boolean aura
    /// presence for the named unit, zero values for a missing unit/host/list.
    #[test]
    fn wrath_lfg_party_auras_are_boolean_predicates() {
        use crate::script::{UiScript, UiTarget, UnitGuids};
        use crate::script::host;

        const PLAYER: u64 = 0x10;
        const PARTY1: u64 = 0x20;

        struct AuraHost {
            asked: Vec<u64>,
            auras: Option<Vec<crate::script::WrathAuraView>>,
            answer_for: u64,
        }

        impl crate::script::CallHost for AuraHost {
            fn apply(
                &mut self,
                _script: &mut UiScript,
                _call: crate::script::ScriptCall,
            ) -> Option<crate::script::CallTail> {
                None
            }

            fn unit_auras(&mut self, guid: u64) -> Option<Vec<crate::script::WrathAuraView>> {
                self.asked.push(guid);
                if guid != self.answer_for {
                    return None;
                }
                self.auras.clone()
            }
        }

        fn aura(spell_id: u32) -> crate::script::WrathAuraView {
            crate::script::WrathAuraView {
                slot: 1,
                spell_id,
                name: String::new(),
                rank: None,
                icon: None,
                count: 1,
                level: 80,
                dispel: None,
                helpful: Some(false),
                caster: None,
                duration: 0.0,
                expiration: None,
                cancelable: false,
                stealable: None,
                consolidate: None,
            }
        }

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        // Non-string argument -> usage error.
        assert!(s.run("UnitHasLFGRandomCooldown(5)").is_err());
        assert!(s.run("UnitHasLFGDeserter(nil)").is_err());
        assert!(s.run("UnitHasLFGRandomCooldown(true)").is_err());

        // No player guid / unknown guid -> zero values.
        assert_eq!(s.arity("UnitHasLFGRandomCooldown('player')").unwrap(), 0);
        assert_eq!(s.arity("UnitHasLFGDeserter('player')").unwrap(), 0);

        s.set_unit_guids(&UnitGuids {
            player: PLAYER,
            party: [PARTY1, 0, 0, 0],
            wrath: true,
            ..Default::default()
        });

        // No host lent -> zero values.
        assert_eq!(s.arity("UnitHasLFGRandomCooldown('player')").unwrap(), 0);
        assert_eq!(s.arity("UnitHasLFGDeserter('player')").unwrap(), 0);

        // Unknown guid (party2 slot is 0) -> zero values, host not asked for 0.
        assert_eq!(s.arity("UnitHasLFGRandomCooldown('party2')").unwrap(), 0);

        let mut host = AuraHost {
            asked: Vec::new(),
            auras: None,
            answer_for: PLAYER,
        };

        // None list -> zero values.
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("UnitHasLFGRandomCooldown('player')").unwrap(), 0);
            assert_eq!(s.arity("UnitHasLFGDeserter('player')").unwrap(), 0);
        });

        // Player aura present -> true for the matching spell only.
        host.answer_for = PLAYER;
        host.auras = Some(vec![aura(71328)]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<bool>("return UnitHasLFGRandomCooldown('player')").unwrap(),
                true
            );
            assert_eq!(
                s.eval::<bool>("return UnitHasLFGDeserter('player')").unwrap(),
                false
            );
        });

        // Absent spell -> false.
        host.auras = Some(vec![aura(1243)]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<bool>("return UnitHasLFGRandomCooldown('player')").unwrap(),
                false
            );
        });

        // Party1 aura present -> true.
        host.answer_for = PARTY1;
        host.auras = Some(vec![aura(71041)]);
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<bool>("return UnitHasLFGDeserter('party1')").unwrap(),
                true
            );
            assert_eq!(
                s.eval::<bool>("return UnitHasLFGRandomCooldown('party1')").unwrap(),
                false
            );
        });

        // Player/party isolation: player has no aura now (None) while party1 has it.
        host.answer_for = PLAYER;
        host.auras = None;
        host::lend(s.lua(), &mut host, || {
            assert_eq!(s.arity("UnitHasLFGRandomCooldown('player')").unwrap(), 0);
        });
        assert!(
            host.asked.iter().all(|&g| g == PLAYER || g == PARTY1),
            "host is asked only for resolved guids"
        );
    }

    #[test]
    fn wrath_multiclass_request_and_poll_round_trip_in_world_and_glue() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.seat_player_guid(0x0102_0304_0506_0708);
        let (ok, serial): (bool, u32) = s
            .eval("return MulticlassRequest(1, 2, 9)")
            .expect("MulticlassRequest in world VM");
        assert!(ok);
        assert_eq!(serial, 1);
        assert_eq!(
            s.take_multiclass_requests(),
            vec![(1, 1, 0x0102_0304_0506_0708, 2, 9)]
        );
        assert_eq!(s.arity("MulticlassPoll()").unwrap(), 0);
        s.push_multiclass_reply(1, 0, 1, b"WMC1".to_vec());
        let (op, status, got_serial, payload): (u16, u16, u32, String) = s
            .eval("return MulticlassPoll(1)")
            .expect("MulticlassPoll in world VM");
        assert_eq!((op, status, got_serial, payload.as_bytes()), (1, 0, 1, b"WMC1".as_slice()));

        let mut glue = UiScript::new_glue_with_target(UiTarget::Wrath335).unwrap();
        let (ok_glue, serial_glue): (bool, u32) = glue
            .eval("return MulticlassRequest(6, 11, 0, 42)")
            .expect("MulticlassRequest in glue VM");
        assert!(ok_glue);
        assert_eq!(serial_glue, 1);
        assert_eq!(glue.take_multiclass_requests(), vec![(6, 1, 42, 11, 0)]);
    }
}
