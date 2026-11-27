//! Wrath spell queries use the lent app host, never a separate catalog or simulated spell state.

use mlua::{Lua, MultiValue, Value};

use super::{host, spellbook, Model};

#[derive(Clone, Debug, PartialEq)]
pub struct SpellInfoView {
    pub name: String,
    pub rank: Option<String>,
    pub icon: Option<String>,
    pub cost: Option<i32>,
    pub channeled: bool,
    pub power_type: i32,
    pub cast_time_ms: u32,
    pub min_range: Option<f32>,
    pub max_range: Option<f32>,
}

/// A presentation interval from the native cast owner, in GetTime milliseconds. The engine
/// owns the identity and timeline; Lua bindings only shape the stock return tuple.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitCastView {
    pub name: String,
    pub rank: Option<String>,
    pub icon: Option<String>,
    pub start_ms: f64,
    pub end_ms: f64,
    pub trade_skill: bool,
    pub cast_id: Option<u64>,
    pub not_interruptible: Option<bool>,
}

fn spell_reference(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, bool)>> {
    let input = args.front().cloned().unwrap_or(Value::Nil);
    if let Some(book) = args.get(1).filter(|value| !matches!(value, Value::Nil)) {
        // Shared by GetSpellInfo, GetSpellLink and GetSpellTexture: the marshaller's verb stays
        // "GetSpellInfo" for all three, as in the base, so an error string names the shared
        // marshaller rather than the calling binding.
        match spellbook::spell_slot_args(input.clone(), book.clone(), "GetSpellInfo") {
            Ok((slot, book)) => {
                let model = lua.app_data_ref::<Model>().expect("model");
                return Ok(spellbook::book_slot(&model, slot, &book)
                    .map(|slot| (slot.spell_id, spellbook::is_pet_book(&book))));
            }
            Err(err) => {
                // `GetSlotFromLua` (`SpellBookFrame.cpp:1447-1487`) raises for a numeric value
                // outside the book. The one exception is the ID `GetActionInfo`
                // (`ActionBarFrame_Scripts.cpp:72-143`) answers for a spell with no displayed
                // book slot: a Wrath addon passes that ID and the book type straight back here.
                return match spellbook::action_bar_spell_reference(lua, input, book.clone())? {
                    Some(spell_id) => Ok(Some((spell_id, false))),
                    None => Err(err),
                };
            }
        }
    }
    if let Some(number) = lua.coerce_number(input.clone())? {
        return Ok(
            (number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX))
                .then_some((number as u32, false)),
        );
    }
    let Value::String(name) = input else {
        return Ok(None);
    };
    let name = name.to_str()?;
    let model = lua.app_data_ref::<Model>().expect("model");
    let player_slots = if model.spell_list.is_empty() {
        &model.spellbook.slots
    } else {
        &model.spell_list
    };
    Ok(
        spellbook::resolve_spell_in_list(player_slots, &model.pet_book, &name)
            .map(|spell| (spell.spell_id, spell.pet)),
    )
}

fn query(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, SpellInfoView)>> {
    let Some((id, _)) = spell_reference(lua, args)? else {
        return Ok(None);
    };
    let host_view = host::with_host(lua, |host| host.spell_info(id)).flatten();
    let view = host_view.or_else(|| {
        lua.app_data_ref::<Model>()
            .expect("model")
            .spell_info
            .as_ref()
            .and_then(|spells| spells.get(&id).cloned())
    });
    Ok(view.map(|view| (id, view)))
}

impl super::UiScript {
    /// Seat the client catalog's `GetSpellInfo` projection, keyed by spell id, for a harness with
    /// no app call host to lend. The live client leaves it unset: its lent host answers from the
    /// one catalog owner, so this is never a second catalog in the scripting engine.
    pub fn set_spell_info(
        &mut self,
        spells: std::sync::Arc<std::collections::HashMap<u32, SpellInfoView>>,
    ) {
        self.model_mut().spell_info = Some(spells);
    }
}

fn string(lua: &Lua, value: Option<&str>) -> mlua::Result<Value> {
    value.map_or(Ok(Value::Nil), |value| {
        lua.create_string(value).map(Value::String)
    })
}

fn unit_cast(lua: &Lua, token: Value, channel: bool) -> mlua::Result<MultiValue> {
    let usage = if channel {
        "Usage: UnitChannelInfo(unit)"
    } else {
        "Usage: UnitCastingInfo(unit)"
    };
    let token = super::binding_abi::string_arg(lua, token, usage)?;
    super::unit::check_unit_token_for(&Some(token.clone()), true)?;
    let guid = {
        let model = lua.app_data_ref::<Model>().expect("model");
        model.unit(&token).map(|unit| unit.guid)
    };
    let view =
        guid.and_then(|guid| host::with_host(lua, |host| host.unit_cast(guid, channel)).flatten());
    let Some(view) = view else {
        return Ok(MultiValue::from_vec(vec![Value::Nil]));
    };
    let name = string(lua, Some(&view.name))?;
    let mut values = vec![
        name.clone(),
        string(lua, Some(view.rank.as_deref().unwrap_or("")))?,
        name,
        string(lua, view.icon.as_deref())?,
        Value::Number(view.start_ms),
        Value::Number(view.end_ms),
        Value::Boolean(view.trade_skill),
    ];
    // The target CastingBarFrame uses slot eight for cast identity, but channel tuples omit it.
    if !channel {
        values.push(
            view.cast_id
                .map_or(Value::Nil, |id| Value::Number(id as f64)),
        );
    }
    values.push(view.not_interruptible.map_or(Value::Nil, Value::Boolean));
    Ok(MultiValue::from_vec(values))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(
        "GetSpellCooldown",
        lua.create_function(|lua, args: MultiValue| {
            let value = if let Some(book) = args.get(1).filter(|book| !matches!(book, Value::Nil)) {
                // Preserve the existing pushed-book and pet-book forms, including their errors.
                spellbook::book_cooldown(
                    lua,
                    (args.front().cloned().unwrap_or(Value::Nil), book.clone()),
                )?
            } else {
                let Some((id, pet)) = spell_reference(lua, &args)? else {
                    return Ok(MultiValue::from_vec(vec![Value::Nil]));
                };
                let Some(cooldown) =
                    host::with_host(lua, |host| host.spell_cooldown(id, pet)).flatten()
                else {
                    return Ok(MultiValue::from_vec(vec![Value::Nil]));
                };
                spellbook::cooldown_tuple(Some(cooldown), super::clock::now(lua))
            };
            Ok(MultiValue::from_vec(vec![
                Value::Number(value.0),
                Value::Number(value.1),
                Value::Integer(i64::from(value.2)),
            ]))
        })?,
    )?;
    globals.set(
        "UnitCastingInfo",
        lua.create_function(|lua, token: Value| unit_cast(lua, token, false))?,
    )?;
    globals.set(
        "UnitChannelInfo",
        lua.create_function(|lua, token: Value| unit_cast(lua, token, true))?,
    )?;
    globals.set(
        "IsSpellKnown",
        lua.create_function(|lua, (id, pet): (Value, Option<bool>)| {
            let id = lua
                .coerce_number(id)?
                .filter(|id| id.is_finite() && *id >= 1.0 && *id <= f64::from(u32::MAX))
                .ok_or_else(|| mlua::Error::runtime("Usage: IsSpellKnown(spellID [, isPet])"))?
                as u32;
            Ok(host::with_host(lua, |host| host.spell_known(id, pet.unwrap_or(false))).flatten())
        })?,
    )?;
    globals.set(
        "GetSpellInfo",
        lua.create_function(|lua, args: MultiValue| {
            let Some((_, view)) = query(lua, &args)? else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            Ok(MultiValue::from_vec(vec![
                string(lua, Some(&view.name))?,
                string(lua, Some(view.rank.as_deref().unwrap_or("")))?,
                string(lua, view.icon.as_deref())?,
                view.cost
                    .map_or(Value::Nil, |cost| Value::Integer(i64::from(cost))),
                Value::Boolean(view.channeled),
                Value::Integer(i64::from(view.power_type)),
                Value::Integer(i64::from(view.cast_time_ms)),
                view.min_range
                    .map_or(Value::Nil, |range| Value::Number(f64::from(range))),
                view.max_range
                    .map_or(Value::Nil, |range| Value::Number(f64::from(range))),
            ]))
        })?,
    )?;
    globals.set(
        "GetSpellTexture",
        lua.create_function(|lua, args: MultiValue| {
            string(
                lua,
                query(lua, &args)?
                    .as_ref()
                    .and_then(|(_, view)| view.icon.as_deref()),
            )
        })?,
    )?;
    // `GetSpellLink(spell)` (`Script_GetSpellLink`, `0x5408E0`): the spell link, then the trade link
    // `CGTradeSkillInfo::GetTradeSkillListLink(spellId)` (`0x5DB060`) writes for a profession
    // opener, else nil (`SpellBookFrame_Scripts.cpp:1185`). The same encoder answers
    // `GetTradeSkillListLink()` for the open window, so the spellbook's shift-click and the window's
    // link button post the same `|Htrade:` text.
    globals.set(
        "GetSpellLink",
        lua.create_function(|lua, args: MultiValue| {
            let Some((id, view)) = query(lua, &args)? else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            let link = format!("|cff71d5ff|Hspell:{id}|h[{}]|h|r", view.name);
            let trade = host::with_host(lua, |host| host.trade_skill_link_source(id))
                .flatten()
                .and_then(|source| super::tradeskill::encode_trade_skill_link(&source));
            Ok(MultiValue::from_vec(vec![
                string(lua, Some(&link))?,
                string(lua, trade.as_deref())?,
            ]))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{host, SpellInfoView};
    use crate::script::action::{ACTION_KIND_ITEM, ACTION_KIND_MACRO, ACTION_KIND_SPELL};
    use crate::script::{
        ActionSlot, CallHost, CallTail, ScriptCall, SpellSlotView, UiScript, UiTarget,
    };

    const BAR_SPELL: u32 = 3009;
    const BOOK_SPELL: u32 = 133;

    /// The host's spell catalog, answering only the IDs it knows and recording every query.
    struct SpellHost {
        known: Vec<u32>,
        cooldowns: Vec<(u32, (i64, u32, bool))>,
        asked: Vec<u32>,
    }

    impl CallHost for SpellHost {
        fn apply(&mut self, _script: &mut UiScript, _call: ScriptCall) -> Option<CallTail> {
            None
        }

        fn spell_info(&mut self, id: u32) -> Option<SpellInfoView> {
            self.asked.push(id);
            self.known.contains(&id).then(|| SpellInfoView {
                name: format!("Spell {id}"),
                rank: Some("Rank 1".into()),
                icon: Some(format!("Interface\\Icons\\Spell_{id}")),
                cost: None,
                channeled: false,
                power_type: 0,
                cast_time_ms: 0,
                min_range: None,
                max_range: None,
            })
        }

        fn spell_cooldown(&mut self, id: u32, pet: bool) -> Option<(i64, u32, bool)> {
            assert!(!pet, "the action-bar fallback is player-book only");
            self.cooldowns
                .iter()
                .find_map(|(spell, cooldown)| (*spell == id).then_some(*cooldown))
        }
    }

    fn slot(kind: u8, action: u32) -> ActionSlot {
        ActionSlot {
            kind,
            action,
            ..Default::default()
        }
    }

    fn wrath() -> UiScript {
        UiScript::with_target(UiTarget::Wrath335).unwrap()
    }

    /// The addon's `nativeInfo(key, book)` fallback: a spell on the bar but not in the displayed
    /// book answers its ID from `GetActionInfo`, and every Wrath spell query accepts that ID with
    /// the book type instead of raising `Invalid spell slot`.
    #[test]
    fn an_action_bar_spell_id_with_a_book_answers_the_hosts_spell() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BAR_SPELL)));
        let mut host = SpellHost {
            known: vec![BAR_SPELL],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<String>("return (GetSpellInfo(3009, BOOKTYPE_SPELL))")
                    .unwrap(),
                "Spell 3009"
            );
            let link = s
                .eval::<String>("return GetSpellLink(3009, BOOKTYPE_SPELL)")
                .unwrap();
            assert!(
                link.contains("spell:3009") && link.contains("Spell 3009"),
                "{link}"
            );
            assert_eq!(
                s.eval::<String>("return GetSpellTexture(3009, BOOKTYPE_SPELL)")
                    .unwrap(),
                "Interface\\Icons\\Spell_3009"
            );
            assert_eq!(
                s.eval::<(String, String)>("return GetSpellName(3009, BOOKTYPE_SPELL)")
                    .unwrap(),
                ("Spell 3009".into(), "Rank 1".into())
            );
            assert_eq!(
                s.eval::<(f64, f64, i64)>("return GetSpellCooldown(3009, BOOKTYPE_SPELL)")
                    .unwrap(),
                (0.0, 0.0, 1)
            );
        });
        assert!(host.asked.contains(&BAR_SPELL), "{:?}", host.asked);
    }

    /// A bar spell the host catalog does not know answers nil/no values, never a slot error.
    #[test]
    fn an_unknown_action_bar_spell_answers_nil_not_a_slot_error() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BAR_SPELL)));
        let mut host = SpellHost {
            known: Vec::new(),
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            assert!(s
                .eval::<bool>("return GetSpellInfo(3009, BOOKTYPE_SPELL) == nil")
                .unwrap());
            assert!(s
                .eval::<bool>("return GetSpellLink(3009, BOOKTYPE_SPELL) == nil")
                .unwrap());
            assert!(s
                .eval::<bool>("return GetSpellTexture(3009, BOOKTYPE_SPELL) == nil")
                .unwrap());
            assert!(s
                .eval::<bool>("return GetSpellName(3009, BOOKTYPE_SPELL) == nil")
                .unwrap());
            assert_eq!(
                s.eval::<(f64, f64, i64)>("return GetSpellCooldown(3009, BOOKTYPE_SPELL)")
                    .unwrap(),
                (0.0, 0.0, 1)
            );
        });
        assert_eq!(
            host.asked,
            vec![BAR_SPELL; 4],
            "the host's catalog is asked for the ID, not a slot; cooldown has its own owner"
        );
    }

    /// The same out-of-range number is still a genuine invalid slot when no action bar holds it.
    #[test]
    fn an_out_of_range_number_off_the_action_bar_still_raises() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BOOK_SPELL)));
        let mut host = SpellHost {
            known: vec![BAR_SPELL],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            for call in [
                "GetSpellInfo(3009, BOOKTYPE_SPELL)",
                "GetSpellLink(3009, BOOKTYPE_SPELL)",
                "GetSpellTexture(3009, BOOKTYPE_SPELL)",
                "GetSpellName(3009, BOOKTYPE_SPELL)",
                "GetSpellCooldown(3009, BOOKTYPE_SPELL)",
            ] {
                let err = s.run(call).unwrap_err().to_string();
                assert!(err.contains("Invalid spell slot"), "{call}: {err}");
            }
        });
        assert!(host.asked.is_empty(), "{:?}", host.asked);
    }

    /// A value inside the slot range still resolves through the book, even when it is also a
    /// spell ID the host knows.
    #[test]
    fn an_in_range_book_slot_still_reads_the_book_not_a_spell_id() {
        let mut s = wrath();
        s.set_spell_list(vec![SpellSlotView {
            spell_id: BOOK_SPELL,
            name: "Fireball".into(),
            ..Default::default()
        }]);
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BOOK_SPELL)));
        let mut host = SpellHost {
            known: vec![BOOK_SPELL],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<String>("return (GetSpellInfo(1, BOOKTYPE_SPELL))")
                    .unwrap(),
                "Spell 133"
            );
            assert_eq!(
                s.eval::<String>("return GetSpellTexture(1, BOOKTYPE_SPELL)")
                    .unwrap(),
                "Interface\\Icons\\Spell_133"
            );
            assert_eq!(
                s.eval::<String>("return (GetSpellName(1, BOOKTYPE_SPELL))")
                    .unwrap(),
                "Fireball",
                "GetSpellName reads the book slot, not the host catalog"
            );
        });
        assert_eq!(host.asked, vec![BOOK_SPELL, BOOK_SPELL]);
    }

    /// The guard is the slot kind, not any action-bar entry: an item or macro slot carrying the
    /// same number does not make it a spell ID.
    #[test]
    fn non_spell_action_slots_do_not_enable_the_id_fallback() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_ITEM, BAR_SPELL)));
        s.set_action(2, Some(slot(ACTION_KIND_MACRO, BAR_SPELL)));
        let mut host = SpellHost {
            known: vec![BAR_SPELL],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            for call in [
                "GetSpellInfo(3009, BOOKTYPE_SPELL)",
                "GetSpellLink(3009, BOOKTYPE_SPELL)",
                "GetSpellTexture(3009, BOOKTYPE_SPELL)",
                "GetSpellName(3009, BOOKTYPE_SPELL)",
                "GetSpellCooldown(3009, BOOKTYPE_SPELL)",
            ] {
                let err = s.run(call).unwrap_err().to_string();
                assert!(err.contains("Invalid spell slot"), "{call}: {err}");
            }
        });
        assert!(host.asked.is_empty(), "{:?}", host.asked);
    }

    /// `GetSlotFromLua` allows index 1023 (slot 1024); 1025 is the first out-of-range value and
    /// falls back only when it is on the bar.
    #[test]
    fn the_slot_range_boundary_is_1024() {
        let mut s = wrath();
        // Put both values on the bar so only the range check can distinguish them.
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, 1024)));
        s.set_action(2, Some(slot(ACTION_KIND_SPELL, 1025)));
        let mut host = SpellHost {
            known: vec![1024, 1025],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            // 1024 is an in-range empty slot: nil, not the host's spell 1024.
            assert!(s
                .eval::<bool>("return GetSpellInfo(1024, BOOKTYPE_SPELL) == nil")
                .unwrap());
            // 1025 is out of range and on the bar: the host's spell.
            assert_eq!(
                s.eval::<String>("return (GetSpellInfo(1025, BOOKTYPE_SPELL))")
                    .unwrap(),
                "Spell 1025"
            );
        });
        // The same 1025 off the bar is a genuine invalid slot.
        s.set_action(2, None);
        let err = s
            .run("GetSpellInfo(1025, BOOKTYPE_SPELL)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Invalid spell slot"), "{err}");
    }

    /// The raw-ID fallback is player-book only: `GetActionInfo` never answers a pet-book ID that
    /// way, so a player bar spell with `"pet"` must still be a genuine invalid slot.
    #[test]
    fn the_pet_book_does_not_enable_the_player_id_fallback() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BAR_SPELL)));
        let mut host = SpellHost {
            known: vec![BAR_SPELL],
            cooldowns: vec![(BAR_SPELL, (100_000, 5_000, true))],
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            for call in [
                "GetSpellInfo(3009, BOOKTYPE_PET)",
                "GetSpellLink(3009, BOOKTYPE_PET)",
                "GetSpellTexture(3009, BOOKTYPE_PET)",
                "GetSpellName(3009, BOOKTYPE_PET)",
                "GetSpellCooldown(3009, BOOKTYPE_PET)",
            ] {
                let err = s.run(call).unwrap_err().to_string();
                assert!(err.contains("Invalid spell slot"), "{call}: {err}");
            }
        });
        assert!(host.asked.is_empty(), "{:?}", host.asked);
    }

    /// The shim forwards to the cooldown owner rather than pre-checking `spell_info`; the owner
    /// decides, as `Script_GetSpellCooldown` does. (The current app host's own `cooldown` query
    /// still needs its catalog, so this is about the shim, not a guaranteed live cooldown.)
    #[test]
    fn the_cooldown_owner_is_not_gated_on_spell_info() {
        let mut s = wrath();
        s.set_now(100.0);
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BAR_SPELL)));
        let mut host = SpellHost {
            known: Vec::new(), // no `spell_info` answer
            cooldowns: vec![(BAR_SPELL, (100_000, 5_000, true))],
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<(f64, f64, i64)>("return GetSpellCooldown(3009, BOOKTYPE_SPELL)")
                    .unwrap(),
                (100.0, 5.0, 1)
            );
        });
        assert!(
            host.asked.is_empty(),
            "the cooldown owner is not the spell catalog"
        );
    }

    /// The fallback is a Wrath action-bar compatibility path, not a general slot relaxation.
    #[test]
    fn the_action_bar_spell_id_fallback_is_wrath_only() {
        let mut s = UiScript::new().unwrap();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BAR_SPELL)));
        for call in [
            "GetSpellName(3009, BOOKTYPE_SPELL)",
            "GetSpellCooldown(3009, BOOKTYPE_SPELL)",
        ] {
            let err = s.run(call).unwrap_err().to_string();
            assert!(err.contains("Invalid spell slot"), "{call}: {err}");
        }
    }

    /// A harness with no app call host seats the client catalog's projection: the three spell
    /// queries read it, an id it does not hold stays nil, and a fork shares it. A lent host still
    /// answers first, as the live client's does.
    #[test]
    fn a_seated_catalog_answers_the_spell_queries_without_a_host() {
        let mut s = wrath();
        s.set_spell_info(std::sync::Arc::new(std::collections::HashMap::from([(
            78,
            SpellInfoView {
                name: "Heroic Strike".into(),
                rank: Some("Rank 1".into()),
                icon: Some("Interface\\Icons\\Ability_Rogue_Eviscerate".into()),
                cost: None,
                channeled: false,
                power_type: 0,
                cast_time_ms: 0,
                min_range: None,
                max_range: Some(5.0),
            },
        )])));
        assert_eq!(
            s.eval::<String>("return (GetSpellInfo(78))").unwrap(),
            "Heroic Strike"
        );
        assert_eq!(
            s.eval::<String>("return GetSpellTexture(78)").unwrap(),
            "Interface\\Icons\\Ability_Rogue_Eviscerate"
        );
        assert!(s
            .eval::<String>("return (GetSpellLink(78))")
            .unwrap()
            .contains("[Heroic Strike]"));
        assert!(s.eval::<bool>("return GetSpellInfo(79) == nil").unwrap());
        // The fork a survey makes per addon shares the seat.
        let fork = s.fork();
        assert_eq!(
            fork.eval::<String>("return (GetSpellInfo(78))").unwrap(),
            "Heroic Strike"
        );
        // The lent host's catalog wins over the seat, as the live app's does.
        let mut host = SpellHost {
            known: vec![78],
            cooldowns: Vec::new(),
            asked: Vec::new(),
        };
        host::lend(s.lua(), &mut host, || {
            assert_eq!(
                s.eval::<String>("return (GetSpellInfo(78))").unwrap(),
                "Spell 78"
            );
        });
        assert_eq!(host.asked, vec![78]);
    }
}
