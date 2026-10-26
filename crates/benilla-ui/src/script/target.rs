//! Immutable client contract, independent of whether the VM hosts glue or in-game scripts.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiTarget {
    #[default]
    Vanilla112,
    Wrath335,
}

impl UiTarget {
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
