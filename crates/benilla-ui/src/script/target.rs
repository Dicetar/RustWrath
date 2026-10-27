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
            (Self::Wrath335, super::VmKind::InGame) => WRATH_INGAME_EVENTS,
            _ => &[],
        };
        vm.events().iter().copied().chain(extra.iter().copied())
    }

    pub(crate) fn event_key(self, vm: super::VmKind, name: &str) -> Option<&'static str> {
        self.events(vm).find(|event| event.eq_ignore_ascii_case(name))
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
];
