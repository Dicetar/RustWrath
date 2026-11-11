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
                    Some(reference) => Ok(Some(reference)),
                    None => Err(err),
                };
            }
        }
    }
    if let Some(number) = lua.coerce_number(input.clone())? {
        return Ok((number.is_finite() && number >= 1.0 && number <= f64::from(u32::MAX))
            .then_some((number as u32, false)));
    }
    let Value::String(name) = input else { return Ok(None) };
    let name = name.to_str()?;
    let model = lua.app_data_ref::<Model>().expect("model");
    let player_slots = if model.spell_list.is_empty() {
        &model.spellbook.slots
    } else {
        &model.spell_list
    };
    Ok(spellbook::resolve_spell_in_list(player_slots, &model.pet_book, &name)
        .map(|spell| (spell.spell_id, spell.pet)))
}

fn query(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<(u32, SpellInfoView)>> {
    let Some((id, _)) = spell_reference(lua, args)? else { return Ok(None) };
    Ok(host::with_host(lua, |host| host.spell_info(id)).flatten().map(|view| (id, view)))
}

fn string(lua: &Lua, value: Option<&str>) -> mlua::Result<Value> {
    value.map_or(Ok(Value::Nil), |value| lua.create_string(value).map(Value::String))
}

fn unit_cast(lua: &Lua, token: Value, channel: bool) -> mlua::Result<MultiValue> {
    let usage = if channel { "Usage: UnitChannelInfo(unit)" } else { "Usage: UnitCastingInfo(unit)" };
    let token = super::binding_abi::string_arg(lua, token, usage)?;
    super::unit::check_unit_token_for(&Some(token.clone()), true)?;
    let guid = {
        let model = lua.app_data_ref::<Model>().expect("model");
        model.unit(&token).map(|unit| unit.guid)
    };
    let view = guid.and_then(|guid| host::with_host(lua, |host| host.unit_cast(guid, channel)).flatten());
    let Some(view) = view else { return Ok(MultiValue::from_vec(vec![Value::Nil])); };
    let name = string(lua, Some(&view.name))?;
    let mut values = vec![
        name.clone(), string(lua, Some(view.rank.as_deref().unwrap_or("")))?, name,
        string(lua, view.icon.as_deref())?, Value::Number(view.start_ms), Value::Number(view.end_ms),
        Value::Boolean(view.trade_skill),
    ];
    // The target CastingBarFrame uses slot eight for cast identity, but channel tuples omit it.
    if !channel {
        values.push(view.cast_id.map_or(Value::Nil, |id| Value::Number(id as f64)));
    }
    values.push(view.not_interruptible.map_or(Value::Nil, Value::Boolean));
    Ok(MultiValue::from_vec(values))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("GetSpellCooldown", lua.create_function(|lua, args: MultiValue| {
        let value = if let Some(book) = args.get(1).filter(|book| !matches!(book, Value::Nil)) {
            // Preserve the existing pushed-book and pet-book forms, including their errors.
            spellbook::book_cooldown(lua, (args.front().cloned().unwrap_or(Value::Nil), book.clone()))?
        } else {
            let Some((id, pet)) = spell_reference(lua, &args)? else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            let Some(cooldown) = host::with_host(lua, |host| host.spell_cooldown(id, pet)).flatten() else {
                return Ok(MultiValue::from_vec(vec![Value::Nil]));
            };
            spellbook::cooldown_tuple(Some(cooldown), super::clock::now(lua))
        };
        Ok(MultiValue::from_vec(vec![
            Value::Number(value.0), Value::Number(value.1), Value::Integer(i64::from(value.2)),
        ]))
    })?)?;
    globals.set("UnitCastingInfo", lua.create_function(|lua, token: Value| unit_cast(lua, token, false))?)?;
    globals.set("UnitChannelInfo", lua.create_function(|lua, token: Value| unit_cast(lua, token, true))?)?;
    globals.set("IsSpellKnown", lua.create_function(|lua, (id, pet): (Value, Option<bool>)| {
        let id = lua.coerce_number(id)?.filter(|id| id.is_finite() && *id >= 1.0 && *id <= f64::from(u32::MAX))
            .ok_or_else(|| mlua::Error::runtime("Usage: IsSpellKnown(spellID [, isPet])"))? as u32;
        Ok(host::with_host(lua, |host| host.spell_known(id, pet.unwrap_or(false))).flatten())
    })?)?;
    globals.set("GetSpellInfo", lua.create_function(|lua, args: MultiValue| {
        let Some((_, view)) = query(lua, &args)? else { return Ok(MultiValue::from_vec(vec![Value::Nil])) };
        Ok(MultiValue::from_vec(vec![
            string(lua, Some(&view.name))?,
            string(lua, Some(view.rank.as_deref().unwrap_or("")))?,
            string(lua, view.icon.as_deref())?,
            view.cost.map_or(Value::Nil, |cost| Value::Integer(i64::from(cost))),
            Value::Boolean(view.channeled),
            Value::Integer(i64::from(view.power_type)),
            Value::Integer(i64::from(view.cast_time_ms)),
            view.min_range.map_or(Value::Nil, |range| Value::Number(f64::from(range))),
            view.max_range.map_or(Value::Nil, |range| Value::Number(f64::from(range))),
        ]))
    })?)?;
    globals.set("GetSpellTexture", lua.create_function(|lua, args: MultiValue| {
        string(lua, query(lua, &args)?.as_ref().and_then(|(_, view)| view.icon.as_deref()))
    })?)?;
    globals.set("GetSpellLink", lua.create_function(|lua, args: MultiValue| {
        let Some((id, view)) = query(lua, &args)? else { return Ok(Value::Nil) };
        let link = format!("|cff71d5ff|Hspell:{id}|h[{}]|h|r", view.name);
        string(lua, Some(&link))
    })?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{host, SpellInfoView};
    use crate::script::action::ACTION_KIND_SPELL;
    use crate::script::{
        ActionSlot, CallHost, CallTail, ScriptCall, SpellSlotView, UiScript, UiTarget,
    };

    const BAR_SPELL: u32 = 3009;
    const BOOK_SPELL: u32 = 133;

    /// The host's spell catalog, answering only the IDs it knows and recording every query.
    struct SpellHost {
        known: Vec<u32>,
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
            vec![BAR_SPELL; 5],
            "the host is asked for the ID, not a slot"
        );
    }

    /// The same out-of-range number is still a genuine invalid slot when no action bar holds it.
    #[test]
    fn an_out_of_range_number_off_the_action_bar_still_raises() {
        let mut s = wrath();
        s.set_action(1, Some(slot(ACTION_KIND_SPELL, BOOK_SPELL)));
        let mut host = SpellHost {
            known: vec![BAR_SPELL],
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
}
