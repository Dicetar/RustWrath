//! 12340 macro ABI regressions; no installed assets or server required.

use crate::script::{MacroLimits, MacroState, MacroView, UiScript, UiTarget};

fn script() -> UiScript {
    let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
    script.set_macro_icons(vec!["Interface\\Icons\\Ability_Ambush".into()]);
    script
}

#[test]
fn wrath_create_macro_uses_argument_four_for_character_scope() {
    let script = script();
    script
        .run("CreateMacro('Account', 1, '/cast Fireball'); CreateMacro('Character', 1, '/cast Frostbolt', 1)")
        .unwrap();
    assert_eq!(
        script.eval::<(i64, i64)>("return GetNumMacros()").unwrap(),
        (1, 1)
    );
    assert!(!script.macros().by_id(1).unwrap().local_only);
    assert!(!script.macros().saved(true)[0].local_only);
}

#[test]
fn wrath_macro_info_accepts_names_and_returns_three_values_or_none() {
    let script = script();
    script
        .run("CreateMacro('Fire', 1, '/cast Fireball')")
        .unwrap();
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo('Fire'))")
            .unwrap(),
        3
    );
    assert_eq!(
        script
            .eval::<String>("local _, _, body = GetMacroInfo('Fire'); return body")
            .unwrap(),
        "/cast Fireball"
    );
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo('missing'))")
            .unwrap(),
        0
    );
    assert_eq!(
        script.eval::<String>("return GetMacroInfo('1')").unwrap(),
        "Fire"
    );
}

#[test]
fn wrath_edit_pickup_and_delete_resolve_the_same_macro_name() {
    let script = script();
    script
        .run("CreateMacro('Fire', 1, '/cast Fireball')")
        .unwrap();
    script
        .run("EditMacro('Fire', 'Frost', nil, '/cast Frostbolt', 1)")
        .unwrap();
    assert_eq!(
        script
            .eval::<String>("local _, _, body = GetMacroInfo('Frost'); return body")
            .unwrap(),
        "/cast Frostbolt"
    );
    assert!(!script.macros().by_id(1).unwrap().local_only);
    script.run("PickupMacro('Frost'); PlaceAction(1)").unwrap();
    assert!(script.eval::<bool>("return HasAction(1) == 1").unwrap());
    script.run("DeleteMacro('Frost')").unwrap();
    assert!(script.eval::<bool>("return HasAction(1) == nil").unwrap());
    assert_eq!(
        script.eval::<(i64, i64)>("return GetNumMacros()").unwrap(),
        (0, 0)
    );
}

/// `RunMacro` fires one `EXECUTE_CHAT_LINE` per body line and `StopMacro` from a line's
/// handler drops the rest; the running id/button are visible while it runs and nil after.
#[test]
fn wrath_run_macro_fires_lines_and_stop_drops_the_rest() {
    let script = script();
    script
        .run("CreateMacro('Talk', 1, '/say one\\n/say two')")
        .unwrap();
    assert_eq!(
        script
            .eval::<String>("return GetMacroBody('Talk')")
            .unwrap(),
        "/say one\n/say two"
    );
    script
        .run(
            "seen = {}; running = nil; button = nil
             local f = CreateFrame('Frame'); f:RegisterEvent('EXECUTE_CHAT_LINE')
             f:SetScript('OnEvent', function(_, event, line)
                 assert(event == 'EXECUTE_CHAT_LINE')
                 table.insert(seen, line)
                 running = GetRunningMacro()
                 button = GetRunningMacroButton()
                 if line == '/say one' then StopMacro() end
             end)
             RunMacro('Talk', 'ActionButton1')",
        )
        .unwrap();
    assert_eq!(script.eval::<i64>("return #seen").unwrap(), 1);
    assert_eq!(script.eval::<i64>("return running").unwrap(), 1);
    assert_eq!(
        script.eval::<String>("return button").unwrap(),
        "ActionButton1"
    );
    assert_eq!(
        script
            .eval::<i64>("return GetRunningMacro() == nil and 1 or 0")
            .unwrap(),
        1
    );
    script
        .run("seen = {}; RunMacroText('/say a\\n/say b')")
        .unwrap();
    assert_eq!(script.eval::<i64>("return #seen").unwrap(), 2);
    assert!(script.errors().is_empty(), "{:?}", script.errors());
}

/// 12340's sorted table is 36 account entries then 36 character entries (`m_sortedMacros[72]`),
/// with the character scope capped at 18: the stock MacroFrame reads account 1..=36 and character
/// at `MAX_ACCOUNT_MACROS + i` (37..=54).
#[test]
fn wrath_macro_table_is_36_account_and_18_character_over_a_72_slot_stride() {
    let script = script();
    script
        .run("MAX_ACCOUNT_MACROS = 36; MAX_CHARACTER_MACROS = 18")
        .unwrap();
    script
        .run("for i = 1, 36 do CreateMacro(string.format('A%02d', i), 1, '/cast Fireball') end")
        .unwrap();
    script
        .run("for i = 1, 18 do CreateMacro(string.format('C%02d', i), 1, '/cast Frostbolt', 1) end")
        .unwrap();
    assert_eq!(
        script.eval::<(i64, i64)>("return GetNumMacros()").unwrap(),
        (36, 18)
    );
    assert_eq!(
        script
            .eval::<String>("local n = GetMacroInfo(36); return n")
            .unwrap(),
        "A36"
    );
    assert_eq!(
        script
            .eval::<String>("local n = GetMacroInfo(37); return n")
            .unwrap(),
        "C01"
    );
    assert_eq!(
        script
            .eval::<String>("local n = GetMacroInfo(54); return n")
            .unwrap(),
        "C18"
    );
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo(19))")
            .unwrap(),
        3
    );
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo(55))")
            .unwrap(),
        0
    );
    assert_eq!(
        script
            .eval::<i64>("return GetMacroIndexByName('C01')")
            .unwrap(),
        37
    );
}

/// The current stock constants cap account at 36 and character at 18; the 37th and 19th are the
/// scoped "already have N macros" refusal, as the reference's `s_maxMacros[2]` does.
#[test]
fn wrath_macro_caps_refuse_the_37th_account_and_19th_character() {
    let script = script();
    script
        .run("MAX_ACCOUNT_MACROS = 36; MAX_CHARACTER_MACROS = 18")
        .unwrap();
    script
        .run("for i = 1, 36 do CreateMacro(string.format('A%02d', i), 1, '') end")
        .unwrap();
    let err = script
        .eval::<String>("local ok, e = pcall(CreateMacro, 'A37', 1, ''); return e")
        .unwrap();
    assert!(err.contains("already have 36 macros"), "{err}");
    script
        .run("for i = 1, 18 do CreateMacro(string.format('C%02d', i), 1, '', 1) end")
        .unwrap();
    let err = script
        .eval::<String>("local ok, e = pcall(CreateMacro, 'C19', 1, '', 1); return e")
        .unwrap();
    assert!(err.contains("already have 18 macros"), "{err}");
}

/// The caps are the live stock globals, not a Rust table: lowering the account constant hides
/// entries past it and refuses new ones; raising it back restores the table's view.
#[test]
fn wrath_macro_caps_follow_the_live_stock_constants() {
    let script = script();
    script
        .run("MAX_ACCOUNT_MACROS = 36; MAX_CHARACTER_MACROS = 18")
        .unwrap();
    script
        .run("for i = 1, 20 do CreateMacro(string.format('A%02d', i), 1, '') end")
        .unwrap();
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo(20))")
            .unwrap(),
        3
    );
    script.run("MAX_ACCOUNT_MACROS = 18").unwrap();
    assert_eq!(
        script
            .eval::<i64>("return select('#', GetMacroInfo(19))")
            .unwrap(),
        0
    );
    assert_eq!(
        script
            .eval::<bool>("return pcall(CreateMacro, 'A21', 1, '')")
            .unwrap(),
        false
    );
    script.run("MAX_ACCOUNT_MACROS = 36").unwrap();
    assert_eq!(
        script
            .eval::<String>("local n = GetMacroInfo(20); return n")
            .unwrap(),
        "A20"
    );
}

/// The file reader keeps the same 36 account records the Wrath table can address; the donor
/// reader still stops at its 18.
#[test]
fn wrath_file_reader_keeps_36_account_records() {
    fn views(prefix: &str, n: usize) -> Vec<MacroView> {
        (0..n)
            .map(|i| MacroView {
                name: format!("{prefix}{i:02}"),
                ..Default::default()
            })
            .collect()
    }
    let state =
        MacroState::from_scopes_with_limits(MacroLimits::WRATH, views("A", 36), views("C", 18));
    assert_eq!(state.count(false), 36);
    assert_eq!(state.count(true), 18);
    assert_eq!(state.get(36).unwrap().name, "A35");
    assert_eq!(state.get(37).unwrap().name, "C00");
    let donor = MacroState::from_scopes(views("A", 36), views("C", 18));
    assert_eq!(donor.count(false), 18, "the donor reader stops at 18");
    assert_eq!(donor.count(true), 18);
}
