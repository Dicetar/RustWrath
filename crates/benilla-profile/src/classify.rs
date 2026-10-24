//! Failure classification across two profiles (plan rev 3, P0.4/P2.3):
//!
//! A profile-dependent failure implicates overrides, precedence or interactions — it is *not*
//! evidence that a custom asset is defective. A failure that persists in both profiles is a
//! parser/missing-dependency/locale hypothesis — it does not uniquely identify a stock-format
//! defect either. The classifier states where a path lives; it never assigns blame downstream.

use crate::{Origin, Profile, Status};

/// One side of a comparison, annotated with the winning archive's declared origin.
#[derive(Clone, Debug)]
pub struct Side {
    pub status: Status,
    pub origin: Option<Origin>,
}

/// The classification of one internal path across the base and wm-hd profiles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Readable in both; a parse failure here is shared — stock-format or parser territory.
    Shared,
    /// Readable only through wm-origin archives: override content, precedence or interaction.
    WmOnly,
    /// Present in the wm-hd profile only via stock-origin archives it still loads.
    StockOnly,
    /// In neither profile: a missing dependency, not an HD/stock question.
    Missing,
    /// Tombstoned by the winning archive of a profile that has the path at all.
    Tombstoned,
}

/// Compare one internal path across two opened profiles.
pub fn classify(base: &Profile, hd: &Profile, name: &str) -> (Class, Side, Side) {
    let a = side(base, name);
    let b = side(hd, name);
    (classify_sides(&a, &b), a, b)
}

/// The pure core: classify a path from its two profile sides. Testable without archives.
pub fn classify_sides(base: &Side, hd: &Side) -> Class {
    use Status::{Found, Missing, Tombstoned};
    match (&base.status, &hd.status) {
        (Missing, Missing) | (Missing, Tombstoned { .. }) | (Tombstoned { .. }, Missing) => Class::Missing,
        (Missing, Found { .. }) => match hd.origin {
            Some(Origin::Wm) => Class::WmOnly,
            Some(Origin::Stock) | None => Class::StockOnly,
        },
        // Found in base but missing from wm-hd is a manifest-coverage gap, not an HD/stock
        // question; the sides carry the asymmetry, the class stays stock-side.
        (Found { .. }, Missing)
        | (Found { .. }, Found { .. })
        | (Found { .. }, Tombstoned { .. })
        | (Tombstoned { .. }, Found { .. }) => Class::Shared,
        (Tombstoned { .. }, Tombstoned { .. }) => Class::Tombstoned,
    }
}

fn side(profile: &Profile, name: &str) -> Side {
    Side { status: profile.status(name), origin: profile.origin_of(name) }
}
