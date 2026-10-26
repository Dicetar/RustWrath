//! Immutable client contract, independent of whether the VM hosts glue or in-game scripts.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTarget {
    #[default]
    Vanilla112,
    Wrath335,
}

impl UiTarget {
    pub(crate) fn events(self, vm: super::VmKind) -> impl Iterator<Item = &'static str> {
        let extra: &'static [&'static str] = match (self, vm) {
            (Self::Wrath335, super::VmKind::InGame) => WRATH_CAST_EVENTS,
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

// The 3.3.5 CastingBarFrame's subscriptions plus the success event used by installed combatlogfix.
// Producers remain separate; admitting a listener must not invent a cast, identity or timestamp.
const WRATH_CAST_EVENTS: &[&str] = &[
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
];
