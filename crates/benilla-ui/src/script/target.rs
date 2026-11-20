//! Immutable client contract, independent of whether the VM hosts glue or in-game scripts.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTarget {
    #[default]
    Vanilla112,
    Wrath335,
}

impl UiTarget {
    pub(crate) fn locale_names(self) -> &'static [&'static str] {
        match self {
            Self::Vanilla112 => &crate::toc::LOCALES,
            // AzerothCore 12340 Common.cpp localeNames, including the Wrath-only indices.
            Self::Wrath335 => &[
                "enUS", "koKR", "frFR", "deDE", "zhCN", "zhTW", "esES", "esMX", "ruRU",
            ],
        }
    }

    pub(crate) fn events(self, vm: super::VmKind) -> impl Iterator<Item = &'static str> {
        let extra: &'static [&'static str] = match (self, vm) {
            (Self::Wrath335, super::VmKind::Glue) => &["SERVER_SPLIT_NOTICE"],
            (Self::Wrath335, super::VmKind::InGame) => WRATH_INGAME_EVENTS,
            _ => &[],
        };
        vm.events().iter().copied().chain(extra.iter().copied())
    }

    pub(crate) fn event_key(self, vm: super::VmKind, name: &str) -> Option<&'static str> {
        self.events(vm)
            .find(|event| event.eq_ignore_ascii_case(name))
    }

    pub(crate) fn inheritance(self) -> crate::framexml::Inheritance {
        match self {
            Self::Vanilla112 => crate::framexml::Inheritance::Single,
            Self::Wrath335 => crate::framexml::Inheritance::List,
        }
    }

    pub const fn version(self) -> &'static str {
        match self {
            Self::Vanilla112 => super::client::VERSION,
            Self::Wrath335 => "3.3.5",
        }
    }

    pub const fn build(self) -> &'static str {
        match self {
            Self::Vanilla112 => super::client::BUILD,
            Self::Wrath335 => "12340",
        }
    }

    pub const fn build_date(self) -> &'static str {
        match self {
            Self::Vanilla112 => super::client::BUILD_DATE,
            Self::Wrath335 => "Jun 24 2010",
        }
    }

    pub const fn interface(self) -> u32 {
        match self {
            Self::Vanilla112 => 11200,
            Self::Wrath335 => 30300,
        }
    }

    pub(super) fn dialect(self) -> mlua::benilla_lua::Dialect {
        match self {
            Self::Vanilla112 => mlua::benilla_lua::Dialect::Vanilla50,
            Self::Wrath335 => mlua::benilla_lua::Dialect::Wrath51,
        }
    }
}

// Observed subscriptions in the pinned 3.3.5 FrameXML (wowgaming/3.3.5-interface-files,
// d0339b17b0221db76e6acd2dc2915d224a5b62ca), plus installed combatlogfix's success event.
// This extends listener admission, not native event IDs or producer/payload coverage.
const WRATH_INGAME_EVENTS: &[&str] = &[
    "UNIT_RUNIC_POWER",
    "UNIT_MAXRUNIC_POWER",
    "UNIT_SPELLCAST_START",
    "UNIT_SPELLCAST_STOP",
    "UNIT_SPELLCAST_SUCCEEDED",
    "UNIT_SPELLCAST_FAILED",
    "UNIT_SPELLCAST_INTERRUPTED",
    "UNIT_SPELLCAST_DELAYED",
    "UNIT_SPELLCAST_CHANNEL_START",
    "UNIT_SPELLCAST_CHANNEL_UPDATE",
    "UNIT_SPELLCAST_CHANNEL_STOP",
    "UNIT_SPELLCAST_INTERRUPTIBLE",
    "UNIT_SPELLCAST_NOT_INTERRUPTIBLE",
    // UIParent and Player/Unit/CastingBar/Buff/Rune/Pet/Target/PartyMember/Focus frames.
    // Names already present in the donor catalog are deliberately not repeated here.
    "ACHIEVEMENT_EARNED",
    "ADDON_ACTION_BLOCKED",
    "ARENA_TEAM_INVITE_REQUEST",
    "AUCTION_HOUSE_DISABLED",
    "BARBER_SHOP_CLOSE",
    "BARBER_SHOP_OPEN",
    "CANCEL_SUMMON",
    "CHANNEL_INVITE_REQUEST",
    "CHANNEL_PASSWORD_REQUEST",
    // Blizzard_CombatLog.lua registers COMBAT_LOG_EVENT alongside COMBAT_LOG_EVENT_UNFILTERED,
    // and WMBridge consumes PARTY_KILL through the unfiltered combat-log event
    // (`UnitCombatLog_C.cpp:866-887`); the host emits it from the decoded kill packet.
    "COMBAT_LOG_EVENT",
    "COMBAT_LOG_EVENT_UNFILTERED",
    "CONFIRM_DISENCHANT_ROLL",
    "DISABLE_TAXI_BENCHMARK",
    "ENABLE_TAXI_BENCHMARK",
    "END_BOUND_TRADEABLE",
    "END_REFUND",
    "GOSSIP_CONFIRM",
    "GOSSIP_CONFIRM_CANCEL",
    "GUILDBANKFRAME_CLOSED",
    "GUILDBANKFRAME_OPENED",
    "INSTANCE_ENCOUNTER_ENGAGE_UNIT",
    "INSTANCE_LOCK_START",
    "INSTANCE_LOCK_STOP",
    "LEVEL_GRANT_PROPOSED",
    "MACRO_ACTION_BLOCKED",
    "MUTELIST_UPDATE",
    "PET_RENAMEABLE",
    "PLAYER_FOCUS_CHANGED",
    "PLAYER_ROLES_ASSIGNED",
    // QuestLogFrame.lua:260-264 handles this accept edge; the 1.12 table predates the event.
    "QUEST_ACCEPTED",
    "RAID_INSTANCE_WELCOME",
    "RAISED_AS_GHOUL",
    "READY_CHECK_CONFIRM",
    "READY_CHECK_FINISHED",
    "RUNE_POWER_UPDATE",
    "RUNE_TYPE_UPDATE",
    "SOCKET_INFO_UPDATE",
    "TALENTS_INVOLUNTARILY_RESET",
    "UNIT_ENTERED_VEHICLE",
    "UNIT_ENTERING_VEHICLE",
    "UNIT_EXITED_VEHICLE",
    "UNIT_EXITING_VEHICLE",
    "UNIT_THREAT_SITUATION_UPDATE",
    "USE_GLYPH",
    "VOICE_PUSH_TO_TALK_START",
    "VOICE_PUSH_TO_TALK_STOP",
    "VOICE_START",
    "VOICE_STATUS_UPDATE",
    "VOICE_STOP",
    "WOW_MOUSE_NOT_FOUND",
    // The rest of the 12340 in-game event table (`Ui/Types.h` EVENT ids 0-721) the donor catalog
    // and the observed list above lack: without them `RegisterEvent` admitted no listener and
    // every signal of these names reached no frame (the multisell bar, quest POIs, companions).
    "UNIT_TARGET",
    "UNIT_COMBO_POINTS",
    "PLAYER_TOTEM_UPDATE",
    "MINIMAP_UPDATE_TRACKING",
    "CHAT_MSG_MONSTER_PARTY",
    "CHAT_MSG_OPENING",
    "CHAT_MSG_TRADESKILLS",
    "CHAT_MSG_PET_INFO",
    "CHAT_MSG_RAID_BOSS_WHISPER",
    "CHAT_MSG_RESTRICTED",
    "CHAT_MSG_ACHIEVEMENT",
    "CHAT_MSG_GUILD_ACHIEVEMENT",
    "LOOT_SLOT_CHANGED",
    "TRAINER_DESCRIPTION_UPDATE",
    "UNIT_SPELLCAST_SENT",
    "UNIT_SPELLCAST_FAILED_QUIET",
    "PET_BAR_HIDE",
    "PET_BAR_UPDATE_USABLE",
    "UPDATE_SHAPESHIFT_FORM",
    "UPDATE_SHAPESHIFT_USABLE",
    "UPDATE_SHAPESHIFT_COOLDOWN",
    "TRADE_POTENTIAL_BIND_ENCHANT",
    "MAIL_LOCK_SEND_ITEMS",
    "MAIL_UNLOCK_SEND_ITEMS",
    "UPDATE_INVENTORY_DURABILITY",
    "UPDATE_LFG_LIST_INCREMENTAL",
    "SOCKET_INFO_CLOSE",
    "PETITION_VENDOR_SHOW",
    "PETITION_VENDOR_CLOSED",
    "PETITION_VENDOR_UPDATE",
    "KNOWLEDGE_BASE_SETUP_LOAD_SUCCESS",
    "KNOWLEDGE_BASE_SETUP_LOAD_FAILURE",
    "KNOWLEDGE_BASE_QUERY_LOAD_SUCCESS",
    "KNOWLEDGE_BASE_QUERY_LOAD_FAILURE",
    "KNOWLEDGE_BASE_ARTICLE_LOAD_SUCCESS",
    "KNOWLEDGE_BASE_ARTICLE_LOAD_FAILURE",
    "KNOWLEDGE_BASE_SYSTEM_MOTD_UPDATED",
    "KNOWLEDGE_BASE_SERVER_MESSAGE",
    "ARENA_TEAM_UPDATE",
    "ARENA_TEAM_ROSTER_UPDATE",
    "HONOR_CURRENCY_UPDATE",
    "KNOWN_TITLES_UPDATE",
    "NEW_TITLE_EARNED",
    "OLD_TITLE_LOST",
    "LFG_UPDATE",
    "LFG_PROPOSAL_UPDATE",
    "LFG_PROPOSAL_SHOW",
    "LFG_PROPOSAL_FAILED",
    "LFG_PROPOSAL_SUCCEEDED",
    "LFG_ROLE_UPDATE",
    "LFG_ROLE_CHECK_UPDATE",
    "LFG_ROLE_CHECK_SHOW",
    "LFG_ROLE_CHECK_HIDE",
    "LFG_ROLE_CHECK_ROLE_CHOSEN",
    "LFG_QUEUE_STATUS_UPDATE",
    "LFG_BOOT_PROPOSAL_UPDATE",
    "LFG_LOCK_INFO_RECEIVED",
    "LFG_UPDATE_RANDOM_INFO",
    "LFG_OFFER_CONTINUE",
    "LFG_OPEN_FROM_GOSSIP",
    "LFG_COMPLETION_REWARD",
    "PARTY_LFG_RESTRICTED",
    "COMBAT_RATING_UPDATE",
    "MODIFIER_STATE_CHANGED",
    "UPDATE_STEALTH",
    "VOICE_CHANNEL_STATUS_UPDATE",
    "UPDATE_FLOATING_CHAT_WINDOWS",
    "MOVIE_RECORDING_PROGRESS",
    "MOVIE_COMPRESSING_PROGRESS",
    "MOVIE_UNCOMPRESSED_MOVIE",
    "GUILDBANKBAGSLOTS_CHANGED",
    "GUILDBANK_ITEM_LOCK_CHANGED",
    "GUILDBANK_UPDATE_TABS",
    "GUILDBANK_UPDATE_MONEY",
    "GUILDBANKLOG_UPDATE",
    "GUILDBANK_UPDATE_WITHDRAWMONEY",
    "GUILDBANK_UPDATE_TEXT",
    "GUILDBANK_TEXT_CHANGED",
    "CHANNEL_UI_UPDATE",
    "CHANNEL_COUNT_UPDATE",
    "CHANNEL_ROSTER_UPDATE",
    "CHANNEL_VOICE_UPDATE",
    "CHANNEL_FLAGS_UPDATED",
    "VOICE_SESSIONS_UPDATE",
    "VOICE_CHAT_ENABLED_UPDATE",
    "VOICE_LEFT_SESSION",
    "INSPECT_TALENT_READY",
    "VOICE_SELF_MUTE",
    "VOICE_PLATE_START",
    "VOICE_PLATE_STOP",
    "ARENA_SEASON_WORLD_STATE",
    "GUILD_EVENT_LOG_UPDATE",
    "GUILDTABARD_UPDATE",
    "SOUND_DEVICE_UPDATE",
    "COMMENTATOR_MAP_UPDATE",
    "COMMENTATOR_ENTER_WORLD",
    "COMMENTATOR_PLAYER_UPDATE",
    "PLAYER_ENTERING_BATTLEGROUND",
    "BARBER_SHOP_SUCCESS",
    "BARBER_SHOP_APPEARANCE_APPLIED",
    "CALENDAR_UPDATE_INVITE_LIST",
    "CALENDAR_UPDATE_EVENT_LIST",
    "CALENDAR_NEW_EVENT",
    "CALENDAR_OPEN_EVENT",
    "CALENDAR_CLOSE_EVENT",
    "CALENDAR_UPDATE_EVENT",
    "CALENDAR_UPDATE_PENDING_INVITES",
    "CALENDAR_EVENT_ALARM",
    "CALENDAR_UPDATE_ERROR",
    "CALENDAR_ACTION_PENDING",
    "VEHICLE_ANGLE_SHOW",
    "VEHICLE_ANGLE_UPDATE",
    "VEHICLE_POWER_SHOW",
    "VEHICLE_PASSENGERS_CHANGED",
    "PLAYER_GAINS_VEHICLE_DATA",
    "PLAYER_LOSES_VEHICLE_DATA",
    "PET_FORCE_NAME_DECLENSION",
    "SYNCHRONIZE_SETTINGS",
    "PLAY_MOVIE",
    "CRITERIA_UPDATE",
    "RECEIVED_ACHIEVEMENT_LIST",
    "KNOWN_CURRENCY_TYPES_UPDATE",
    "CURRENCY_DISPLAY_UPDATE",
    "COMPANION_LEARNED",
    "COMPANION_UNLEARNED",
    "COMPANION_UPDATE",
    "UNIT_THREAT_LIST_UPDATE",
    "GLYPH_ADDED",
    "GLYPH_REMOVED",
    "GLYPH_UPDATED",
    "GLYPH_ENABLED",
    "GLYPH_DISABLED",
    "TRACKED_ACHIEVEMENT_UPDATE",
    "ARENA_OPPONENT_UPDATE",
    "INSPECT_ACHIEVEMENT_READY",
    "PARTY_CONVERTED_TO_RAID",
    "PVPQUEUE_ANYWHERE_SHOW",
    "PVPQUEUE_ANYWHERE_UPDATE_AVAILABLE",
    "PLAYER_TALENT_UPDATE",
    "ACTIVE_TALENT_GROUP_CHANGED",
    "PET_TALENT_UPDATE",
    "PREVIEW_TALENT_POINTS_CHANGED",
    "PREVIEW_PET_TALENT_POINTS_CHANGED",
    "WEAR_EQUIPMENT_SET",
    "EQUIPMENT_SETS_CHANGED",
    "PLAYER_EQUIPMENT_CHANGED",
    "ITEM_LOCKED",
    "ITEM_UNLOCKED",
    "TRADE_SKILL_FILTER_UPDATE",
    "EQUIPMENT_SWAP_PENDING",
    "EQUIPMENT_SWAP_FINISHED",
    "NPC_PVPQUEUE_ANYWHERE",
    "UPDATE_MULTI_CAST_ACTIONBAR",
    "ENABLE_XP_GAIN",
    "DISABLE_XP_GAIN",
    "BATTLEFIELD_MGR_ENTRY_INVITE",
    "BATTLEFIELD_MGR_ENTERED",
    "BATTLEFIELD_MGR_QUEUE_REQUEST_RESPONSE",
    "BATTLEFIELD_MGR_EJECT_PENDING",
    "BATTLEFIELD_MGR_EJECTED",
    "BATTLEFIELD_MGR_QUEUE_INVITE",
    "BATTLEFIELD_MGR_STATE_CHANGE",
    "WORLD_STATE_UI_TIMER_UPDATE",
    "UPDATE_CHAT_COLOR_NAME_BY_CLASS",
    "GMRESPONSE_RECEIVED",
    "VEHICLE_UPDATE",
    "MAIL_SUCCESS",
    "QUEST_QUERY_COMPLETE",
    "QUEST_POI_UPDATE",
    "PLAYER_DIFFICULTY_CHANGED",
    "CHAT_MSG_PARTY_LEADER",
    "VOTE_KICK_REASON_NEEDED",
    "ENABLE_LOW_LEVEL_RAID",
    "DISABLE_LOW_LEVEL_RAID",
    "CHAT_MSG_TARGETICONS",
    "AUCTION_MULTISELL_START",
    "AUCTION_MULTISELL_UPDATE",
    "AUCTION_MULTISELL_FAILURE",
    "PET_SPELL_POWER_UPDATE",
    "BN_CONNECTED",
    "BN_DISCONNECTED",
    "BN_SELF_ONLINE",
    "BN_SELF_OFFLINE",
    "BN_FRIEND_LIST_SIZE_CHANGED",
    "BN_FRIEND_INVITE_LIST_INITIALIZED",
    "BN_FRIEND_INVITE_SEND_RESULT",
    "BN_FRIEND_INVITE_ADDED",
    "BN_FRIEND_INVITE_REMOVED",
    "BN_FRIEND_INFO_CHANGED",
    "BN_CUSTOM_MESSAGE_CHANGED",
    "BN_CUSTOM_MESSAGE_LOADED",
    "CHAT_MSG_BN_WHISPER",
    "CHAT_MSG_BN_WHISPER_INFORM",
    "BN_CHAT_WHISPER_UNDELIVERABLE",
    "BN_CHAT_CHANNEL_JOINED",
    "BN_CHAT_CHANNEL_LEFT",
    "BN_CHAT_CHANNEL_CLOSED",
    "CHAT_MSG_BN_CONVERSATION",
    "CHAT_MSG_BN_CONVERSATION_NOTICE",
    "CHAT_MSG_BN_CONVERSATION_LIST",
    "BN_CHAT_CHANNEL_MESSAGE_UNDELIVERABLE",
    "BN_CHAT_CHANNEL_MESSAGE_BLOCKED",
    "BN_CHAT_CHANNEL_MEMBER_JOINED",
    "BN_CHAT_CHANNEL_MEMBER_LEFT",
    "BN_CHAT_CHANNEL_MEMBER_UPDATED",
    "BN_CHAT_CHANNEL_CREATE_SUCCEEDED",
    "BN_CHAT_CHANNEL_CREATE_FAILED",
    "BN_CHAT_CHANNEL_INVITE_SUCCEEDED",
    "BN_CHAT_CHANNEL_INVITE_FAILED",
    "BN_BLOCK_LIST_UPDATED",
    "BN_SYSTEM_MESSAGE",
    "BN_REQUEST_FOF_SUCCEEDED",
    "BN_REQUEST_FOF_FAILED",
    "BN_NEW_PRESENCE",
    "BN_TOON_NAME_UPDATED",
    "BN_FRIEND_ACCOUNT_ONLINE",
    "BN_FRIEND_ACCOUNT_OFFLINE",
    "BN_FRIEND_TOON_ONLINE",
    "BN_FRIEND_TOON_OFFLINE",
    "BN_MATURE_LANGUAGE_FILTER",
    "COMMENTATOR_SKIRMISH_QUEUE_REQUEST",
    "COMMENTATOR_SKIRMISH_MODE_REQUEST",
    "CHAT_MSG_BN_INLINE_TOAST_ALERT",
    "CHAT_MSG_BN_INLINE_TOAST_BROADCAST",
    "CHAT_MSG_BN_INLINE_TOAST_BROADCAST_INFORM",
    "CHAT_MSG_BN_INLINE_TOAST_CONVERSATION",
];

#[cfg(test)]
mod wrath_regressions {
    use super::UiTarget;
    use crate::script::{LocalizedClassName, QuadContent, UiScript};

    fn host() -> UiScript {
        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        script.set_screen_size(800.0, 600.0);
        script
    }

    fn class(id: u8, file: &str, male: &str, female: &str) -> LocalizedClassName {
        LocalizedClassName {
            id,
            file: file.into(),
            male: male.into(),
            female: female.into(),
        }
    }

    #[test]
    fn observed_class_labels_preserve_tables_and_reject_invalid_replacement() {
        let mut script = host();
        assert!(script.run("FillLocalizedClassList({})").is_err());
        script
            .set_localized_class_names(0, vec![class(1, "WARRIOR", "male", "female")])
            .unwrap();
        script
            .run(
                r#"
            local male, female = { keep = 42 }, {}
            assert(FillLocalizedClassList(male, false) == male)
            assert(FillLocalizedClassList(female, true) == female)
            assert(male.keep == 42 and male.WARRIOR == "male" and female.WARRIOR == "female")
        "#,
            )
            .unwrap();
        assert!(script
            .set_localized_class_names(
                0,
                vec![
                    class(1, "WARRIOR", "wrong", "wrong"),
                    class(1, "MAGE", "wrong", "wrong"),
                ]
            )
            .is_err());
        assert_eq!(
            script
                .eval::<String>("return FillLocalizedClassList({}).WARRIOR")
                .unwrap(),
            "male"
        );
        script.set_locale(8);
        assert_eq!(script.eval::<String>("return GetLocale()").unwrap(), "ruRU");
        assert!(script.run("FillLocalizedClassList({})").is_err());
    }

    #[test]
    fn wrath_table_boundaries_and_binary_formats_follow_the_selected_dialect() {
        let script = host();
        script
            .run(
                r#"
            local t = { "one", "two", "three" }
            assert(select('#', table.remove(t, 99)) == 0 and #t == 3 and t[3] == "three")
            assert(select('#', table.remove(t, 0)) == 0 and #t == 3)
            assert(not pcall(table.insert, t, 1, "bad", "surplus") and #t == 3)
            assert(table.concat(t, ',', 1, 0) == "")
            assert(format("a\000b") == "a\000b")
            assert(format("%.s", strrep("x", 100)) == "")
            assert(format("%q", "\r") == '"\\r"')
            assert(format("%10$s", "1", "2", "3", "4", "5", "6", "7", "8", "9", "ten") == "ten")
            assert(format("%F", 1.25) == format("%f", 1.25))
        "#,
            )
            .unwrap();
    }

    #[test]
    fn event_queries_keep_wrapper_identity_order_and_unregister_membership() {
        let script = host();
        script
            .run(
                r#"
            local a, b = CreateFrame("Frame"), CreateFrame("Frame")
            a:RegisterEvent("CHAT_MSG_WHISPER"); b:RegisterEvent("CHAT_MSG_WHISPER")
            local first, second = GetFramesRegisteredForEvent("chat_msg_whisper")
            assert(first == a and second == b and a:IsEventRegistered("chat_msg_whisper"))
            a:UnregisterEvent("CHAT_MSG_WHISPER"); a:RegisterEvent("CHAT_MSG_WHISPER")
            first, second = GetFramesRegisteredForEvent("CHAT_MSG_WHISPER")
            assert(first == b and second == a)
            a:UnregisterAllEvents(); b:UnregisterAllEvents()
            assert(not a:IsEventRegistered("CHAT_MSG_WHISPER"))
            assert(select('#', GetFramesRegisteredForEvent("CHAT_MSG_WHISPER")) == 0)
            a:RegisterAllEvents()
            assert(a:IsEventRegistered("RUNE_POWER_UPDATE"))
            assert(GetFramesRegisteredForEvent("RUNE_POWER_UPDATE") == a)
            a:UnregisterAllEvents()
            assert(select('#', GetFramesRegisteredForEvent("RUNE_POWER_UPDATE")) == 0)
        "#,
            )
            .unwrap();
    }

    #[test]
    fn wrath_admits_the_combat_log_event_for_addons() {
        let script = host();
        script
            .run(
                r#"
            local listener = CreateFrame("Frame", "WrathCombatLogAdmission")
            assert(not listener:IsEventRegistered("COMBAT_LOG_EVENT_UNFILTERED"))
            listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
            assert(listener:IsEventRegistered("COMBAT_LOG_EVENT_UNFILTERED"))
            listener:UnregisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
            assert(not listener:IsEventRegistered("COMBAT_LOG_EVENT_UNFILTERED"))
        "#,
            )
            .unwrap();
    }

    #[test]
    fn mouse_over_uses_scaled_region_geometry_without_mouse_enablement() {
        let mut script = host();
        script
            .run(
                r#"
            area = CreateFrame("Frame")
            area:SetPoint("BOTTOMLEFT", 100, 100); area:SetSize(100, 100); area:SetScale(2)
            texture = area:CreateTexture(); texture:SetAllPoints(area)
            label = area:CreateFontString(); label:SetAllPoints(area)
            assert(not area:IsMouseEnabled())
        "#,
            )
            .unwrap();
        script.resolve();
        script.mouse_move(300.0, 300.0);
        script
            .run("assert(area:IsMouseOver() and texture:IsMouseOver() and label:IsMouseOver())")
            .unwrap();
        script.mouse_move(50.0, 50.0);
        script
            .run(
                r#"
            assert(not area:IsMouseOver())
            assert(area:IsMouseOver(1000, -1000, -1000, 1000))
        "#,
            )
            .unwrap();
    }

    #[test]
    fn installed_mod_setters_reuse_visibility_and_button_state() {
        let script = host();
        script
            .run(
                r#"
            local button = CreateFrame("Button")
            local texture, label = button:CreateTexture(), button:CreateFontString()
            button:SetShown(false); assert(not button:IsShown())
            button:SetShown(0); assert(button:IsShown())
            texture:SetShown(false); label:SetShown(false)
            assert(not texture:IsShown() and not label:IsShown())
            texture:SetShown(""); label:SetShown(true)
            assert(texture:IsShown() and label:IsShown())
            button:SetEnabled(false); assert(button:IsEnabled() == 0)
            button:SetEnabled(0); assert(button:IsEnabled() == 1)
            button:SetShown(nil); assert(not button:IsShown())
        "#,
            )
            .unwrap();
    }

    #[test]
    fn disabled_motion_delivers_tooltip_scripts_without_enabling_clicks_or_visuals() {
        let mut script = host();
        script
            .run(
                r#"
            enters, leaves, clicks = 0, 0, 0
            button = CreateFrame("Button")
            button:SetPoint("BOTTOMLEFT", 0, 0); button:SetSize(100, 100)
            button:SetNormalTexture("Interface\\N.blp")
            button:SetDisabledTexture("Interface\\D.blp")
            button:SetHighlightTexture("Interface\\H.blp")
            button:SetScript("OnEnter", function() enters = enters + 1 end)
            button:SetScript("OnLeave", function() leaves = leaves + 1 end)
            button:SetScript("OnClick", function() clicks = clicks + 1 end)
            button:Disable()
        "#,
            )
            .unwrap();
        script.resolve();
        script.mouse_move(50.0, 50.0);
        script.mouse_move(200.0, 200.0);
        script.run("assert(enters == 0 and leaves == 0)").unwrap();
        script
            .run("button:SetMotionScriptsWhileDisabled(true)")
            .unwrap();
        script.mouse_move(50.0, 50.0);
        script.mouse_button(50.0, 50.0, "LeftButton", true);
        script.mouse_button(50.0, 50.0, "LeftButton", false);
        script
            .run("assert(enters == 1 and clicks == 0 and button:IsEnabled() == 0)")
            .unwrap();
        let paths: Vec<_> = script
            .extract()
            .into_iter()
            .filter_map(|quad| match quad.content {
                QuadContent::Texture {
                    path: Some(path), ..
                } => Some(path),
                _ => None,
            })
            .collect();
        assert!(paths.contains(&"Interface\\D.blp".to_string()));
        assert!(!paths.contains(&"Interface\\N.blp".to_string()));
        assert!(!paths.contains(&"Interface\\H.blp".to_string()));
        script.pointer_left_hover();
        script.run("assert(leaves == 1)").unwrap();
    }

    #[test]
    fn motion_xml_keeps_inherited_true_and_honors_explicit_false() {
        let script = host();
        let document = crate::framexml::parse(r#"<Ui>
            <Button name="MotionTemplate" virtual="true" motionScriptsWhileDisabled="true"/>
            <Button name="InheritedMotion" inherits="MotionTemplate"/>
            <Button name="OverriddenMotion" inherits="MotionTemplate" motionScriptsWhileDisabled="false"/>
        </Ui>"#).unwrap();
        let report = crate::loader::load(&script, &document, &|_| None);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        script
            .run(
                r#"
            assert(InheritedMotion:GetMotionScriptsWhileDisabled())
            assert(not OverriddenMotion:GetMotionScriptsWhileDisabled())
        "#,
            )
            .unwrap();
    }

    #[test]
    fn leave_can_admit_next_disabled_button_and_enter_can_enable_it_once() {
        let mut script = host();
        script.run(r#"
            enters, leaves = 0, 0
            old = CreateFrame("Button"); old:SetPoint("BOTTOMLEFT", 0, 0); old:SetSize(100, 100)
            nextButton = CreateFrame("Button")
            nextButton:SetPoint("BOTTOMLEFT", 200, 0); nextButton:SetSize(100, 100); nextButton:Disable()
            old:SetScript("OnLeave", function() nextButton:SetMotionScriptsWhileDisabled(true) end)
            nextButton:SetScript("OnEnter", function() enters = enters + 1; nextButton:Enable() end)
            nextButton:SetScript("OnLeave", function() leaves = leaves + 1 end)
        "#).unwrap();
        script.resolve();
        script.mouse_move(50.0, 50.0);
        script.mouse_move(250.0, 50.0);
        script
            .run("assert(enters == 1 and nextButton:IsEnabled() == 1)")
            .unwrap();
        script
            .run("nextButton:Hide(); assert(leaves == 1)")
            .unwrap();
        let vanilla = UiScript::new().unwrap();
        vanilla
            .run(
                r#"
            local button = CreateFrame("Button")
            assert(button.SetMotionScriptsWhileDisabled == nil and button.SetShown == nil)
            assert(button.IsMouseOver == nil and GetFramesRegisteredForEvent == nil)
        "#,
            )
            .unwrap();
    }
}
