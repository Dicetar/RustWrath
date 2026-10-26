//! Explicit language selection. Donor constructors continue to select Vanilla50.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Vanilla50,
    Wrath51,
}

impl Dialect {
    pub(crate) fn is_51(self) -> bool { self == Self::Wrath51 }
}
