//! Stock UI-host smoke test (R11): load the selected 3.3.5a archive profile's real FrameXML
//! prefix through PlayerFrame.xml in TOC order, manually
//! fire `UNIT_HEALTH`, query `UnitExists`, and read back a CVar in one VM. It does not prove a
//! session-driven event, a production player action, cross-host persistence or an installed
//! addon or loose-file overlay; those remain separate acceptance work. Missing profile/data
//! admission skips through the standard data gate; BENILLA_REQUIRE_DATA=1 makes that a failure.
//! Acceptance runs must set that flag plus WM_ASSET_MANIFEST/WM_ASSET_PROFILE and retain output.
//! No shared test-loader stand-ins or replacement Lua APIs are installed by this fixture.

use benilla_ui::script::{ScriptValue, UiScript, UiTarget, UnitGuids, UnitState};

/// A fresh chain avoids the shared loader's cached source and its vanilla fixture stand-ins.
fn player_frame_script() -> Option<UiScript> {
    let Some(manifest) = std::env::var_os("WM_ASSET_MANIFEST") else {
        benilla_formats::skipped("Wrath UI smoke needs an explicit WM_ASSET_MANIFEST", &[]);
        return None;
    };
    let manifest = std::path::PathBuf::from(manifest);
    let profile = std::env::var("WM_ASSET_PROFILE")
        .expect("selected Wrath UI smoke manifest requires WM_ASSET_PROFILE");
    assert!(!profile.trim().is_empty(), "Wrath UI profile must be named");
    let data = benilla_formats::wow_data_or_skip!(None);
    let mut chain = benilla_assets::open_profile_chain(&data, &manifest, &profile)
        .unwrap_or_else(|error| panic!("Wrath UI profile {profile} / {}: {error:#}", manifest.display()));
    let toc_bytes = chain.read(super::reference_ui::TOC)
        .expect("selected Wrath profile must supply FrameXML.toc");
    let toc = benilla_ui::toc::Toc::parse(&benilla_ui::source::decode(&toc_bytes));
    assert_eq!(toc.interface_version(), 30300, "selected FrameXML must be the Wrath interface");
    let last = toc.files.iter().position(|path|
        path.replace('\\', "/").rsplit('/').next()
            .is_some_and(|leaf| leaf.eq_ignore_ascii_case("PlayerFrame.xml")))
        .expect("selected FrameXML.toc must list PlayerFrame.xml");

    let mut s = UiScript::with_target(UiTarget::Wrath335).unwrap();
    assert_eq!(s.target(), UiTarget::Wrath335, "explicit Wrath host");
    s.keep_calls();
    s.set_screen_size(1024.0, 768.0);
    s.set_text_measurer(Box::new(super::FixedWidthFont(7.0)));
    let mut cvars = crate::cvars::Cvars::default();
    super::lifecycle::register_wrath_cvars(&mut cvars);
    cvars.seed_vm(&mut s);
    s.register_cvars(crate::cvars::registered_pairs());
    crate::chr_classes::seat_localized_class_names_from_chain(&mut s, &mut chain)
        .expect("Wrath UI smoke must seat a complete localized class catalog before Constants.lua");
    // The played character the smoke reads back: the host's own unit state, pushed the way the
    // app pushes it at login, so the stock unit verbs answer what a session would.
    s.set_unit_guids(&UnitGuids {
        player: 0x10,
        wrath: true,
        ..Default::default()
    });
    s.set_unit(
        "player",
        Some(UnitState {
            exists: true,
            has_object: true,
            guid: 0x10,
            name: Some("Smoke".to_owned()),
            health: 900,
            max_health: 1000,
            level: 60,
            ..Default::default()
        }),
    );
    eprintln!("WRATH_UI_SMOKE_DATA target={:?} interface=30300 profile={profile} manifest={} data={}",
        s.target(), manifest.display(), data.display());
    let toc_path = super::reference_ui::TOC.replace('\\', "/");
    let (directory, _) = toc_path.rsplit_once('/').expect("FrameXML TOC has a directory");
    for entry in &toc.files[..=last] {
        // The host's permanent skip: benilla has no voice chat (`reference_ui::WRATH_SKIPPED_FILES`).
        if super::reference_ui::wrath_skipped(entry) {
            continue;
        }
        let path = benilla_ui::loader::join_ref(directory, entry);
        let bytes = chain.read(&path).unwrap_or_else(|error| panic!("{path}: {error:#}"));
        if path.to_ascii_lowercase().ends_with(".lua") {
            s.run_chunk_named(&bytes, &format!("@{path}"))
                .unwrap_or_else(|error| panic!("{path}: {error}"));
        } else {
            let doc = benilla_ui::framexml::parse(&benilla_ui::source::decode(&bytes))
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            let report = benilla_ui::loader::load_in(&s, &doc, &path, &|request| chain.read(request).ok());
            assert!(report.errors.is_empty(), "{path}: loader errors: {:?}", report.errors);
            assert!(report.missing_files.is_empty(), "{path}: missing files: {:?}", report.missing_files);
            // The wrath FrameXML text sinks reach for keys the FrameXML locale stream no longer
            // defines (`OptionsFrameTemplates.xml`'s OPTIONS header, `BattlefieldFrame.xml`'s
            // BATTLEGROUND_LORETEXT_1); the real client answers a miss with an empty GetText and
            // shows the raw key, so benilla's key-miss warning is the faithful outcome. The one
            // other expected notice is the template the permanently skipped voice chat owned
            // (`PlayerFrame.xml` inherits `VoiceChatSpeakerTemplate` from `VoiceChat.xml`); the
            // speaker stays a plain hidden frame, as a voice-disabled client's UI shows nothing.
            for warning in &report.warnings {
                assert!(
                    warning.ends_with(
                        "is shaped like a GlobalStrings key but no such string global exists — \
                         showing the key"
                    ) || warning
                        == "unknown template 'VoiceChatSpeakerTemplate' referenced by inherits; \
                            skipping",
                    "{path}: loader warnings: {warning:?}"
                );
            }
        }
    }
    Some(s)
}

#[test]
fn stock_player_frame_host_smoke_test() {
    let Some(mut s) = player_frame_script() else { return; };

    for name in ["targetStatusText", "partyStatusText", "petStatusText", "threatShowNumeric"] {
        let value: String = s.eval(&format!("return GetCVar('{name}')")).unwrap();
        assert_eq!(value, "0", "Wrath startup CVar {name} must be registered");
    }

    // 1. The frame exists and the stock health bar reads the unit it is anchored to.
    assert!(
        s.eval::<bool>(r#"return PlayerFrame ~= nil and PlayerFrameHealthBar ~= nil"#)
            .unwrap(),
        "the stock PlayerFrame and its health bar loaded"
    );
    assert!(
        s.eval::<bool>(
            "return PlayerHitIndicator ~= nil and PlayerFrame.feedbackText == PlayerHitIndicator"
        ).unwrap(),
        "PlayerFrame_OnLoad must finish combat feedback initialization"
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
    //    `CombatDamage` is the 1.12 host's registered master row (the wrath-era name
    //    `floatingCombatTextCombatDamage` is not a 1.12 registration).
    s.run("SetCVar('CombatDamage', '0')").unwrap();
    let persisted: String = s.eval("return GetCVar('CombatDamage')").unwrap();
    assert_eq!(persisted, "0", "the cvar write is observable");

    // 5. Compatibility ledger note (issue 18): assert honestly what is NOT claimed. The
    //    Wrath-specific UnitFrame.lua of 3.3.5 loads stock; any API gap encountered would
    //    fail this test rather than be stubbed. Record what loaded for the ledger.
    let errors = s.errors();
    assert!(
        errors.is_empty(),
        "stock file evaluation must be error-free, got: {errors:?}"
    );

    // 6. Stock animation engine: the TutorialFrame call-out's named Alpha, built by the XML
    //    loader from the real prefix, plays on the host clock, moves its owner's alpha, and
    //    fires OnFinished once. This runs after the load-error assertion because a tick also
    //    runs stock OnUpdate handlers whose later TOC files this prefix deliberately omits.
    s.run(r#"
        assert(TutorialFrameCallOutPulser, "the stock pulser animation is built")
        ANIM_FINISHED = 0
        TutorialFrameCallOutPulser:SetDuration(0.01)
        TutorialFrameCallOutPulser:SetScript("OnFinished", function()
            ANIM_FINISHED = ANIM_FINISHED + 1
        end)
        TutorialFrameCallOutPulser:Play()
        assert(TutorialFrameCallOutPulser:IsPlaying(), "the stock pulser reports playing")
    "#).expect("the stock pulser animation exists and plays");
    s.tick(0.02);
    assert_eq!(
        s.eval::<i64>("return ANIM_FINISHED").unwrap(),
        1,
        "the stock Alpha fired OnFinished once"
    );
    let callout_alpha: f64 = s
        .eval("return TutorialFrameCallOutPulser:GetRegionParent():GetAlpha()")
        .unwrap();
    assert!(
        callout_alpha < 1.0 && callout_alpha > 0.2,
        "the stock Alpha moved its owner's alpha: {callout_alpha}"
    );
}
