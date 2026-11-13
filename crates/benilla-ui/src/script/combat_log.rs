//! The 3.3.5a raw combat-log query/filter globals, from
//! `WoW/Source/Object/ObjectClient/UnitCombatLog_C_Scripts.cpp`.
//!
//! The filter list and the retention fallback are real state. The reference's raw history is not:
//! `QueueCombatLogEntry` (`0x0074FF20`, `UnitCombatLog_C.cpp:914`) and `SendCombatLogEvents`
//! (`0x0074F910`, `:866`) are the producer that links `CombatLogEntry` records into `s_entries`,
//! and `CombatLogEntry::PushEvent` (`0x0074E290`, `:1241-1276`) builds the tuple the query verbs
//! return. Benilla has no such producer yet, so the history is truthfully empty:
//! `CombatLogGetNumEntries()` answers 0, `CombatLogSetCurrentEntry`/`CombatLogAdvanceEntry` answer
//! nil, `CombatLogGetCurrentEntry()` answers no values, and `CombatLogClearEntries` clears
//! nothing. No event tuple is synthesized.
//!
//! Two branches of the reference filter are also not reached because nothing matches entries yet:
//! `CombatLogFilter::SetFilterData` (`0x0074E050`, `:333`) tokenizes the event text into a 50-bit
//! event mask and parses guid selectors with `StringToGuid`; here the ordered rows keep the event
//! text and the raw guid text. `CombatLogAddFilter`'s optional fourth spell id/name selector is
//! likewise not stored: the stock 3.3.5a caller passes three arguments. All of these must be
//! completed together with the entry ring and its matcher.
//!
//! Consequence of the empty ring: a filter change (`Blizzard_CombatLog_Refilter`) succeeds but
//! clears the visible frame and refills nothing. The stock frame registers the filtered
//! `COMBAT_LOG_EVENT` (`Blizzard_CombatLog.lua:3365-3366`), while the only current producer emits
//! `COMBAT_LOG_EVENT_UNFILTERED` for `PARTY_KILL`
//! (`crates/benilla-app/src/ui_script/wrath_combat_log_feed.rs`). The API no longer aborts; this
//! is not working filtered content.
//!
//! The retention value is the reference's `combatLogRetentionTime` CVar when the host registers it
//! (`crates/benilla-app/src/cvars/table.rs`): `Script_CombatLogGetRetentionTime` (`0x0074D9E0`)
//! pushes the row's `m_intValue`, the Storm `SStrToInt` of its text, and
//! `Script_CombatLogSetRetentionTime` (`0x0074D5B0`) writes the integer back. A bare test VM with
//! no row falls back to `CombatLogState::retention_seconds`, whose default 300 matches the
//! reference's registered `"300"`. That field is only the bare-VM fallback: while the row exists
//! the setter writes the CVar alone, so a direct `SetCVar` and the native getter cannot disagree.
//! A refused (read-only) CVar write leaves the getter on the old value, as the reference's getter
//! reads the same record.
//!
//! `CombatLog_Object_IsA` (`0x0074D600`), which `CombatLog_OnEvent` calls before formatting a
//! named endpoint (`Blizzard_CombatLog.lua:2239`, `:2242`, `:2902-2916`), is installed by this
//! module. With only unfiltered `PARTY_KILL` producers the stock frame does not reach that path
//! yet; the remaining step before filtered event content is the entry ring and its matcher, not
//! this native.
//!
//! The same table also registers `CombatTextSetActiveUnit` (`Script_CombatTextSetActiveUnit`,
//! `0x0074D580`, `:153-158`; the registrar `UnitCombatLogRegisterScriptFunctions` is `0x0074D6B0`).
//! Stock `Blizzard_CombatText.lua:114`, `:120` and `:537` call it, and `Blizzard_CombatText` is
//! LoadOnDemand (`crates/benilla-app/assets/ui/OptionsFrame.xml:36-39`), so the Combat Text options
//! path is an adjacent, separate symptom that remains uninstalled with no Project B consumer.
//!
//! Stock callers:
//! - `Interface\AddOns\Blizzard_CombatLog\Blizzard_CombatLog.lua:633-695` calls
//!   `CombatLogResetFilter()` then `CombatLogAddFilter(eventList, source, dest)`.
//! - `:709-764` walks the history with `CombatLogGetNumEntries()`,
//!   `CombatLogSetCurrentEntry(0)`, `CombatLogGetCurrentEntry()` and `CombatLogAdvanceEntry(-1)`.

use mlua::{Lua, MultiValue, Value};

use super::Model;

/// One `CombatLogFilter` row: the event-list text and the two selectors, in insertion order.
/// The reference's fourth spell selector has no consumer here (see the module doc).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CombatLogFilter {
    /// The events argument as text, `None` when the caller passed nil (all events). The reference
    /// tokenizes this into its 50-bit mask in `SetFilterData`.
    pub(crate) events: Option<String>,
    pub(crate) source: FilterSelector,
    pub(crate) dest: FilterSelector,
}

/// `CombatLogAddFilter`'s source/dest selector: a numeric category mask, a guid string, or the
/// reference's default `0xFFFFFFFF` for any other Lua type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum FilterSelector {
    /// Neither a number nor a string: the reference stores `0xFFFFFFFF`.
    #[default]
    All,
    /// The caller's number, truncated to the reference's `uint32_t` (so `-1` is `0xFFFFFFFF`).
    Mask(u32),
    /// The caller's guid text, kept as written; the reference parses it with `StringToGuid`.
    Guid(String),
}

/// The filter list (`s_filters`) and the fallback `combatLogRetentionTime` value used when no such
/// CVar row is registered. The reference default is `"300"` seconds
/// (`WoW/Source/Ui/GameUI_CVar.cpp:633`); no raw-entry ring exists beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CombatLogState {
    pub(crate) filters: Vec<CombatLogFilter>,
    pub(crate) retention_seconds: i32,
}

impl Default for CombatLogState {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            retention_seconds: 300,
        }
    }
}

/// The reference accepts a numeric mask when either a high bit is set, or all four low category
/// fields (`0xF`, `0xF0`, `0x300`, `0xFC00`) are present (`Script_CombatLogAddFilter` `0x0074FF70`).
fn mask_complete(mask: u32) -> bool {
    mask & 0xFFFF_0000 != 0
        || (mask & 0x0000_000F != 0
            && mask & 0x0000_00F0 != 0
            && mask & 0x0000_0300 != 0
            && mask & 0x0000_FC00 != 0)
}

/// A numeric selector's `uint32_t`, as `lua_tonumber` then `static_cast<uint32_t>` does.
fn mask_of(value: &Value) -> Option<u32> {
    match value {
        Value::Integer(value) => Some(*value as u32),
        Value::Number(value) => Some(*value as i64 as u32),
        _ => None,
    }
}

/// Read one source/dest selector exactly as `Script_CombatLogAddFilter` switches on `lua_type`:
/// number is a category mask and raises when incomplete, string is a guid, anything else is the
/// all-mask default.
fn selector(value: &Value, field: &str) -> mlua::Result<FilterSelector> {
    if let Some(mask) = mask_of(value) {
        if !mask_complete(mask) {
            return Err(mlua::Error::runtime(format!(
                "CombatLogAddFilter(): incomplete filter for {field}"
            )));
        }
        return Ok(FilterSelector::Mask(mask));
    }
    match value {
        Value::String(guid) => Ok(FilterSelector::Guid(guid.to_string_lossy())),
        _ => Ok(FilterSelector::All),
    }
}

fn number_arg(lua: &Lua, args: &MultiValue) -> mlua::Result<Option<f64>> {
    lua.coerce_number(args.front().cloned().unwrap_or(Value::Nil))
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    // `0x0074FA60`: `s_filters.Clear()`.
    globals.set(
        "CombatLogResetFilter",
        lua.create_function(|lua, ()| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .combat_log
                .filters
                .clear();
            Ok(())
        })?,
    )?;

    // `Script_CombatLog_Object_IsA` (`0x0074D600`): AND the two masks and answer 1.0 when a high
    // category bit or all four low category groups survive, else nil. `lua_tonumber` reads a
    // missing/non-numeric argument as 0; the stock formatter uses the 1/nil shape bare.
    globals.set(
        "CombatLog_Object_IsA",
        lua.create_function(|lua, (mask, category): (Value, Value)| {
            let mask = lua.coerce_number(mask)?.unwrap_or(0.0) as i64 as u32;
            let category = lua.coerce_number(category)?.unwrap_or(0.0) as i64 as u32;
            let flags = mask & category;
            let is_a = flags & 0xFFFF_0000 != 0
                || (flags & 0x0000_000F != 0
                    && flags & 0x0000_00F0 != 0
                    && flags & 0x0000_0300 != 0
                    && flags & 0x0000_FC00 != 0);
            Ok(if is_a { Value::Number(1.0) } else { Value::Nil })
        })?,
    )?;

    // `0x0074FF70`: one row. The reference returns 0 values and stores the row after validation.
    globals.set(
        "CombatLogAddFilter",
        lua.create_function(
            |lua, (events, source, dest): (Value, Value, Value)| {
                // The reference's optional fourth spell selector (arg 4) is not stored: the stock
                // 3.3.5a caller passes three arguments and no entry matcher consumes it.
                let events = lua.coerce_string(events)?.map(|s| s.to_string_lossy());
                let source = selector(&source, "srcMask")?;
                let dest = selector(&dest, "dstMask")?;
                lua.app_data_mut::<Model>()
                    .expect("model app_data")
                    .combat_log
                    .filters
                    .push(CombatLogFilter {
                        events,
                        source,
                        dest,
                    });
                Ok(())
            },
        )?,
    )?;

    // `0x0074FA70`: one integer. With no producer it is always 0; `all` does not change that.
    globals.set(
        "CombatLogGetNumEntries",
        lua.create_function(|_, _: MultiValue| Ok(0i64))?,
    )?;

    // `0x0074FAE0`: one nil, after the reference's numeric-index check.
    globals.set(
        "CombatLogSetCurrentEntry",
        lua.create_function(|lua, args: MultiValue| {
            if number_arg(lua, &args)?.is_none() {
                return Err(mlua::Error::runtime(
                    "Usage: CombatLogSetCurrentEntry(index [,ignoreFilter])",
                ));
            }
            Ok(Value::Nil)
        })?,
    )?;

    // `0x0074F2B0`: no values while no entry is current.
    globals.set(
        "CombatLogGetCurrentEntry",
        lua.create_function(|_, _: MultiValue| Ok(MultiValue::new()))?,
    )?;

    // `0x0074FC20`: one nil, after the reference's numeric-count check.
    globals.set(
        "CombatLogAdvanceEntry",
        lua.create_function(|lua, args: MultiValue| {
            if number_arg(lua, &args)?.is_none() {
                return Err(mlua::Error::runtime(
                    "Usage: CombatLogAdvanceEntry(count [,ignoreFilter])",
                ));
            }
            Ok(Value::Nil)
        })?,
    )?;

    // `0x00751120`: no raw entries to recycle, so no values.
    globals.set(
        "CombatLogClearEntries",
        lua.create_function(|_, ()| Ok(()))?,
    )?;

    // `0x0074D9E0`: `g_combatLogRetentionTimeCVar->m_intValue`, Storm's `SStrToInt` of the row's
    // text. A bare test VM has no row, so the Model fallback answers instead.
    globals.set(
        "CombatLogGetRetentionTime",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let seconds = match model.cvars.get("combatlogretentiontime") {
                Some(slot) => crate::script::binding_abi::s_str_to_int(&slot.value),
                None => model.combat_log.retention_seconds,
            };
            Ok(i64::from(seconds))
        })?,
    )?;

    // `0x0074D5B0`: `CombatLogSetRetentionTime(seconds)`, a number stored as an integer. The row
    // write is the reference's `CVar::Set`; the Model copy is the bare-VM fallback and is not
    // authoritative while the row exists, so a direct `SetCVar` leaves it behind. A read-only row
    // refuses the CVar write, so the getter above keeps answering the old value there.
    globals.set(
        "CombatLogSetRetentionTime",
        lua.create_function(|lua, args: MultiValue| {
            let Some(seconds) = number_arg(lua, &args)? else {
                return Err(mlua::Error::runtime(
                    "Usage: CombatLogSetRetentionTime(seconds)",
                ));
            };
            let seconds = crate::script::binding_abi::ftol(seconds);
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            // The CVar is the source of truth once it exists; writing the fallback too would leave
            // a stale copy that a later `SetCVar` never updates.
            if !model.cvars.contains_key("combatlogretentiontime") {
                model.combat_log.retention_seconds = seconds;
            }
            crate::script::cvars::set_from_engine(
                &mut model,
                "combatLogRetentionTime",
                seconds.to_string(),
            );
            Ok(())
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Model, UiScript, UiTarget};

    fn wrath() -> UiScript {
        UiScript::with_target(UiTarget::Wrath335).unwrap()
    }

    fn filters(script: &UiScript) -> Vec<CombatLogFilter> {
        script
            .lua()
            .app_data_ref::<Model>()
            .expect("model app_data")
            .combat_log
            .filters
            .clone()
    }

    #[test]
    fn reset_clears_a_stored_filter() {
        let script = wrath();
        script
            .run(r#"CombatLogAddFilter("SWING_DAMAGE", 2147483648, 2147483648)"#)
            .unwrap();
        assert_eq!(filters(&script).len(), 1, "the filter was stored");
        script.run("CombatLogResetFilter()").unwrap();
        assert!(filters(&script).is_empty(), "reset clears s_filters");
    }

    /// The stock `Blizzard_CombatLog_ApplyFilters` shape: reset, then one `AddFilter` per enabled
    /// row, with complete numeric masks or guid strings. A complete low-only mask is accepted, as
    /// the reference accepts one with all four category fields present.
    #[test]
    fn apply_filters_sequence_stores_complete_masks_and_guids() {
        let script = wrath();
        script
            .run(
                r#"
                CombatLogResetFilter()
                CombatLogAddFilter("SWING_DAMAGE SPELL_DAMAGE", 1297, 2147483648)
                CombatLogAddFilter(nil, "0x0000000000000011", "0xF130000000000022")
                "#,
            )
            .unwrap();
        let rows = filters(&script);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            CombatLogFilter {
                events: Some("SWING_DAMAGE SPELL_DAMAGE".into()),
                source: FilterSelector::Mask(0x0000_0511),
                dest: FilterSelector::Mask(0x8000_0000),
            }
        );
        assert_eq!(
            rows[1],
            CombatLogFilter {
                events: None,
                source: FilterSelector::Guid("0x0000000000000011".into()),
                dest: FilterSelector::Guid("0xF130000000000022".into()),
            }
        );
    }

    /// The install's stock constants: `COMBATLOG_FILTER_MINE = 0x4511`,
    /// `COMBATLOG_FILTER_MY_PET = 0x3111`, so the default source mask is `0x7511` with a nil dest;
    /// `COMBATLOG_FILTER_EVERYTHING = 0xFFFFFFFF`. Stock calls never pass a numeric 0 (the
    /// `sourceFlags == 0 or destFlags == 0` HACK at `Blizzard_CombatLog.lua:691` avoids it), and a
    /// numeric 0 still raises here.
    #[test]
    fn stock_filter_constants_are_complete_and_zero_raises() {
        let script = wrath();
        script
            .run(r#"CombatLogAddFilter("", 29969, nil)"#) // 0x7511 = MINE|MY_PET
            .unwrap();
        script
            .run(r#"CombatLogAddFilter("", 17681, nil)"#) // 0x4511 = MINE
            .unwrap();
        script
            .run(r#"CombatLogAddFilter("", 4294967295, 4294967295)"#) // EVERYTHING
            .unwrap();
        let rows = filters(&script);
        assert_eq!(rows[0].source, FilterSelector::Mask(0x0000_7511));
        assert_eq!(rows[0].dest, FilterSelector::All, "the stock dest is nil");
        assert_eq!(rows[1].source, FilterSelector::Mask(0x0000_4511));
        assert_eq!(rows[1].dest, FilterSelector::All);
        assert_eq!(rows[2].source, FilterSelector::Mask(0xFFFF_FFFF));
        assert_eq!(rows[2].dest, FilterSelector::Mask(0xFFFF_FFFF));
        assert!(
            script.run(r#"CombatLogAddFilter("", 0, nil)"#).is_err(),
            "a numeric zero mask is incomplete and must raise"
        );
        assert_eq!(
            filters(&script).len(),
            3,
            "the rejected zero row is not stored"
        );
    }

    /// `lua_isnumber` accepts numeric strings, and the two cursor setters keep the reference's
    /// Usage errors for anything else.
    #[test]
    fn cursor_setters_accept_numbers_and_raise_on_non_numbers() {
        let script = wrath();
        assert_eq!(
            script
                .eval::<bool>("return CombatLogSetCurrentEntry('0') == nil")
                .unwrap(),
            true
        );
        assert_eq!(
            script
                .eval::<bool>("return CombatLogAdvanceEntry('1') == nil")
                .unwrap(),
            true
        );
        assert!(script.run("CombatLogSetCurrentEntry('x')").is_err());
        assert!(script.run("CombatLogAdvanceEntry(nil)").is_err());
    }

    /// `CombatLog_Object_IsA` ANDs the masks: a high category bit or all four low groups survives;
    /// a partial low overlap is nil. The stock formatter tests the result bare.
    #[test]
    fn object_is_a_requires_a_high_bit_or_every_low_group() {
        let script = wrath();
        assert_eq!(
            script
                .eval::<i64>("return CombatLog_Object_IsA(0xFFFFFFFF, 0xFFFF0000)")
                .unwrap(),
            1,
            "a high category bit survives"
        );
        assert_eq!(
            script
                .eval::<i64>("return CombatLog_Object_IsA(0x00000511, 0x00000511)")
                .unwrap(),
            1,
            "all four low groups survive"
        );
        assert!(script
            .eval::<bool>("return CombatLog_Object_IsA(0x00000001, 0x00000001) == nil")
            .unwrap());
        assert_eq!(
            script
                .arity("CombatLog_Object_IsA(1, 1)")
                .unwrap(),
            1,
            "always one value"
        );
    }

    /// An incomplete low-only numeric mask raises before the row is stored, with the reference's
    /// message for the failed selector.
    #[test]
    fn incomplete_numeric_masks_raise_and_store_nothing() {
        let script = wrath();
        let err = script
            .run(r#"CombatLogAddFilter("SWING_DAMAGE", 1, 2147483648)"#)
            .unwrap_err();
        assert!(
            err.to_string().contains("incomplete filter for srcMask"),
            "{err}"
        );
        assert!(filters(&script).is_empty());
        let err = script
            .run(r#"CombatLogAddFilter("SWING_DAMAGE", 2147483648, 1)"#)
            .unwrap_err();
        assert!(
            err.to_string().contains("incomplete filter for dstMask"),
            "{err}"
        );
        assert!(filters(&script).is_empty());
    }

    #[test]
    fn empty_history_queries_have_the_reference_shapes() {
        let script = wrath();
        assert_eq!(
            script.eval::<i64>("return CombatLogGetNumEntries()").unwrap(),
            0
        );
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetNumEntries(true)")
                .unwrap(),
            0,
            "the all flag cannot reveal entries that do not exist"
        );
        assert_eq!(
            script
                .eval::<bool>("return CombatLogSetCurrentEntry(0) == nil")
                .unwrap(),
            true
        );
        assert_eq!(
            script
                .eval::<bool>("return CombatLogSetCurrentEntry(-1, true) == nil")
                .unwrap(),
            true
        );
        assert_eq!(
            script
                .eval::<bool>("return CombatLogAdvanceEntry(-1) == nil")
                .unwrap(),
            true
        );
        assert_eq!(
            script.arity("CombatLogGetCurrentEntry()").unwrap(),
            0,
            "no current entry means no values, not a nil"
        );
        assert_eq!(script.arity("CombatLogClearEntries()").unwrap(), 0);
    }

    /// The exact arities the stock caller distinguishes: one integer from the counter and the
    /// retention getter, one nil from the cursor setters, zero from the rest.
    #[test]
    fn return_arities_match_the_reference() {
        let script = wrath();
        assert_eq!(script.arity("CombatLogResetFilter()").unwrap(), 0);
        assert_eq!(
            script
                .arity("CombatLogAddFilter('SWING_DAMAGE', 2147483648, 2147483648)")
                .unwrap(),
            0
        );
        assert_eq!(script.arity("CombatLogGetNumEntries()").unwrap(), 1);
        assert_eq!(script.arity("CombatLogSetCurrentEntry(0)").unwrap(), 1);
        assert_eq!(script.arity("CombatLogGetCurrentEntry()").unwrap(), 0);
        assert_eq!(script.arity("CombatLogAdvanceEntry(-1)").unwrap(), 1);
        assert_eq!(script.arity("CombatLogClearEntries()").unwrap(), 0);
        assert_eq!(script.arity("CombatLogGetRetentionTime()").unwrap(), 1);
        assert_eq!(script.arity("CombatLogSetRetentionTime(120)").unwrap(), 0);
    }

    #[test]
    fn retention_round_trips_from_the_stock_default() {
        let script = wrath();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            300,
            "combatLogRetentionTime defaults to 300 seconds"
        );
        script.run("CombatLogSetRetentionTime(120)").unwrap();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            120
        );
        assert_eq!(
            script
                .lua()
                .app_data_ref::<Model>()
                .expect("model app_data")
                .combat_log
                .retention_seconds,
            120
        );
        assert!(
            script.run("CombatLogSetRetentionTime()").is_err(),
            "the reference raises Usage for a non-number"
        );
    }

    /// With the host's `combatLogRetentionTime` row registered, the native pair follows the CVar as
    /// `Script_CombatLogGetRetentionTime`/`SetRetentionTime` do, and `GetCVar` no longer raises.
    #[test]
    fn retention_reads_and_writes_the_registered_cvar() {
        let script = wrath();
        script.register_cvars([("combatLogRetentionTime", "300")]);
        assert_eq!(
            script
                .eval::<String>(r#"return GetCVar("combatLogRetentionTime")"#)
                .unwrap(),
            "300",
            "the registered row resolves"
        );
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            300
        );
        script.run("CombatLogSetRetentionTime(120)").unwrap();
        assert_eq!(
            script
                .eval::<String>(r#"return GetCVar("combatLogRetentionTime")"#)
                .unwrap(),
            "120",
            "the setter writes the CVar as the reference's CVar::Set does"
        );
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            120
        );
        script
            .run(r#"SetCVar("combatLogRetentionTime", "45")"#)
            .unwrap();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            45,
            "SetCVar moves the native getter once the row exists"
        );
    }

    /// The getter is the row's `m_intValue`: Storm's `SStrToInt` of its text, so a non-numeric value
    /// reads as 0 while `GetCVar` still answers the text that was stored.
    #[test]
    fn retention_cvar_text_is_parsed_as_the_reference_integer() {
        let script = wrath();
        script.register_cvars([("combatLogRetentionTime", "300")]);
        script
            .run(r#"SetCVar("combatLogRetentionTime", "12abc")"#)
            .unwrap();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            12,
            "SStrToInt stops at the first non-digit"
        );
        script
            .run(r#"SetCVar("combatLogRetentionTime", "not a number")"#)
            .unwrap();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            0,
            "a non-numeric row text is m_intValue 0"
        );
        assert_eq!(
            script
                .eval::<String>(r#"return GetCVar("combatLogRetentionTime")"#)
                .unwrap(),
            "not a number",
            "the row keeps its text; only the native view parses it"
        );
    }

    /// A read-only row refuses the write as `CVar::Set` does: the native getter keeps the applied
    /// value, and the Model fallback keeps its old copy because the row owns the value once it
    /// exists (`f12dda68`).
    #[test]
    fn read_only_retention_cvar_keeps_the_native_getter_on_the_old_value() {
        let mut script = wrath();
        script.register_cvars([("combatLogRetentionTime", "300")]);
        script.set_cvar_read_only("combatLogRetentionTime", true);
        script.run("CombatLogSetRetentionTime(120)").unwrap();
        assert_eq!(
            script
                .eval::<i64>("return CombatLogGetRetentionTime()")
                .unwrap(),
            300,
            "the refused write leaves the CVar at its applied value"
        );
        // The row is the source of truth once it exists (`f12dda68`): the Model copy keeps its
        // old value so a refused or later write cannot be shadowed by a stale fallback.
        assert_eq!(
            script
                .lua()
                .app_data_ref::<Model>()
                .expect("model app_data")
                .combat_log
                .retention_seconds,
            300,
            "the row owns the value; the refused write leaves the fallback behind"
        );
    }
}
