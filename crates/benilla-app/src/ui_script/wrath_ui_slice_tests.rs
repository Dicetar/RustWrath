//! Stock UI-host smoke test (R11): load the real 3.3.5a PlayerFrame dependency chain, manually
//! fire `UNIT_HEALTH`, query `UnitExists`, and read back a CVar in one VM. It does not prove a
//! session-driven event, a production player action, cross-host persistence or an installed
//! addon; those remain issue 22/24 and Phase D acceptance work. The test skips without game data.
//! `wow_data_or_skip!` pattern. Everything asserted here is real stock-file behavior; no
//! missing API is stubbed successful.

use benilla_ui::script::{ScriptValue, UiScript};

use super::test_ui::load_ui as load_xml;

/// The production load order for the player unit frame, from the stock `PlayerFrame`
/// dependency chain (the same prefix the trainer test uses, plus the unit-frame files).
fn player_frame_script() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(7.0)));
    for f in [
        "Interface\\FrameXML\\GlobalStrings.lua",
        "Interface\\FrameXML\\Fonts.xml",
        "Interface\\FrameXML\\BasicControls.xml",
        "Interface\\FrameXML\\LocaleProperties.lua",
        r"Interface\FrameXML\UIParent.xml",
        r"Interface\FrameXML\MoneyFrame.lua",
        r"Interface\FrameXML\MoneyFrame.xml",
        "Interface\\FrameXML\\GameTooltip.xml",
        r"Interface\FrameXML\UIPanelTemplates.lua",
        r"Interface\FrameXML\UIPanelTemplates.xml",
        "Interface\\FrameXML\\TextStatusBar.lua",
        "Interface\\FrameXML\\TextStatusBar.xml",
        "Interface\\FrameXML\\UnitFrame.lua",
        r"Interface\FrameXML\UnitFrame.xml",
        "Interface\\FrameXML\\PlayerFrame.xml",
    ] {
        load_xml(&s, &f);
    }
    s
}

#[test]
fn stock_player_frame_host_smoke_test() {
    benilla_formats::wow_data_or_skip!();
    let mut s = player_frame_script();

    // 1. The frame exists and the stock health bar reads the unit it is anchored to.
    assert!(
        s.eval::<bool>(r#"return PlayerFrame ~= nil and PlayerHealthBar ~= nil"#)
            .unwrap(),
        "the stock PlayerFrame and its health bar loaded"
    );

    // 2. Manual event-delivery smoke: fire `UNIT_HEALTH` with the stock player token and read
    //    the host's UnitHealth path. This does not prove the session produced the event.
    s.run(r#"
        seen_event = nil
        seen_unit = nil
        seen_health = nil
        local frame = CreateFrame("Frame")
        frame:RegisterEvent("UNIT_HEALTH")
        frame:SetScript("OnEvent", function(_, event, unit)
            seen_event = event
            seen_unit = unit
            seen_health = UnitHealth("player")
        end)
        health_at_hook = UnitHealth("player")
    "#).unwrap();

    // The host has seeded unit state for the stock frame; this is a host readback, not a
    // session-fed health assertion.
    s.fire_event("UNIT_HEALTH", vec![ScriptValue::Str("player".to_owned())]);
    assert_eq!(
        s.eval::<String>("return seen_event").unwrap(),
        "UNIT_HEALTH",
        "the stock OnEvent ran with the real event name"
    );
    assert_eq!(
        s.eval::<String>("return seen_unit").unwrap(),
        "player",
        "the event argument is the fired unit token"
    );
    // The stock health query reads the host's seeded unit state, not a session update.
    let health: f64 = s.eval("return UnitHealth('player')").unwrap();
    assert!(health > 0.0, "host UnitHealth is observable: {health}");

    // 3. A stock query smoke: UnitExists('target') reads the host state. This is not a
    //    permitted production player action.
    assert!(
        !s.eval::<bool>("return UnitExists('target')").unwrap(),
        "an observed no-target must read as no target"
    );

    // 4. Same-VM CVar readback only. Cross-host persistence/reload is not asserted.
    s.run("SetCVar('floatingCombatTextCombatDamage', '0')").unwrap();
    let persisted: String = s.eval("return GetCVar('floatingCombatTextCombatDamage')").unwrap();
    assert_eq!(persisted, "0", "the cvar write is observable");

    // 5. Compatibility ledger note (issue 18): assert honestly what is NOT claimed. The
    //    Wrath-specific UnitFrame.lua of 3.3.5 loads stock; any API gap encountered would
    //    fail this test rather than be stubbed. Record what loaded for the ledger.
    let errors = s.errors();
    assert!(
        errors.is_empty(),
        "stock file evaluation must be error-free, got: {errors:?}"
    );
}
