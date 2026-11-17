//! 12340 macro ABI regressions; no installed assets or server required.

use crate::script::{UiScript, UiTarget};

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
