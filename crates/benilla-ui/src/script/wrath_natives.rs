//! Wrath client-query natives the stock 3.3.5a FrameXML calls at load and the 1.12 interpreter
//! has no verb for. Device queries describe supported local hardware. Some imported startup
//! bindings below remain compatibility fallbacks rather than projections of live gameplay
//! state; registering them does not establish support for those subsystems.
//!
//! The QuestPOI bindings are one such fallback: they answer the reference's empty branch because
//! the quest-blob pipeline has no producer or renderer yet. See `install_quest_poi`.

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
                })
                .unwrap_or_else(|| {
                    super::item_stats::item_link_for_target(
                        model.target,
                        item.item_id,
                        "Item",
                        1,
                    )
                });
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
            let player_slots = if model.spell_list.is_empty() { &model.spellbook.slots } else { &model.spell_list };
            let (slot, book) = match slot(player_slots) {
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
            let link = model
                .item_templates
                .get(&action.action)
                .map(|template| {
                    super::item_stats::item_link_for_target(
                        model.target,
                        action.action,
                        &template.name,
                        template.quality,
                    )
                })
                .unwrap_or_else(|| {
                    super::item_stats::item_link_for_target(
                        model.target,
                        action.action,
                        "Item",
                        1,
                    )
                });
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

/// The 3.3.5 rune recharge window the client's `CGRuneInfo::ResyncRunes` uses to turn the writer's
/// 0..255 progress byte into a `GetTime()` start (`RuneInfo.cpp:411`: `10000.0f * progress`).
/// `GetRuneCooldown`'s second return is the same ten-second recharge for a death knight.
const RUNE_RECHARGE_SECONDS: f64 = 10.0;

/// `WowTime::WowDecodeTime` (`WowTime.cpp:364-399`) plus `CalendarGetDate`'s one-based shape
/// (`Calendar_Scripts.cpp:75-80`): weekday, month, day and year, or zero/1999 for a field the
/// writer marked unknown. A missing calendar reply is handled by the caller and returns four
/// zeros, the stock clock frame's load-time answer.
fn calendar_date(zone_time: u32) -> (i64, i64, i64, i64) {
    let field = |shift: u32, mask: u32, sentinel: u32| -> i64 {
        let value = (zone_time >> shift) & mask;
        if value == sentinel {
            -1
        } else {
            i64::from(value)
        }
    };
    (
        field(11, 7, 7) + 1,
        field(20, 0xf, 0xf) + 1,
        field(14, 0x3f, 0x3f) + 1,
        field(24, 0x1f, 0x1f) + 2000,
    )
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
    // `UnitIsTalking(unit)` (`ComSatClient_Scripts.cpp` 0x007DF0B0): non-string -> no values;
    // a string -> 1.0 if the unit is in the voice talking set, else nil. Voice chat is permanently
    // absent, so every string answers nil; non-string preserves the zero-return arity.
    lua.globals().set(
        "UnitIsTalking",
        lua.create_function(|lua, unit: Value| -> mlua::Result<MultiValue> {
            if lua.coerce_string(unit)?.is_none() {
                return Ok(MultiValue::new());
            }
            // Voice absent: a string answers nil (one value), not 1.0.
            Ok(MultiValue::from_vec(vec![Value::Nil]))
        })?,
    )?;
    // `GetLFGQueuedList([table])` (`LFGInfo_Scripts.cpp:544`): the caller's table, cleared and
    // filled with the queued dungeon slots (`table[slot] = true`).
    lua.globals().set(
        "GetLFGQueuedList",
        lua.create_function(|lua, arg: Option<Value>| {
            let table = match arg {
                Some(Value::Table(table)) => {
                    table.clear()?;
                    table
                }
                Some(Value::Nil) | None => lua.create_table()?,
                Some(_) => return Err(mlua::Error::runtime("Usage: GetLFDQueuedList([table])")),
            };
            let dungeons = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_lfg
                .queued_dungeons
                .clone();
            for slot in dungeons {
                table.set(i64::from(slot), true)?;
            }
            Ok(Value::Table(table))
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

    // `VehicleExit`, `VehiclePrevSeat`, `VehicleNextSeat`, `UnitSwitchToVehicleSeat`, `CanExitVehicle`
    // (`UnitVehicle_C.cpp:744-818`): enqueue scoped 12340 vehicle requests when the player is in a
    // replicated vehicle scope.
    lua.globals().set(
        "VehicleExit",
        lua.create_function(|lua, _args: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::VehicleExit {
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "VehiclePrevSeat",
        lua.create_function(|lua, _args: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::VehiclePrevSeat {
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "VehicleNextSeat",
        lua.create_function(|lua, _args: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::VehicleNextSeat {
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "UnitSwitchToVehicleSeat",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let seat = args
                .get(1)
                .and_then(|v| lua.coerce_integer(v.clone()).ok().flatten())
                .unwrap_or(1) as i8;
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let (Some((session_epoch, world_generation)), Some(vehicle)) =
                (model.wrath_scope, model.vehicle_guid)
            {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::VehicleSwitchSeat {
                            vehicle,
                            seat,
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "CanExitVehicle",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.vehicle_guid.is_some())
        })?,
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

    // `UnitInVehicle`, `UnitHasVehicleUI`, `UnitTargetsVehicleInRaidUI`, `UnitInVehicleControlSeat`:
    // vehicle unitframe queries. PetFrame and UnitFrame check `UnitInVehicle` to position vehicle
    // frames; when absent, target and pet frame initialization crash.
    lua.globals().set(
        "UnitInVehicle",
        lua.create_function(|lua, unit: Option<String>| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let in_veh = unit.as_deref() == Some("player") && model.vehicle_guid.is_some();
            Ok(in_veh)
        })?,
    )?;
    lua.globals().set(
        "UnitHasVehicleUI",
        lua.create_function(|_, _unit: Option<String>| Ok(false))?,
    )?;
    lua.globals().set(
        "UnitTargetsVehicleInRaidUI",
        lua.create_function(|_, _unit: Option<String>| Ok(false))?,
    )?;
    lua.globals().set(
        "UnitInVehicleControlSeat",
        lua.create_function(|_, _unit: Option<String>| Ok(false))?,
    )?;

    // Character PaperDoll and stats window natives (`PaperDollFrame.lua:280`, `:292`, etc.)
    lua.globals().set(
        "GetUnitMaxHealthModifier",
        lua.create_function(|_, _unit: Option<String>| Ok(0.0))?,
    )?;
    lua.globals().set(
        "GetCombatRatingBonus",
        lua.create_function(|_, _rating: Option<u32>| Ok(0.0))?,
    )?;
    lua.globals().set(
        "GetSpellBonusDamage",
        lua.create_function(|_, _school: Option<u32>| Ok(0))?,
    )?;
    lua.globals().set(
        "GetSpellBonusHealing",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "GetInventoryItemsForSlot",
        lua.create_function(|lua, _slot: Option<i64>| {
            let t = lua.create_table()?;
            Ok(t)
        })?,
    )?;
    lua.globals().set(
        "GetBackpackCurrencyInfo",
        lua.create_function(|_, _index: Option<i64>| Ok(Value::Nil))?,
    )?;
    // Equipment sets (`GetNumEquipmentSets`, read by the gear-manager dialog's OnLoad in
    // `PaperDollFrame.lua:2156`): the host has no equipment-set backend yet (`equipmentManager`
    // is an inert CVar), so the count is the reference's answer with none saved — zero — and the
    // load-time loop does not run.
    lua.globals().set(
        "GetNumEquipmentSets",
        lua.create_function(|_, ()| Ok(0))?,
    )?;
    lua.globals().set(
        "CanShowAchievementUI",
        lua.create_function(|_, ()| Ok(true))?,
    )?;
    lua.globals().set(
        "GetCurrentArenaSeason",
        lua.create_function(|_, ()| Ok(8))?,
    )?;
    lua.globals().set(
        "IsAtStableMaster",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "GetVoiceStatus",
        lua.create_function(|_, _unit: Option<String>| Ok(Value::Nil))?,
    )?;
    lua.globals().set(
        "VoiceChat_Animate",
        lua.create_function(|_, (_frame, _state): (Value, Value)| Ok(()))?,
    )?;
    lua.globals().set(
        "HasLFGRestrictions",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "RequestLFDPartyLockInfo",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    lua.globals().set(
        "IsBattlefieldArena",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "GetRandomBGHonorCurrencyBonuses",
        lua.create_function(|_, ()| Ok((0, 0, 0, 0)))?,
    )?;
    lua.globals().set(
        "ClearFocus",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    lua.globals().set(
        "BNGetNumFriendInvites",
        lua.create_function(|_, ()| Ok(0))?,
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

    // `GetLFGProposal` (`UIParent.lua:3571`, `LFGInfo_Scripts.cpp:414`): returns
    // `(proposalExists, id, typeID, subtypeID, name, texture, role, hasResponded, totalEncounters,
    // completedEncounters, numMembers, isLeader, isHoliday)` when a proposal is active.
    lua.globals().set(
        "GetLFGProposal",
        lua.create_function(|lua, ()| -> mlua::Result<MultiValue> {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(proposal) = model.wrath_lfg.proposal.as_ref().filter(|p| !p.silent) else {
                return Ok(MultiValue::from_vec(vec![Value::Boolean(false)]));
            };
            let local = proposal.players.iter().find(|p| p.is_local).copied();
            let role_str = match local.map_or(0, |p| p.role) {
                r if r & 0x02 != 0 => "TANK",
                r if r & 0x04 != 0 => "HEALER",
                r if r & 0x08 != 0 => "DAMAGER",
                _ => "NONE",
            };
            let is_leader = local.is_some_and(|p| p.role & 0x01 != 0);
            let has_responded = local.is_some_and(|p| p.responded);
            let dungeon_id = proposal.dungeon_id & 0x00FF_FFFF;
            let completed = proposal.completed_mask.count_ones();
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(true),
                Value::Integer(i64::from(proposal.proposal_id)),
                Value::Integer(1),
                Value::Integer(1),
                Value::String(lua.create_string(&format!("Dungeon #{dungeon_id}"))?),
                Value::String(lua.create_string("")?),
                Value::String(lua.create_string(role_str)?),
                Value::Boolean(has_responded),
                Value::Integer(0),
                Value::Integer(i64::from(completed)),
                Value::Integer(proposal.players.len() as i64),
                Value::Boolean(is_leader),
                Value::Boolean(false),
            ]))
        })?,
    )?;
    // `GetLFGProposalMember(index)` (`LFGInfo_Scripts.cpp:469`): `(role, isPlayer, isInDungeon,
    // isSameGroup, hasResponded, accepted)`.
    lua.globals().set(
        "GetLFGProposalMember",
        lua.create_function(|lua, index: Option<i64>| -> mlua::Result<MultiValue> {
            let idx = index.unwrap_or(1).saturating_sub(1) as usize;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(member) = model
                .wrath_lfg
                .proposal
                .as_ref()
                .and_then(|p| p.players.get(idx))
            else {
                return Ok(MultiValue::new());
            };
            let role_str = match member.role {
                r if r & 0x02 != 0 => "TANK",
                r if r & 0x04 != 0 => "HEALER",
                r if r & 0x08 != 0 => "DAMAGER",
                _ => "NONE",
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(role_str)?),
                Value::Boolean(member.is_local),
                Value::Boolean(member.in_dungeon),
                Value::Boolean(member.in_same_group),
                Value::Boolean(member.responded),
                Value::Boolean(member.accepted),
            ]))
        })?,
    )?;
    // `GetLFGInfoServer` (`UIParent.lua:3572`, `LFGInfo_Scripts.cpp:250`): `(inParty, joined,
    // queued, noPartialClear, achievements, lfgComment, slotCount)`.
    lua.globals().set(
        "GetLFGInfoServer",
        lua.create_function(|lua, ()| -> mlua::Result<MultiValue> {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let lfg = &model.wrath_lfg;
            let comment = if lfg.joined {
                Value::String(lua.create_string(&lfg.comment)?)
            } else {
                Value::Nil
            };
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(lfg.lfg_joined),
                Value::Boolean(lfg.joined),
                Value::Boolean(lfg.queued),
                Value::Boolean(lfg.no_partial_clear),
                Value::Boolean(lfg.achievements),
                comment,
                Value::Integer(lfg.queued_dungeons.len() as i64),
            ]))
        })?,
    )?;
    // `GetLFGQueueStats` (`LFGFrame.lua:392`, `LFGInfo_Scripts.cpp:365`):
    // `(hasData, leaderNeeds, tankNeeds, healerNeeds, dpsNeeds, instanceType, instanceName,
    // tankWait, healerWait, dpsWait, averageWait, queuedTime)`
    lua.globals().set(
        "GetLFGQueueStats",
        lua.create_function(|lua, ()| -> mlua::Result<MultiValue> {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(stats) = model.wrath_lfg.queue_status else {
                return Ok(MultiValue::from_vec(vec![Value::Boolean(false)]));
            };
            let dungeon_id = stats.dungeon_id & 0x00FF_FFFF;
            Ok(MultiValue::from_vec(vec![
                Value::Boolean(true),
                Value::Integer(0),
                Value::Integer(i64::from(stats.tanks_needed)),
                Value::Integer(i64::from(stats.healers_needed)),
                Value::Integer(i64::from(stats.dps_needed)),
                Value::Integer(1),
                Value::String(lua.create_string(&format!("Dungeon #{dungeon_id}"))?),
                Value::Integer(i64::from(stats.wait_time_tank)),
                Value::Integer(i64::from(stats.wait_time_healer)),
                Value::Integer(i64::from(stats.wait_time_dps)),
                Value::Integer(i64::from(stats.wait_time_avg)),
                Value::Integer(i64::from(stats.queued_time)),
            ]))
        })?,
    )?;
    lua.globals().set(
        "GetLFGRoleUpdate",
        lua.create_function(|_, ()| Ok((false, mlua::Value::Nil, mlua::Value::Nil)))?,
    )?;

    // `IsListedInLFR` and `IsPartyLFG` (`UIParent.lua:3583`, `:3585`).
    lua.globals().set(
        "IsListedInLFR",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    lua.globals().set(
        "IsPartyLFG",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.wrath_lfg.lfg_joined)
        })?,
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
    // `InCombatLockdown` (`GameUI_Scripts.cpp`): 1.0 when protected actions are not allowed, nil
    // otherwise. Project B has no secure/taint lockdown; the player's `in_combat` is the practical
    // lockdown signal (approximation, documented).
    lua.globals().set(
        "InCombatLockdown",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(if model.unit("player").is_some_and(|u| u.in_combat) {
                Value::Number(1.0)
            } else {
                Value::Nil
            })
        })?,
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
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model
                .calendar
                .map_or((0i64, 0i64, 0i64, 0i64), |calendar| {
                    calendar_date(calendar.zone_time)
                }))
        })?,
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
    // `GetCritChanceFromAgility("unit")` (`ScriptEvents_Scripts.cpp` 0x0060E130): non-string raises
    // `Usage: GetCritChanceFromAgility("unit")`; active player or pet -> `unit->GetMeleeChanceToCrit()`,
    // else 0. Project B only has the player's `PLAYER_CRIT_PERCENTAGE` scalar, so "player" returns it
    // and every other unit (including "pet", a named gap) returns 0.
    lua.globals().set(
        "GetCritChanceFromAgility",
        lua.create_function(|lua, unit: Value| {
            let unit = crate::script::binding_abi::string_arg(
                lua,
                unit,
                "Usage: GetCritChanceFromAgility(\"unit\")",
            )?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            if unit.eq_ignore_ascii_case("player") {
                Ok(model.player_crit_percentage as f64)
            } else {
                Ok(0.0f64)
            }
        })?,
    )?;

    // `GetMirrorTimerInfo` (`MirrorTimer.lua:82`): breath/fatigue mirror timer info.
    lua.globals().set(
        "GetMirrorTimerInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok(("UNKNOWN", 0i64, 0i64, 0i64, 0i64, "")))?,
    )?;

    // `CalendarGetNumPendingInvites` (`GameTime.lua:157`): pending calendar invites count.
    lua.globals().set(
        "CalendarGetNumPendingInvites",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model
                .calendar
                .map_or(0i64, |calendar| calendar.pending_invites as i64))
        })?,
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
    // `BNGetNumBlockedToons` (`FriendsFrame.lua:453`): blocked toons count. Battle.net is
    // permanently absent, so 0, matching `BNGetNumBlocked`.
    lua.globals().set(
        "BNGetNumBlockedToons",
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
    // `GetNumVoiceSessionMembersBySessionID(id)` (`ChannelFrame.lua:1020`): member count for a
    // voice session. Voice chat is permanently absent, so no values for any valid id (and none
    // for non-number or id > 31).
    lua.globals().set(
        "GetNumVoiceSessionMembersBySessionID",
        lua.create_function(|lua, id: Value| -> mlua::Result<MultiValue> {
            let Some(n) = lua.coerce_number(id)? else {
                return Ok(MultiValue::new());
            };
            let id = n as i64;
            if id < 0 || id > 31 {
                return Ok(MultiValue::new());
            }
            // Voice absent: no matching session, so no values.
            Ok(MultiValue::new())
        })?,
    )?;
    // `GetVoiceSessionMemberInfoBySessionID(session, index)` (ComSatClient): five values, none when
    // voice is absent or args are invalid/out of range.
    lua.globals().set(
        "GetVoiceSessionMemberInfoBySessionID",
        lua.create_function(|lua, args: MultiValue| -> mlua::Result<MultiValue> {
            // Non-number either arg (or wrong arity) -> no values, never a raise.
            if args.len() < 2 {
                return Ok(MultiValue::new());
            }
            let session = args.get(0).cloned().unwrap_or(Value::Nil);
            let index = args.get(1).cloned().unwrap_or(Value::Nil);
            if lua.coerce_number(session)?.is_none() || lua.coerce_number(index)?.is_none() {
                return Ok(MultiValue::new());
            }
            // Voice absent: no matching session, so no values.
            Ok(MultiValue::new())
        })?,
    )?;

    // `GetTotemInfo` (`TotemFrame.lua:34`, `MultiCastActionBarFrame.lua:313`): haveTotem, name, startTime, duration, icon.
    lua.globals().set(
        "GetTotemInfo",
        lua.create_function(|_, _: mlua::MultiValue| Ok((false, "", 0i64, 0i64, "")))?,
    )?;

    // `GetRuneType` (`ScriptEvents_Scripts.cpp:3035-3052`): the decoded 0-based `RuneType` plus
    // one, from `CGRuneInfo::m_runes`. The slot argument is 1-based FrameXML order, and the
    // decoded layout is the server's (blood/unholy/frost/death), never a fixed class grouping.
    lua.globals().set(
        "GetRuneType",
        lua.create_function(|lua, slot: Value| -> mlua::Result<MultiValue> {
            let slot = crate::script::binding_abi::number_arg(
                lua,
                slot,
                "Usage: GetRuneType(slot)",
            )?;
            let index = slot - 1;
            if !(0..=7).contains(&index) {
                return Ok(MultiValue::new());
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(runes) = model.runes.as_deref() else {
                return Ok(MultiValue::new());
            };
            let Some(rune) = runes.get(index as usize) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![Value::Number(
                f64::from(rune.rune_type) + 1.0,
            )]))
        })?,
    )?;
    // `GetRuneCooldown` (`ScriptEvents_Scripts.cpp:3097-3122`): start, duration and ready, all on
    // the `GetTime()` clock. `ResyncRunes` stores `now - 10000 ms * progress`, so the start is the
    // decode clock minus the fraction of the ten-second recharge. No state yet is the reference's
    // no-player answer: no values.
    lua.globals().set(
        "GetRuneCooldown",
        lua.create_function(|lua, slot: Value| -> mlua::Result<MultiValue> {
            let slot = crate::script::binding_abi::number_arg(
                lua,
                slot,
                "Usage: GetRuneCooldown(slot)",
            )?;
            let index = slot - 1;
            if !(0..=7).contains(&index) {
                return Err(mlua::Error::runtime("Invalid slot"));
            }
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Some(runes) = model.runes.as_deref() else {
                return Ok(MultiValue::new());
            };
            let Some(rune) = runes.get(index as usize) else {
                return Ok(MultiValue::new());
            };
            // The writer's byte is a 0..255 progress fraction. Divide by the exact range rather
            // than a truncated decimal constant so 255 is ready at the decode instant instead of
            // missing by a floating-point epsilon.
            let progress = f64::from(rune.cooldown) / 255.0;
            let start = rune.received_at - progress * RUNE_RECHARGE_SECONDS;
            let ready = start + RUNE_RECHARGE_SECONDS <= model.now;
            Ok(MultiValue::from_vec(vec![
                Value::Number(start),
                Value::Number(RUNE_RECHARGE_SECONDS),
                Value::Boolean(ready),
            ]))
        })?,
    )?;
    // `OpenCalendar` (`Calendar_Scripts.cpp:1855-1861`): the stock frame's only fetch. The request
    // is tagged with the world scope the feed pushed, so a calendar opened across a transfer is
    // refused by the Wrath tap instead of fetching the replacement world under the old UI.
    lua.globals().set(
        "OpenCalendar",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some(scope) = model.wrath_scope {
                if model.calendar_fetches.len() >= 4 {
                    model.calendar_fetches.pop_front();
                }
                model.calendar_fetches.push_back(scope);
            }
            Ok(())
        })?,
    )?;

    // `GetHonorCurrency` / `GetArenaCurrency` (`ScriptEvents_Scripts.cpp` 0x0060FC40 / 0x0060FCC0):
    // exactly two values, (currency, cap); honor cap 75000, arena cap 10000. Read from the pushed
    // `HonorState`; absent reads 0.
    lua.globals().set(
        "GetHonorCurrency",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let currency = model.honor.map_or(0u32, |h| h.honor_currency);
            Ok((i64::from(currency), 75000i64))
        })?,
    )?;
    lua.globals().set(
        "GetArenaCurrency",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let currency = model.honor.map_or(0u32, |h| h.arena_currency);
            Ok((i64::from(currency), 10000i64))
        })?,
    )?;

    // `GetLFGRoles`, `SetLFGRoles`, `SetLFGComment`, `SetLFGDungeon`, `ClearAllLFGDungeons`,
    // `JoinLFG`, `LeaveLFG`, `AcceptProposal`, `RejectProposal` (`LFGFrame.lua`,
    // `CGLookingForGroup.cpp:183-241`).
    lua.globals().set(
        "GetLFGRoles",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let roles = lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .wrath_lfg
                .roles;
            Ok((
                roles & 0x01 != 0,
                roles & 0x02 != 0,
                roles & 0x04 != 0,
                roles & 0x08 != 0,
            ))
        })?,
    )?;
    lua.globals().set(
        "SetLFGRoles",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let flag_at = |idx: usize| {
                matches!(
                    args.get(idx),
                    Some(Value::Boolean(true)) | Some(Value::Integer(1..))
                )
            };
            let mut roles = 0u8;
            if flag_at(0) {
                roles |= 0x01;
            }
            if flag_at(1) {
                roles |= 0x02;
            }
            if flag_at(2) {
                roles |= 0x04;
            }
            if flag_at(3) {
                roles |= 0x08;
            }
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.wrath_lfg.roles = roles;
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::LfgSetRoles {
                            roles,
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "SetLFGComment",
        lua.create_function(|lua, comment: Option<String>| {
            let mut comment = comment.unwrap_or_default();
            comment.truncate(256);
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.wrath_lfg.comment = comment.clone();
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::SetLfgComment {
                            comment,
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "SetLFGDungeon",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let slot = match (args.front(), args.get(1)) {
                (Some(a), Some(b)) => {
                    let kind = lua.coerce_integer(a.clone())?.unwrap_or(0) as u32;
                    let id = lua.coerce_integer(b.clone())?.unwrap_or(0) as u32;
                    if id > 0 && kind <= 0xFF {
                        (id & 0x00FF_FFFF) | ((kind & 0xFF) << 24)
                    } else {
                        kind
                    }
                }
                (Some(a), None) => lua.coerce_integer(a.clone())?.unwrap_or(0) as u32,
                _ => 0,
            };
            if slot != 0 {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                if !model.wrath_lfg.selected_slots.contains(&slot)
                    && model.wrath_lfg.selected_slots.len() < 50
                {
                    model.wrath_lfg.selected_slots.push(slot);
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "ClearAllLFGDungeons",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.wrath_lfg.selected_slots.clear();
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "JoinLFG",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let mut slots = model.wrath_lfg.selected_slots.clone();
            if slots.is_empty() {
                for arg in &args {
                    if let Ok(Some(slot)) = lua.coerce_integer(arg.clone()) {
                        if slot > 0 && slots.len() < 50 {
                            slots.push(slot as u32);
                        }
                    }
                }
            }
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    let comment = model.wrath_lfg.comment.clone();
                    let roles = model.wrath_lfg.roles;
                    let no_partial_clear = model.wrath_lfg.no_partial_clear;
                    let achievements = model.wrath_lfg.achievements;
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::LfgJoin {
                            slots,
                            comment,
                            roles,
                            no_partial_clear,
                            achievements,
                            needs: [0, 0, 0],
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "LeaveLFG",
        lua.create_function(|lua, _: mlua::MultiValue| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if let Some((session_epoch, world_generation)) = model.wrath_scope {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::LfgLeave {
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "AcceptProposal",
        lua.create_function(|lua, arg: Option<u32>| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let proposal_id = arg.or_else(|| model.wrath_lfg.proposal.as_ref().map(|p| p.proposal_id));
            if let (Some((session_epoch, world_generation)), Some(proposal_id)) =
                (model.wrath_scope, proposal_id)
            {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::LfgProposalResult {
                            proposal_id,
                            accept: true,
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
    )?;
    lua.globals().set(
        "RejectProposal",
        lua.create_function(|lua, arg: Option<u32>| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let proposal_id = arg.or_else(|| model.wrath_lfg.proposal.as_ref().map(|p| p.proposal_id));
            if let (Some((session_epoch, world_generation)), Some(proposal_id)) =
                (model.wrath_scope, proposal_id)
            {
                if model.wrath_scoped_commands.len() < 16 {
                    model.wrath_scoped_commands.push_back(
                        super::model::WrathScopedCommand::LfgProposalResult {
                            proposal_id,
                            accept: false,
                            session_epoch,
                            world_generation,
                        },
                    );
                }
            }
            Ok(())
        })?,
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
    install_quest_poi(lua)?;

    Ok(())
}

/// The five QuestPOI bindings the 3.3.5a quest-log script surface registers
/// (`QuestLog_Scripts.cpp:1760-1764`):
/// `QuestPOIUpdateIcons` (`QuestLog_Scripts.cpp`, OFFSET `0x005E5740`),
/// `QuestPOIUpdateTexture` (`0x008E5250`), `QuestPOIGetQuestIDByIndex` (`0x005DFA80`),
/// `QuestPOIGetQuestIDByVisibleIndex` (`0x005E5750`) and `QuestPOIGetIconInfo` (`0x005E0590`),
/// in the preserved reference at pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
///
/// The visible-index and icon getters answer the reference's no-POI branches: a negative
/// `GetPOIQuestIDByVisibleIndex` and a failed `GetQuestPOIIcon` both return zero values. Each
/// `lua_isnumber` gate raises the reference's literal usage text, and numeric strings pass just
/// as they do through `0x6f34d0`.
///
/// `QuestPOIGetQuestIDByIndex` is a documented deviation for the empty leg. The reference keeps
/// a fixed POI array whose slots are zero-initialized (`CGQuestLog::InitializeGame`,
/// `QuestLog.cpp:153-164`), so an in-bounds index answers one value, quest id `0`, and only an
/// index outside that array or a negative quest id answers zero values. Project B has no POI
/// store at all, so every index answers zero values rather than reporting an id for a slot this
/// client never filled. No stock FrameXML calls this binding; it exists for addons. The preserved
/// tree is a `HoT` fork whose 100-slot POI array is a fork widening, not a stock 12340 bound;
/// Project B does not adopt it (its Wrath log model keeps the stock 25 slots).
///
/// `QuestPOIUpdateIcons`'s reference body calls `CGQuestLog::UpdatePOIIcons()` (`0x005E2EB0`),
/// which computes per-POI icon positions from blob data. There are no POIs or blobs here, so this
/// binding performs no work and returns 0. `QuestPOIUpdateTexture`'s reference body is plainly
/// `return 0;`, which this binding matches.
///
/// These are truthful empty answers, not a completed quest-blob feature.
/// `benilla_protocol_wrath::quest_poi` decodes `SMSG_QUEST_POI_QUERY_RESPONSE`, but no one sends
/// `CMSG_QUEST_POI_QUERY`, no owner stores the decoded list, nothing selects visible POIs over
/// the current map area, and `WorldMapBlobFrame:DrawQuestBlob` has no renderer. The next
/// prerequisites are therefore the request/selection ring (send the query from the 25-slot quest
/// log, keep the reply keyed by quest id, expose a visible-index view for the current map) and
/// the blob renderer that consumes its geometry. Until those exist these bindings must never
/// fabricate quest ids, log indices or coordinates.
///
/// Visible delta: before this binding existed the stock map aborted on a nil call at
/// `WorldMapFrame.lua:1540`; now `WorldMapFrame_UpdateQuests` (stock `WorldMapFrame.lua:1525`)
/// completes, sets `numQuests` to 0 and runs its hide loops. The map still lists no quests,
/// because `QuestMapUpdateAllQuests` answers 0 (`worldmap.rs`) and no POI data exists. The
/// separate `QuestPOIFrame` unknown-frame-type warning and the missing
/// `WorldMapBlobFrame:DrawQuestBlob` renderer are not fixed by this.
fn install_quest_poi(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set("QuestPOIUpdateIcons", lua.create_function(|_, ()| Ok(()))?)?;
    lua.globals().set(
        "QuestPOIUpdateTexture",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    lua.globals().set(
        "QuestPOIGetQuestIDByIndex",
        lua.create_function(|lua, index: Value| -> mlua::Result<MultiValue> {
            crate::script::binding_abi::number_arg(
                lua,
                index,
                "Usage: QuestPOIGetQuestIDByIndex(index)",
            )?;
            // Documented deviation: the reference's zero-initialized POI array answers quest id
            // 0 for an in-bounds index. Project B has no POI store at all, so every index answers
            // no values rather than reporting a slot this client never filled. No stock caller.
            Ok(MultiValue::new())
        })?,
    )?;
    lua.globals().set(
        "QuestPOIGetQuestIDByVisibleIndex",
        lua.create_function(|lua, index: Value| -> mlua::Result<MultiValue> {
            crate::script::binding_abi::number_arg(
                lua,
                index,
                "Usage: Script_QuestPOIGetQuestIDByVisibleIndex(index)",
            )?;
            // `GetPOIQuestIDByVisibleIndex` has no ring to read, so its answer is negative and
            // the reference returns no values instead of `(questId, GetQuestIndex(questId))`.
            Ok(MultiValue::new())
        })?,
    )?;
    lua.globals().set(
        "QuestPOIGetIconInfo",
        lua.create_function(|lua, quest_id: Value| -> mlua::Result<MultiValue> {
            crate::script::binding_abi::number_arg(
                lua,
                quest_id,
                "Usage: QuestPOIGetIconInfo(questID)",
            )?;
            Ok(MultiValue::new())
        })?,
    )?;
    Ok(())
}

/// One numeric argument to `MulticlassRequest`: absent or `nil` is `None`, a whole number (or
/// numeric string) in `u32` range is `Some`, and anything else is the binding's refusal (`Err`),
/// as the native callback's `ReadInteger` refuses rather than raises.
fn multiclass_arg(args: &MultiValue, index: usize) -> Result<Option<u32>, ()> {
    let check_f64 = |value: f64| -> Result<Option<u32>, ()> {
        if value.is_finite()
            && value >= 0.0
            && value <= f64::from(u32::MAX)
            && value.trunc() == value
        {
            Ok(Some(value as u32))
        } else {
            Err(())
        }
    };
    match args.get(index) {
        None | Some(Value::Nil) => Ok(None),
        Some(Value::Integer(value)) => u32::try_from(*value).map(Some).map_err(|_| ()),
        Some(Value::Number(value)) => check_f64(*value),
        Some(Value::String(s)) => {
            let text = s.to_str().map_err(|_| ())?;
            let trimmed = text.trim();
            if let Ok(int_val) = trimmed.parse::<u32>() {
                Ok(Some(int_val))
            } else if let Ok(float_val) = trimmed.parse::<f64>() {
                check_f64(float_val)
            } else {
                Err(())
            }
        }
        Some(_) => Err(()),
    }
}

/// The addon-facing WMC1 v3 frame: `"WMC1"`, version `3`, op `u16`, serial `u32`, status `u16`,
/// then the reply tail. [`super::UiScript::push_multiclass_reply`]'s payload already starts at the
/// `count u16` because `benilla_protocol_wrath::gameplay::accept_multiclass_reply` strips the
/// magic/version header and everything before the body, so the packet is the exact frame that
/// `WM_Multiclass.lua`'s `receive(hex)` / `multiclass_glue.lua`'s `receive(hex)` parse.
fn multiclass_packet_hex(op: u16, status: u16, serial: u32, payload: &[u8]) -> String {
    const MAGIC: u32 = 0x3143_4d57; // "WMC1", little endian.
    const VERSION: u16 = 3;
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut bytes = Vec::with_capacity(14 + payload.len());
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&op.to_le_bytes());
    bytes.extend_from_slice(&serial.to_le_bytes());
    bytes.extend_from_slice(&status.to_le_bytes());
    bytes.extend_from_slice(payload);

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// WMC1 v3 (`MulticlassRequest` / `MulticlassPoll`) bindings shared by the Wrath in-game
/// (`WM_Multiclass.lua`) and glue (`multiclass_glue.lua`) Lua environments.
///
/// The installed call is `MulticlassRequest(operation, serial, guid, choice, tokenLow, tokenHigh)`
/// and it fails unless the binding returns the number `1`. `operation`, `serial` and `guid` are
/// `u32`, `choice` is a class id in `0..=11`, and the token is a 64-bit value; the world addon
/// passes a talent id in the `guid` slot and its rank in `tokenLow`, while the glue addon passes
/// a character GUID and its preview token. The queue tuple's third slot is intentionally a `u64`:
/// the protocol's 28-byte body is `guid u32` then `choice u8` (plus three reserved bytes) and
/// `benilla_protocol_wrath::gameplay::encode_multiclass_request` writes that slot low-word-first,
/// so `character_guid | (choice as u64) << 32` reproduces `guid, choice, 0, 0, 0` byte for byte.
pub(super) fn install_multiclass(lua: &Lua) -> mlua::Result<()> {
    const MAX_MULTICLASS_QUEUE: usize = 16;

    lua.globals().set(
        "MulticlassRequest",
        lua.create_function(|lua, args: MultiValue| {
            // `ReadInteger` refuses malformed/out-of-range numbers; mirror that with `0` rather
            // than raising, so the addon's `~= 1` branch stays reachable.
            let Ok(op) = multiclass_arg(&args, 0) else {
                return Ok(0i64);
            };
            let Some(op) = op.filter(|op| (1..=5).contains(op)) else {
                return Ok(0i64);
            };
            let Ok(serial) = multiclass_arg(&args, 1) else {
                return Ok(0i64);
            };
            let Some(serial) = serial.filter(|serial| *serial != 0) else {
                return Ok(0i64);
            };
            let guid = match multiclass_arg(&args, 2) {
                Ok(guid) => guid.unwrap_or(0),
                Err(()) => return Ok(0i64),
            };
            let choice = match multiclass_arg(&args, 3) {
                Ok(choice) => choice.unwrap_or(0),
                Err(()) => return Ok(0i64),
            };
            let token_low = match multiclass_arg(&args, 4) {
                Ok(token) => token.unwrap_or(0),
                Err(()) => return Ok(0i64),
            };
            let token_high = match multiclass_arg(&args, 5) {
                Ok(token) => token.unwrap_or(0),
                Err(()) => return Ok(0i64),
            };
            // The native callback's admission rules: a known operation, a class id the wire can
            // carry, a token only on commit/learn, and a list that carries neither GUID nor class.
            if choice > 11
                || (op != 3 && op != 5 && (token_low != 0 || token_high != 0))
                || (op == 1 && (guid != 0 || choice != 0))
            {
                return Ok(0i64);
            }
            let mut model = lua.app_data_mut::<super::Model>().expect("model");
            // The caller owns the serial and correlates the reply itself; only track the next
            // value it is expected to use, never substitute it for the wire value.
            model.multiclass_next_serial = model
                .multiclass_next_serial
                .max(serial.saturating_add(1))
                .max(1);
            if model.multiclass_requests.len() >= MAX_MULTICLASS_QUEUE {
                model.multiclass_requests.pop_front();
            }
            model.multiclass_requests.push_back((
                op as u16,
                serial,
                u64::from(guid) | (u64::from(choice) << 32),
                token_low,
                token_high,
            ));
            Ok(1i64)
        })?,
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
            // One value: the bounded WMC1 v3 hex frame, or nil. The addon assigns a single
            // `MulticlassPoll()` result and stops on `nil`.
            Ok(reply.map(|(op, status, serial, payload)| {
                multiclass_packet_hex(op, status, serial, &payload)
            }))
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

    /// Drain queued `MulticlassRequest(operation, serial, guid, choice, tokenLow, tokenHigh)`
    /// entries as `(op, serial, character_guid, arg0, arg1)`: the caller's serial is preserved and
    /// `character_guid` packs the wire's `guid u32` in the low word and `choice u8` in the high
    /// word, so `encode_multiclass_request` emits `guid, choice, 0, 0, 0` verbatim. `arg0`/`arg1`
    /// are the token low/high words (the world addon's rank in the low word).
    pub fn take_multiclass_requests(&mut self) -> Vec<(u16, u32, u64, u32, u32)> {
        self.model_mut().multiclass_requests.drain(..).collect()
    }

    /// Enqueue a validated `SMSG_MULTICLASS_REPLY` (`0x51A`) frame `(op, status, serial, payload)`
    /// for `MulticlassPoll`; `payload` is the post-serial tail (`count u16` then the body) that
    /// `accept_multiclass_reply` returns.
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

    /// Seat the world scope the `OpenCalendar()` fetch is tagged with. A changed scope drops any
    /// request written under the old one; the feed itself clears the rune/calendar model on the
    /// same transfer.
    pub fn set_wrath_scope(&mut self, session_epoch: u64, world_generation: u64) {
        let mut model = self.model_mut();
        let scope = Some((session_epoch, world_generation));
        if model.wrath_scope != scope {
            model.wrath_scope = scope;
            model.calendar_fetches.clear();
            model.wrath_scoped_commands.clear();
        }
    }

    /// Replace the rune slots with a decoded `SMSG_RESYNC_RUNES` snapshot. Each tuple is
    /// `(rune_type, cooldown byte, GetTime seconds at decode)`.
    pub fn set_runes(&mut self, slots: &[(u8, u8, f64)]) {
        self.model_mut().runes = Some(
            slots
                .iter()
                .map(|&(rune_type, cooldown, received_at)| {
                    super::model::RuneSlotState {
                        rune_type,
                        cooldown,
                        received_at,
                    }
                })
                .collect(),
        );
    }

    /// `SMSG_CONVERT_RUNE`: the slot keeps its cooldown and gains the decoded type.
    pub fn apply_rune_convert(&mut self, rune: u8, rune_type: u8, received_at: f64) {
        let mut model = self.model_mut();
        if let Some(slot) = model
            .runes
            .as_mut()
            .and_then(|runes| runes.get_mut(usize::from(rune)))
        {
            slot.rune_type = rune_type;
            if slot.received_at <= 0.0 {
                slot.received_at = received_at;
            }
        }
    }

    /// `SMSG_ADD_RUNE_POWER`: every masked rune is fully recharged. The reference applies the
    /// same full-progress treatment (`RuneInfo.cpp:419-432`), represented here by the writer's
    /// 255 fraction at the decode clock.
    pub fn apply_rune_power(&mut self, mask: u32, received_at: f64) {
        let mut model = self.model_mut();
        let Some(runes) = model.runes.as_mut() else {
            return;
        };
        for (index, slot) in runes.iter_mut().enumerate() {
            if index < 32 && mask & (1u32 << index) != 0 {
                slot.cooldown = 255;
                slot.received_at = received_at;
            }
        }
    }

    /// Drop the rune slots on a scope change. The next `SMSG_RESYNC_RUNES` establishes the
    /// class's real layout again.
    pub fn clear_runes(&mut self) {
        self.model_mut().runes = None;
    }

    /// Replace the calendar with a decoded `SMSG_CALENDAR_SEND_CALENDAR` snapshot. `revision`
    /// advances once per reply, so a second fetch replaces the first without appending rows.
    pub fn set_calendar(
        &mut self,
        server_time: u32,
        zone_time: u32,
        pending_invites: usize,
        revision: u64,
    ) {
        self.model_mut().calendar = Some(super::model::CalendarState {
            server_time,
            zone_time,
            pending_invites,
            revision,
        });
    }

    /// Drop the calendar before the first reply and on a scope change.
    pub fn clear_calendar(&mut self) {
        self.model_mut().calendar = None;
    }

    /// Clear every scoped rune/calendar/LFG/vehicle fact, including the scope `OpenCalendar()` tags with.
    pub fn clear_wrath_scoped_ui(&mut self) {
        let mut model = self.model_mut();
        model.runes = None;
        model.calendar = None;
        model.wrath_scope = None;
        model.calendar_fetches.clear();
        model.wrath_lfg = super::model::WrathLfgState::default();
        model.wrath_scoped_commands.clear();
        model.vehicle_guid = None;
    }

    /// Drain `OpenCalendar()` requests as `(session_epoch, world_generation)`; the app sends each
    /// as `ClientCommand::OpenCalendar`, where the Wrath tap enforces the same scope.
    pub fn take_calendar_fetches(&mut self) -> Vec<(u64, u64)> {
        self.model_mut().calendar_fetches.drain(..).collect()
    }

    /// Synchronize server-authored 12340 Dungeon Finder state into the VM model while preserving
    /// locally selected roles/dungeons.
    #[allow(clippy::too_many_arguments)]
    pub fn set_wrath_lfg_server_state(
        &mut self,
        joined: bool,
        queued: bool,
        lfg_joined: bool,
        no_partial_clear: bool,
        achievements: bool,
        queued_dungeons: Vec<u32>,
        comment: Option<&str>,
        last_join_result: Option<(u32, u32)>,
        queue_status: Option<super::model::WrathLfgQueueStatusState>,
        proposal: Option<super::model::WrathLfgProposalState>,
    ) {
        let mut model = self.model_mut();
        model.wrath_lfg.joined = joined;
        model.wrath_lfg.queued = queued;
        model.wrath_lfg.lfg_joined = lfg_joined;
        model.wrath_lfg.no_partial_clear = no_partial_clear;
        model.wrath_lfg.achievements = achievements;
        model.wrath_lfg.queued_dungeons = queued_dungeons;
        if let Some(comment) = comment {
            model.wrath_lfg.comment = comment.to_owned();
        }
        model.wrath_lfg.last_join_result = last_join_result;
        model.wrath_lfg.queue_status = queue_status;
        model.wrath_lfg.proposal = proposal;
    }

    /// Set the replicated vehicle transport GUID for the local player.
    pub fn set_wrath_vehicle_guid(&mut self, guid: Option<u64>) {
        self.model_mut().vehicle_guid = guid;
    }

    /// Drain queued 12340 LFG and vehicle commands from the VM.
    pub fn take_wrath_scoped_commands(&mut self) -> Vec<super::model::WrathScopedCommand> {
        self.model_mut().wrath_scoped_commands.drain(..).collect()
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
        // No rune state has arrived: the reference answers no values, not a guessed class layout.
        assert_eq!(s.arity("GetRuneType(1)").unwrap(), 0);
        assert_eq!(s.arity("GetRuneCooldown(1)").unwrap(), 0);
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
        // Installed `WM_Multiclass.lua::request`: `(op, serial, talent, selected, rank, zero)`
        // and it fails unless the binding returns the number 1.
        let accepted: i64 = s
            .eval("return MulticlassRequest(4, 7, 0, 1, 0, 0)")
            .expect("MulticlassRequest in world VM");
        assert_eq!(accepted, 1, "the addon tests `~= 1`");
        assert_eq!(s.arity("MulticlassRequest(5, 8, 1234, 6, 3, 0)").unwrap(), 1);
        assert_eq!(
            s.take_multiclass_requests(),
            vec![
                // `character_guid` packs the caller's guid low-word-first and `selected`/choice
                // in the high word, which is the protocol's `guid u32, choice u8, reserved[3]`.
                (4, 7, 1u64 << 32, 0, 0),
                (5, 8, 1234 | (6u64 << 32), 3, 0),
            ]
        );

        // No reply queued: exactly one nil, not zero values and not the old four-value tuple.
        assert_eq!(s.arity("MulticlassPoll()").unwrap(), 1);
        assert_eq!(
            s.eval::<Option<String>>("return MulticlassPoll()").unwrap(),
            None
        );

        // `push_multiclass_reply`'s payload is what `accept_multiclass_reply` returns after the
        // status/serial: `count u16` then the body. Here op 4, serial 7, status 0, count 1. A
        // reply for another serial stays queued; the argument selects only the matching one.
        s.push_multiclass_reply(4, 8, 8, Vec::new());
        s.push_multiclass_reply(4, 0, 7, vec![1, 0]);
        let values: mlua::MultiValue = s
            .eval("return MulticlassPoll(7)")
            .expect("MulticlassPoll in world VM");
        assert_eq!(values.len(), 1, "one hex string, never the old four values");
        let packet = values[0]
            .as_string()
            .expect("the poll value is a string")
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(packet, "574d4331030004000700000000000100");

        // Decode the frame the way the installed `receive(hex)` does: magic, version, op,
        // serial/id, status/result, count, then the payload.
        let (magic, version, op, serial, status, count, tail): (
            u32,
            u16,
            u16,
            u32,
            u16,
            u16,
            String,
        ) = s
            .eval(&format!(
                r#"
                local h = "{packet}"
                local function uint(n)
                    local value, scale = 0, 1
                    for _ = 1, n do
                        value = value + tonumber(h:sub(1, 2), 16) * scale
                        scale = scale * 256
                        h = h:sub(3)
                    end
                    return value
                end
                local magic, version, op, serial, status, count =
                    uint(4), uint(2), uint(2), uint(4), uint(2), uint(2)
                return magic, version, op, serial, status, count, h
                "#
            ))
            .expect("Lua decode of the WMC1 packet");
        assert_eq!(
            (magic, version, op, serial, status, count),
            (826_494_295, 3, 4, 7, 0, 1)
        );
        assert_eq!(tail, "");

        // The non-zero status path is encoded in the same field order.
        s.push_multiclass_reply(5, 8, 9, vec![0, 0]);
        let packet: Option<String> = s.eval("return MulticlassPoll(9)").unwrap();
        assert_eq!(
            packet.expect("one WMC1 packet"),
            "574d4331030005000900000008000000"
        );

        // The glue VM (`multiclass_glue.lua::request`) uses the same six arguments and serial
        // correlation: guid, choice, then the preview low/high token.
        let mut glue = UiScript::glue_with_target(UiTarget::Wrath335).unwrap();
        let accepted: i64 = glue
            .eval("return MulticlassRequest(2, 21, 42, 6, 0, 0)")
            .expect("MulticlassRequest in glue VM");
        assert_eq!(accepted, 1);
        assert_eq!(
            glue.eval::<i64>("return MulticlassRequest(3, 22, 42, 6, 0x11223344, 0x55667788)")
                .unwrap(),
            1
        );
        // `request(1)` on open/refresh: list carries neither GUID nor class.
        assert_eq!(
            glue.eval::<i64>("return MulticlassRequest(1, 23, 0, 0, 0, 0)")
                .unwrap(),
            1
        );
        assert_eq!(
            glue.take_multiclass_requests(),
            vec![
                (2, 21, 42 | (6u64 << 32), 0, 0),
                (3, 22, 42 | (6u64 << 32), 0x1122_3344, 0x5566_7788),
                (1, 23, 0, 0, 0),
            ]
        );
    }

    #[test]
    fn wrath_multiclass_request_refuses_and_empty_poll_stays_nil() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        for call in [
            "MulticlassRequest(0, 0, 0, 0, 0, 0)",  // the native startup probe
            "MulticlassRequest(9, 1, 0, 1, 0, 0)",  // operation outside 1..=5
            "MulticlassRequest(4, 0, 0, 1, 0, 0)",  // no caller serial to correlate
            "MulticlassRequest(4, 1, 0, 12, 0, 0)", // class id beyond the wire mask
            "MulticlassRequest(2, 1, 42, 6, 7, 0)", // preview carries no token
            "MulticlassRequest(1, 1, 1, 0, 0, 0)",  // list carries no guid
            "MulticlassRequest('x', 1, 0, 1, 0, 0)", // non-number is a refusal, not a raise
            "MulticlassRequest(4, 1, 0, 1, 0, -1)", // negative token
        ] {
            assert_eq!(
                s.eval::<i64>(&format!("return {call}")).unwrap(),
                0,
                "{call}"
            );
        }
        assert!(s.take_multiclass_requests().is_empty());

        // Omitted optional arguments are tolerated and extra arguments are ignored, as Lua allows.
        assert_eq!(s.eval::<i64>("return MulticlassRequest(4, 2)").unwrap(), 1);
        assert_eq!(
            s.eval::<i64>("return MulticlassRequest(4, 3, 0, 1, 0, 0, 99)")
                .unwrap(),
            1
        );
        assert_eq!(
            s.take_multiclass_requests(),
            vec![(4, 2, 0, 0, 0), (4, 3, 1u64 << 32, 0, 0)]
        );

        // A transition clears pending replies as before; the next poll is still one nil.
        s.push_multiclass_reply(4, 0, 2, vec![0, 0]);
        s.clear_multiclass_state();
        assert_eq!(s.arity("MulticlassPoll()").unwrap(), 1);
        assert_eq!(
            s.eval::<Option<String>>("return MulticlassPoll()").unwrap(),
            None
        );
        assert!(s.take_multiclass_requests().is_empty());
    }

    /// A six-rune death knight: resync establishes the server's own type order; a convert only
    /// changes the type, rune power recharges the masked slots, and no state yet is no values.
    #[test]
    fn wrath_rune_natives_read_resync_convert_and_power() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        assert_eq!(s.arity("GetRuneType(1)").unwrap(), 0);
        assert_eq!(s.arity("GetRuneCooldown(1)").unwrap(), 0);

        s.set_now(100.0);
        // Deliberately non-standard order: the decoded type is authoritative, not slot groups.
        s.set_runes(&[
            (0, 0, 100.0),
            (0, 255, 100.0),
            (1, 128, 100.0),
            (2, 64, 100.0),
            (3, 0, 100.0),
            (2, 255, 100.0),
        ]);
        assert_eq!(s.eval::<f64>("return GetRuneType(1)").unwrap(), 1.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(2)").unwrap(), 1.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(3)").unwrap(), 2.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(5)").unwrap(), 4.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(6)").unwrap(), 3.0);

        let (start, duration, ready): (f64, f64, bool) =
            s.eval("return GetRuneCooldown(1)").unwrap();
        assert!((start - 100.0).abs() < 1e-9, "fresh rune starts now: {start}");
        assert_eq!(duration, 10.0);
        assert!(!ready, "a just-used rune is not ready");
        let (start, duration, ready): (f64, f64, bool) =
            s.eval("return GetRuneCooldown(2)").unwrap();
        assert!((start - 90.0).abs() < 1e-9, "full progress is ready: {start}");
        assert_eq!(duration, 10.0);
        assert!(ready);

        // `SMSG_CONVERT_RUNE`: the slot keeps its cooldown and takes the new type.
        s.set_now(110.0);
        s.apply_rune_convert(0, 3, 110.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(1)").unwrap(), 4.0);
        let (start, _, ready): (f64, f64, bool) = s.eval("return GetRuneCooldown(1)").unwrap();
        assert!((start - 100.0).abs() < 1e-9, "convert must not reset the cooldown");
        assert!(ready, "a fully elapsed ten-second recharge is ready");

        // `SMSG_ADD_RUNE_POWER`: slot 3 (index 2) becomes ready at the decode clock.
        s.apply_rune_power(1 << 2, 110.0);
        let (start, duration, ready): (f64, f64, bool) =
            s.eval("return GetRuneCooldown(3)").unwrap();
        assert!((start - 100.0).abs() < 1e-9);
        assert_eq!(duration, 10.0);
        assert!(ready);

        // The reference's invalid slot raises; an out-of-range type read is empty, not a guess.
        assert!(s.run("GetRuneCooldown(9)").is_err());
        assert_eq!(s.arity("GetRuneType(9)").unwrap(), 0);
    }

    /// Calendar date and pending-invite count come from the decoded reply and stay zero/empty
    /// before it. A second reply replaces the first date/count rather than appending.
    #[test]
    fn wrath_calendar_natives_read_the_decoded_snapshot() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        assert_eq!(s.arity("CalendarGetDate()").unwrap(), 4);
        assert_eq!(
            s.eval::<(i64, i64, i64, i64)>("return CalendarGetDate()")
                .unwrap(),
            (0, 0, 0, 0)
        );
        assert_eq!(
            s.eval::<i64>("return CalendarGetNumPendingInvites()").unwrap(),
            0
        );

        // Friday 2026-11-06 14:30, `WowTime::WowEncodeTime` bit layout.
        s.set_calendar(1_700_000_000, 0x1AA1_6B9E, 2, 1);
        assert_eq!(
            s.eval::<(i64, i64, i64, i64)>("return CalendarGetDate()")
                .unwrap(),
            (6, 11, 6, 2026)
        );
        assert_eq!(
            s.eval::<i64>("return CalendarGetNumPendingInvites()").unwrap(),
            2
        );

        // A later fetch replaces the snapshot: the count is not added to the previous one.
        s.set_calendar(1_700_100_000, 0x1AA1_6B9E, 0, 2);
        assert_eq!(
            s.eval::<i64>("return CalendarGetNumPendingInvites()").unwrap(),
            0
        );

        s.clear_calendar();
        assert_eq!(
            s.eval::<(i64, i64, i64, i64)>("return CalendarGetDate()")
                .unwrap(),
            (0, 0, 0, 0)
        );
    }

    /// `OpenCalendar()` tags the fetch with the scope the feed pushed; without a live scope the
    /// stock call is a no-op, and clearing the scope drops an already queued request.
    #[test]
    fn wrath_open_calendar_tags_its_fetch_with_the_world_scope() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.run("OpenCalendar()").unwrap();
        assert!(s.take_calendar_fetches().is_empty());

        s.set_wrath_scope(7, 9);
        s.run("OpenCalendar()").unwrap();
        assert_eq!(s.take_calendar_fetches(), vec![(7, 9)]);
        assert!(s.take_calendar_fetches().is_empty(), "one fetch per call");

        s.run("OpenCalendar()").unwrap();
        s.clear_wrath_scoped_ui();
        assert!(
            s.take_calendar_fetches().is_empty(),
            "a transfer drops the old scope's fetch"
        );
    }

    /// The stock quest list (`WorldMapFrame.lua:1540`, `:1547`; `WatchFrame.lua:1308`) treats the
    /// QuestPOI getters' zero values as "no POIs": the repaint calls return nothing and the
    /// visible-index loop never sees an index to show. Fabricated ids would make the map try to
    /// paint quests that no request/selection/renderer stage can supply.
    #[test]
    fn wrath_quest_poi_natives_answer_empty_without_a_poi_ring() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        assert_eq!(s.arity("QuestPOIUpdateIcons()").unwrap(), 0);
        assert_eq!(s.arity("QuestPOIUpdateIcons(1, 'x')").unwrap(), 0);
        assert_eq!(s.arity("QuestPOIUpdateTexture()").unwrap(), 0);
        assert_eq!(s.arity("QuestPOIUpdateTexture(1, 'x')").unwrap(), 0);
        // `ByIndex` is the documented deviation: the reference's zero-initialized POI array
        // answers quest id 0 for an in-bounds slot, while Project B has no POI store at all and
        // therefore answers no values. No stock FrameXML calls it.
        assert_eq!(s.arity("QuestPOIGetQuestIDByIndex(1)").unwrap(), 0);
        assert_eq!(s.arity("QuestPOIGetQuestIDByVisibleIndex(1)").unwrap(), 0);
        assert_eq!(s.arity("QuestPOIGetIconInfo(1)").unwrap(), 0);

        // `WorldMapFrame_UpdateQuests`'s guard (`WorldMapFrame.lua:1548`) must see nil, not a
        // synthetic index or quest id.
        s.run(
            r#"
            for i = 1, 3 do
                local questId, questLogIndex = QuestPOIGetQuestIDByVisibleIndex(i)
                if questLogIndex then error("no POI ring may report a quest-log index") end
                if questId then error("no POI ring may report a quest id") end
            end
            "#,
        )
        .unwrap();
    }

    /// Each getter's `lua_isnumber` gate raises the reference's literal usage text for nil, a
    /// missing argument, a table, a function and a non-numeric string; a numeric string passes
    /// through `0x6f34d0` and then reaches the same empty no-POI answer.
    #[test]
    fn wrath_quest_poi_getters_gate_on_lua_isnumber() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        for (name, usage) in [
            ("QuestPOIGetQuestIDByIndex", "Usage: QuestPOIGetQuestIDByIndex(index)"),
            (
                "QuestPOIGetQuestIDByVisibleIndex",
                "Usage: Script_QuestPOIGetQuestIDByVisibleIndex(index)",
            ),
            ("QuestPOIGetIconInfo", "Usage: QuestPOIGetIconInfo(questID)"),
        ] {
            for call in [
                format!("{name}()"),
                format!("{name}(nil)"),
                format!("{name}({{}})"),
                format!("{name}(function() end)"),
                format!("{name}('not a number')"),
            ] {
                let err = s.run(&call).expect_err(&call).to_string();
                // The VM prefixes `luaL_error`'s position like the reference; the literal usage
                // text must still be exactly what follows it.
                assert!(
                    err.ends_with(usage),
                    "{call}: expected the exact {usage:?}, got {err}"
                );
            }
            for call in [
                format!("{name}(1)"),
                format!("{name}('2')"),
                format!("{name}(1.9)"),
            ] {
                assert_eq!(s.arity(&call).unwrap(), 0, "{call}");
            }
        }
    }

    #[test]
    fn wrath_dual_spec_talents_and_glyph_sockets_read_groups() {
        use crate::script::talent::{
            TalentTabView, TalentView, WrathTalentGroupView, WrathTalentUiState,
        };

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_wrath_talents(WrathTalentUiState {
            active_group: 2,
            groups: vec![
                WrathTalentGroupView {
                    tabs: vec![TalentTabView {
                        name: "Arms".into(),
                        texture: Some("Ability_Warrior_SavageBlow".into()),
                        points_spent: 5,
                        background: "WarriorArms".into(),
                    }],
                    talents: vec![vec![TalentView {
                        talent_id: 101,
                        name: "Heroic Strike".into(),
                        texture: Some("Ability_Rogue_Ambush".into()),
                        tier: 1,
                        column: 1,
                        rank: 2,
                        max_rank: 3,
                        exceptional: false,
                        meets_prereq: true,
                        prereqs: Vec::new(),
                        display_spell: 284,
                        next_spell: 285,
                        reqs: Vec::new(),
                        learnable: true,
                    }]],
                    unspent_points: 4,
                    glyphs: [21, 22, 0, 24, 0, 0],
                },
                WrathTalentGroupView {
                    tabs: vec![TalentTabView {
                        name: "Protection".into(),
                        texture: Some("Ability_Warrior_DefensiveStance".into()),
                        points_spent: 11,
                        background: "WarriorProtection".into(),
                    }],
                    talents: vec![vec![TalentView {
                        talent_id: 202,
                        name: "Shield Specialization".into(),
                        texture: Some("INV_Shield_06".into()),
                        tier: 1,
                        column: 2,
                        rank: 5,
                        max_rank: 5,
                        exceptional: false,
                        meets_prereq: true,
                        prereqs: Vec::new(),
                        display_spell: 12298,
                        next_spell: 0,
                        reqs: Vec::new(),
                        learnable: false,
                    }]],
                    unspent_points: 9,
                    glyphs: [31, 0, 33, 0, 35, 36],
                },
            ],
            pet_unspent_points: 3,
            pet_talents: Vec::new(),
            pet_tabs: Vec::new(),
            pet_talent_views: Vec::new(),
        });

        assert_eq!(s.eval::<i64>("return GetNumTalentGroups()").unwrap(), 2);
        assert_eq!(s.eval::<i64>("return GetActiveTalentGroup()").unwrap(), 2);
        assert_eq!(
            s.eval::<i64>("return GetUnspentTalentPoints(false, false, 1)").unwrap(),
            4
        );
        assert_eq!(
            s.eval::<i64>("return GetUnspentTalentPoints(false, false, 2)").unwrap(),
            9
        );
        assert_eq!(
            s.eval::<i64>("return GetUnspentTalentPoints(false, true, 1)").unwrap(),
            3
        );

        let (tab1_name, _, tab1_spent): (String, String, u32) =
            s.eval("return GetTalentTabInfo(1, false, false, 1)").unwrap();
        assert_eq!(tab1_name, "Arms");
        assert_eq!(tab1_spent, 5);

        let (tab2_name, _, tab2_spent): (String, String, u32) =
            s.eval("return GetTalentTabInfo(1, false, false, 2)").unwrap();
        assert_eq!(tab2_name, "Protection");
        assert_eq!(tab2_spent, 11);

        let (enabled, glyph_type, glyph_id): (bool, i64, Option<i64>) =
            s.eval("return GetGlyphSocketInfo(1, 1)").unwrap();
        assert!(enabled);
        assert_eq!(glyph_type, 1);
        assert_eq!(glyph_id, Some(21));

        let (enabled, glyph_type, glyph_id): (bool, i64, Option<i64>) =
            s.eval("return GetGlyphSocketInfo(2, 2)").unwrap();
        assert!(enabled);
        assert_eq!(glyph_type, 2);
        assert_eq!(glyph_id, None);
    }

    #[test]
    fn wrath_lfg_and_vehicle_natives_read_server_state_and_emit_scoped_commands() {
        use crate::script::model::{
            WrathLfgProposalMemberState, WrathLfgProposalState, WrathLfgQueueStatusState,
            WrathScopedCommand,
        };

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_wrath_scope(3, 5);
        s.set_wrath_vehicle_guid(Some(0xF130_0001_0000_0042));
        assert!(s.eval::<bool>("return CanExitVehicle()").unwrap());

        s.set_wrath_lfg_server_state(
            true,
            true,
            true,
            false,
            true,
            vec![262],
            Some("Heroic daily"),
            None,
            Some(WrathLfgQueueStatusState {
                dungeon_id: 262,
                wait_time_avg: 45,
                wait_time: 40,
                wait_time_tank: 10,
                wait_time_healer: 20,
                wait_time_dps: 60,
                tanks_needed: 0,
                healers_needed: 1,
                dps_needed: 2,
                queued_time: 12,
            }),
            Some(WrathLfgProposalState {
                dungeon_id: 262,
                state: 1,
                proposal_id: 77,
                silent: false,
                completed_mask: 0b101,
                players: vec![WrathLfgProposalMemberState {
                    role: 0x02 | 0x01,
                    is_local: true,
                    in_dungeon: false,
                    in_same_group: true,
                    responded: true,
                    accepted: true,
                }],
            }),
        );

        let (in_party, joined, queued, _no_partial, achievements, comment, slots): (
            bool,
            bool,
            bool,
            bool,
            bool,
            String,
            i64,
        ) = s.eval("return GetLFGInfoServer()").unwrap();
        assert!(in_party && joined && queued && achievements);
        assert_eq!(comment, "Heroic daily");
        assert_eq!(slots, 1);

        let (has_stats, _, tanks, healers, dps): (bool, i64, i64, i64, i64) =
            s.eval("return GetLFGQueueStats()").unwrap();
        assert!(has_stats);
        assert_eq!((tanks, healers, dps), (0, 1, 2));

        let (exists, prop_id, _, _, _, _, role, responded, _, completed, members, is_leader): (
            bool,
            i64,
            i64,
            i64,
            String,
            String,
            String,
            bool,
            i64,
            i64,
            i64,
            bool,
        ) = s.eval("return GetLFGProposal()").unwrap();
        assert!(exists);
        assert_eq!(prop_id, 77);
        assert_eq!(role, "TANK");
        assert!(responded && is_leader);
        assert_eq!((completed, members), (2, 1));

        s.run(
            r#"
            SetLFGRoles(false, true, false, true)
            SetLFGComment("Need emblems")
            ClearAllLFGDungeons()
            SetLFGDungeon(1, 262)
            JoinLFG()
            AcceptProposal()
            VehicleNextSeat()
            UnitSwitchToVehicleSeat("player", 2)
            VehicleExit()
            "#,
        )
        .unwrap();

        let cmds = s.take_wrath_scoped_commands();
        assert_eq!(cmds.len(), 7);
        assert!(matches!(
            cmds[0],
            WrathScopedCommand::LfgSetRoles {
                roles: 0x0A,
                session_epoch: 3,
                world_generation: 5
            }
        ));
        assert!(matches!(
            &cmds[2],
            WrathScopedCommand::LfgJoin {
                slots,
                roles: 0x0A,
                session_epoch: 3,
                world_generation: 5,
                ..
            } if slots == &[0x0100_0106]
        ));
        assert!(matches!(
            cmds[3],
            WrathScopedCommand::LfgProposalResult {
                proposal_id: 77,
                accept: true,
                session_epoch: 3,
                world_generation: 5
            }
        ));
        assert!(matches!(
            cmds[5],
            WrathScopedCommand::VehicleSwitchSeat {
                vehicle: 0xF130_0001_0000_0042,
                seat: 2,
                session_epoch: 3,
                world_generation: 5
            }
        ));

        s.clear_wrath_scoped_ui();
        assert!(!s.eval::<bool>("return CanExitVehicle()").unwrap());
        assert!(!s.eval::<bool>("return IsPartyLFG()").unwrap());
    }

    #[test]
    fn wrath_multiclass_request_matches_installed_addon_contract() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        // Op 1 (list), Op 2 (preview), Op 3 (commit with token), Op 4 (active), Op 5 (learn rank).
        assert_eq!(s.eval::<i64>("return MulticlassRequest(1, 10, 0, 0, 0, 0)").unwrap(), 1);
        assert_eq!(s.eval::<i64>("return MulticlassRequest(2, 11, 42, 8, 0, 0)").unwrap(), 1);
        assert_eq!(
            s.eval::<i64>("return MulticlassRequest(3, 12, 42, 8, 0x11223344, 0x55667788)")
                .unwrap(),
            1
        );
        assert_eq!(s.eval::<i64>("return MulticlassRequest(5, 13, 301, 2, 3, 0)").unwrap(), 1);

        let reqs = s.take_multiclass_requests();
        assert_eq!(
            reqs,
            vec![
                (1, 10, 0, 0, 0),
                (2, 11, 42 | (8 << 32), 0, 0),
                (3, 12, 42 | (8 << 32), 0x1122_3344, 0x5566_7788),
                (5, 13, 301 | (2 << 32), 3, 0),
            ]
        );

        s.push_multiclass_reply(1, 0, 10, vec![0x01, 0x00, 0x08]);
        let hex: String = s.eval("return MulticlassPoll(10)").unwrap();
        assert!(hex.starts_with("574d4331030001000a0000000000010008"));
        assert!(s.eval::<Option<String>>("return MulticlassPoll(10)").unwrap().is_none());
    }

    #[test]
    fn wm_questie_compat_suppresses_only_900000_plus_errors() {
        use crate::script::quest_log::{QuestRecordView, QuestSlotView, QUEST_LOG_SLOTS};

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_quest_log_player(true);
        s.seed_quest_record(
            783,
            QuestRecordView {
                title: "A Threat Within".into(),
                level: 1,
                suggested_players: 0,
                daily: false,
                zone_or_sort: 12,
                header: "Northshire Valley".into(),
                ..Default::default()
            },
        );
        s.seed_quest_record(
            900001,
            QuestRecordView {
                title: "WM Custom Directive".into(),
                level: 1,
                suggested_players: 2,
                daily: true,
                zone_or_sort: 12,
                header: "Northshire Valley".into(),
                ..Default::default()
            },
        );
        let mut slots = [QuestSlotView::default(); QUEST_LOG_SLOTS];
        slots[0] = QuestSlotView {
            quest_id: 783,
            ..Default::default()
        };
        slots[1] = QuestSlotView {
            quest_id: 900001,
            ..Default::default()
        };
        s.quest_log_changed(slots);

        // `GetQuestLogTitle` returns `(title, level, questTag, suggestedGroup, isHeader, isCollapsed, isComplete, isDaily, questID)`
        // at indices 1..9 in Wrath335, allowing `WMQuestieCompat.lua` to suppress only `questID >= 900000`.
        let (stock_id, custom_id, suppressed_custom, kept_stock): (i64, i64, bool, bool) = s
            .eval(
                r#"
                local _, _, _, _, _, _, _, _, q1 = GetQuestLogTitle(2)
                local _, _, _, _, _, _, _, _, q2 = GetQuestLogTitle(3)
                local function isSuppressed(msg)
                    local id = tonumber(string.match(msg, "quest (%d+)"))
                    return id ~= nil and id >= 900000
                end
                return q1, q2, isSuppressed("Questie error on quest " .. q2), not isSuppressed("Questie error on quest " .. q1)
                "#,
            )
            .unwrap();
        assert_eq!(stock_id, 783);
        assert_eq!(custom_id, 900001);
        assert!(suppressed_custom);
        assert!(kept_stock);
    }

    #[test]
    fn bagnon_item_slot_info_reads_wrath_seven_tuple() {
        use crate::script::container::{ContainerSlot, ContainerState};
        use crate::script::item_stats::ItemTemplateView;
        use std::collections::HashMap;

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_item_template(
            6948,
            ItemTemplateView {
                name: "Hearthstone".into(),
                quality: 1,
                inventory_type: 0,
                flags: 0x04, // ITEM_FLAG_HAS_LOOT
                ..Default::default()
            },
        );
        let mut slots = HashMap::new();
        slots.insert(
            1,
            ContainerSlot {
                item_id: 6948,
                guid: 0x4000_0000_0000_0001,
                texture: Some("Interface\\Icons\\INV_Misc_Rune_01".into()),
                count: 1,
                quality: Some(1),
                inventory_type: 1,
                readable: true,
                link: Some("|cffffffff|Hitem:6948:0:0:0:0:0:0:0:1|h[Hearthstone]|h|r".into()),
                ..Default::default()
            },
        );
        s.set_container(
            0,
            Some(ContainerState {
                name: None,
                num_slots: 16,
                slots,
            }),
        );

        let (tex, count, locked, quality, readable, lootable, link): (
            String,
            i64,
            Option<i64>,
            i64,
            Option<i64>,
            Option<i64>,
            String,
        ) = s.eval("return GetContainerItemInfo(0, 1)").unwrap();
        assert_eq!(tex, "Interface\\Icons\\INV_Misc_Rune_01");
        assert_eq!(count, 1);
        assert_eq!(locked, None);
        assert_eq!(quality, 1);
        assert_eq!(readable, Some(1));
        assert_eq!(lootable, Some(1));
        assert_eq!(link, "|cffffffff|Hitem:6948:0:0:0:0:0:0:0:1|h[Hearthstone]|h|r");
    }

    #[test]
    fn lfg_join_sends_from_the_lfd_frame() {
        use crate::script::model::WrathScopedCommand;

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_wrath_scope(11, 22);
        s.run(
            r#"
            SetLFGRoles(true, true, false, true)
            SetLFGComment("Random heroic")
            ClearAllLFGDungeons()
            SetLFGDungeon(1, 262)
            JoinLFG()
            "#,
        )
        .unwrap();
        let cmds = s.take_wrath_scoped_commands();
        assert!(cmds.iter().any(|c| matches!(
            c,
            WrathScopedCommand::LfgJoin {
                roles: 0x0B,
                slots,
                comment,
                session_epoch: 11,
                world_generation: 22,
                ..
            } if slots == &[0x0100_0106] && comment == "Random heroic"
        )));
    }

    #[test]
    fn calendar_get_calendar_refreshes_native_date_and_invites() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_wrath_scope(4, 8);
        s.run("OpenCalendar()").unwrap();
        assert_eq!(s.take_calendar_fetches(), vec![(4, 8)]);
        s.set_calendar(1_700_000_000, 0x1AA1_6B9E, 3, 1);
        assert_eq!(
            s.eval::<(i64, i64, i64, i64)>("return CalendarGetDate()")
                .unwrap(),
            (6, 11, 6, 2026)
        );
        assert_eq!(
            s.eval::<i64>("return CalendarGetNumPendingInvites()").unwrap(),
            3
        );
    }

    #[test]
    fn rune_frame_reads_resync_state() {
        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_now(50.0);
        s.set_runes(&[
            (0, 0, 50.0),
            (0, 255, 50.0),
            (1, 128, 50.0),
            (1, 255, 50.0),
            (2, 0, 50.0),
            (3, 255, 50.0),
        ]);
        assert_eq!(s.eval::<f64>("return GetRuneType(1)").unwrap(), 1.0);
        assert_eq!(s.eval::<f64>("return GetRuneType(6)").unwrap(), 4.0);
        let (_, _, ready1): (f64, f64, bool) = s.eval("return GetRuneCooldown(1)").unwrap();
        let (_, _, ready2): (f64, f64, bool) = s.eval("return GetRuneCooldown(2)").unwrap();
        assert!(!ready1);
        assert!(ready2);
    }

    #[test]
    fn vehicle_exit_uses_the_seat_transport() {
        use crate::script::model::WrathScopedCommand;

        let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        s.set_wrath_scope(9, 12);
        s.set_wrath_vehicle_guid(Some(0xF130_0002_0000_0099));
        assert!(s.eval::<bool>("return UnitInVehicle('player')").unwrap());
        s.run("VehicleExit()").unwrap();
        assert_eq!(
            s.take_wrath_scoped_commands(),
            vec![WrathScopedCommand::VehicleExit {
                session_epoch: 9,
                world_generation: 12,
            }]
        );
    }

    #[test]
    fn wrath_rune_calendar_vehicle_natives_read_decoded_state() {
        rune_frame_reads_resync_state();
        calendar_get_calendar_refreshes_native_date_and_invites();
        vehicle_exit_uses_the_seat_transport();
    }
}
