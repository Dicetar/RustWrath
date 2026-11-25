//! `IsAddOnLoadable` (3.3.5a `UIUtil/AddOns.cpp`, `0x005F7E90`) as a pure function: the one
//! load law every surface consults. The first check that fires decides, in the reference's order:
//! missing, the visiting guard (a cycle is loadable), enabled, banned, corrupt, incompatible,
//! version, dependencies, demand.
//!
//! For the 3.3.5a client the law is exact:
//! - `## Interface` below 20000 is `INCOMPATIBLE`, a hard refusal that `checkAddonVersion` does
//!   not override (`AddOns.cpp:867`);
//! - any other interface other than 30300 reports `INTERFACE_VERSION`; with
//!   `checkAddonVersion` on it refuses, with the check off it falls through (`AddOns.cpp:870`);
//! - required dependencies recurse with the demand flag set, so a LoadOnDemand dependency is
//!   allowed (`AddOns.cpp:882`);
//! - the demand gate comes last: a LoadOnDemand addon refuses a normal load with
//!   `DEMAND_LOADED` and loads only through `LoadAddOn` (`AddOns.cpp:896`).
//!
//! The 11200 law below is the donor fixture's, kept only so the donor reference tests can still
//! exercise their own client. It is selected by the client interface (below 20000), never by the
//! production target.

/// The `## Interface` the 3.3.5a client implements, `GetCodeInterfaceVersion`
/// (`UIUtil/AddOns.cpp:98`).
pub const CLIENT_INTERFACE: u32 = 30300;

/// 3.3.5a refuses anything below this outright, before the version check (`AddOns.cpp:867`).
pub const MINIMUM_INTERFACE: u32 = 20000;

/// The donor 1.12 client's interface, for the donor fixtures only.
pub const DONOR_CLIENT_INTERFACE: u32 = 11200;

/// One addon as the gate reads it; every addon registry lowers into this.
pub struct GateRow<'a> {
    pub name: &'a str,
    /// This character's enable state (`AddOns.txt`; an addon nobody disabled is enabled).
    pub enabled: bool,
    /// `[rec+0x24] == 2`: the server banned it (check 4).
    pub banned: bool,
    /// `[rec+0x2a]`: its signature check answered 2 (check 5).
    pub corrupt: bool,
    /// `## Interface` as the client parses it ([crate::toc::Toc::interface_version]): the leading
    /// integer, `0` when absent.
    pub interface: u32,
    pub load_on_demand: bool,
    /// Loaded this session; a loaded dependency satisfies the recursion outright.
    pub loaded: bool,
    /// `## Dependencies` / `## RequiredDeps` / any `## Dep*` (one list in the reference).
    pub dependencies: Vec<&'a str>,
}

/// The arbiter's answer: loadable, or the two out-params.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Loadable,
    Refused {
        /// The addon's own reason token (`DISABLED`, `INCOMPATIBLE`, `DEMAND_LOADED`, …).
        reason: Option<&'static str>,
        /// A failing dependency's raw token, rendered as `DEP_<token>`.
        dep: Option<&'static str>,
    },
}

impl Verdict {
    pub fn loadable(self) -> bool {
        self == Verdict::Loadable
    }

    /// `AddOnUnloadableToken` (`AddOns.cpp:114`) with its callers' nil guard, so `LOADABLE` never
    /// renders.
    pub fn token(self) -> Option<String> {
        match self {
            Verdict::Loadable => None,
            Verdict::Refused { reason, dep } => reason
                .map(str::to_owned)
                .or_else(|| dep.map(|d| format!("DEP_{d}"))),
        }
    }
}

/// `IsAddOnLoadable` for the 3.3.5a client.
///
/// `demand_load` is the reference's `demandLoad`: true for `LoadAddOn` and the in-game
/// `GetAddOnInfo`, false for the startup walk and the glue list. `version_check` is the live
/// `checkAddonVersion`, read per query.
pub fn can_load(rows: &[GateRow], index: usize, demand_load: bool, version_check: bool) -> Verdict {
    can_load_for_interface(rows, index, demand_load, version_check, CLIENT_INTERFACE)
}

/// The same load law with the owning VM's client interface, including every dependency. An
/// interface below 20000 selects the donor 1.12 law, which exists only for donor fixtures.
pub fn can_load_for_interface(
    rows: &[GateRow],
    index: usize,
    demand_load: bool,
    version_check: bool,
    client_interface: u32,
) -> Verdict {
    let wrath = client_interface >= MINIMUM_INTERFACE;
    let mut visiting = vec![false; rows.len()];
    walk(
        rows,
        index,
        demand_load,
        version_check,
        client_interface,
        wrath,
        &mut visiting,
    )
}

/// One level of the arbiter, its checks in the reference's order.
fn walk(
    rows: &[GateRow],
    i: usize,
    demand_load: bool,
    version_check: bool,
    client_interface: u32,
    wrath: bool,
    visiting: &mut [bool],
) -> Verdict {
    // Check 2: the visiting guard; a cycle resolves as loadable.
    if visiting[i] {
        return Verdict::Loadable;
    }
    let row = &rows[i];
    // Check 3: enabled.
    if !row.enabled {
        return Verdict::Refused {
            reason: Some("DISABLED"),
            dep: None,
        };
    }
    // Checks 4 and 5: banned (reason 3) and corrupt (reason 4), in that order.
    if row.banned {
        return Verdict::Refused {
            reason: Some("BANNED"),
            dep: None,
        };
    }
    if row.corrupt {
        return Verdict::Refused {
            reason: Some("CORRUPT"),
            dep: None,
        };
    }
    if wrath {
        // Check 6: the 3.3.5a compatibility floor, a hard refusal.
        if row.interface < MINIMUM_INTERFACE {
            return Verdict::Refused {
                reason: Some("INCOMPATIBLE"),
                dep: None,
            };
        }
        // Check 7: exact 30300, overridable only by `checkAddonVersion`.
        if row.interface != client_interface && version_check {
            return Verdict::Refused {
                reason: Some("INTERFACE_VERSION"),
                dep: None,
            };
        }
    } else if row.interface != client_interface && version_check {
        // The donor 1.12 gate: exact 11200, no compatibility floor.
        return Verdict::Refused {
            reason: Some("INTERFACE_VERSION"),
            dep: None,
        };
    }
    // Check 8: required dependencies. 3.3.5a recurses with demandLoad set, so a LoadOnDemand
    // dependency is allowed; the donor law passes its own caller's flag.
    let dep_demand = if wrath { true } else { demand_load };
    visiting[i] = true;
    for dep in &row.dependencies {
        let found = rows.iter().position(|r| r.name.eq_ignore_ascii_case(dep));
        let verdict = match found {
            None => Verdict::Refused {
                reason: Some("MISSING"),
                dep: None,
            },
            Some(d) if rows[d].loaded => Verdict::Loadable,
            Some(d) => walk(
                rows,
                d,
                dep_demand,
                version_check,
                client_interface,
                wrath,
                visiting,
            ),
        };
        if let Verdict::Refused { reason, dep } = verdict {
            visiting[i] = false;
            return Verdict::Refused {
                reason: None,
                dep: reason.or(dep),
            };
        }
    }
    visiting[i] = false;
    // Check 9: the demand gate.
    if wrath {
        if !demand_load && row.load_on_demand {
            return Verdict::Refused {
                reason: Some("DEMAND_LOADED"),
                dep: None,
            };
        }
    } else if demand_load && !row.load_on_demand && !row.loaded {
        return Verdict::Refused {
            reason: Some("NOT_DEMAND_LOADED"),
            dep: None,
        };
    }
    Verdict::Loadable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(name: &'a str, deps: Vec<&'a str>) -> GateRow<'a> {
        GateRow {
            name,
            enabled: true,
            banned: false,
            corrupt: false,
            interface: CLIENT_INTERFACE,
            load_on_demand: false,
            loaded: false,
            dependencies: deps,
        }
    }

    fn donor_row<'a>(name: &'a str, deps: Vec<&'a str>) -> GateRow<'a> {
        GateRow {
            interface: DONOR_CLIENT_INTERFACE,
            ..row(name, deps)
        }
    }

    /// Enabled comes before every later check.
    #[test]
    fn a_disabled_addon_reports_only_disabled() {
        let mut a = row("A", vec!["Ghost"]);
        a.enabled = false;
        a.interface = 0;
        let rows = vec![a];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("DISABLED")
        );
    }

    /// Banned and corrupt come after enabled and before the version gate, and a dependent of
    /// either reads `DEP_BANNED` / `DEP_CORRUPT`.
    #[test]
    fn banned_and_corrupt_refuse_and_their_dependents_inherit_it() {
        let mut a = row("A", vec![]);
        a.banned = true;
        a.interface = 0;
        let mut b = row("B", vec!["A"]);
        b.corrupt = true;
        let c = row("C", vec!["B"]);
        let rows = vec![a, b, c];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("BANNED"),
            "before the version gate"
        );
        assert_eq!(
            can_load(&rows, 1, false, true).token().as_deref(),
            Some("CORRUPT"),
            "its own check 5 comes before its dependencies"
        );
        assert_eq!(
            can_load(&rows, 2, false, true).token().as_deref(),
            Some("DEP_CORRUPT")
        );
        let mut off = row("D", vec![]);
        off.enabled = false;
        off.banned = true;
        assert_eq!(
            can_load(&[off], 0, false, true).token().as_deref(),
            Some("DISABLED"),
            "enabled is check 3, ahead of banned"
        );
    }

    #[test]
    fn the_wrath_version_gate_is_exact_and_force_load_erases_it() {
        let mut a = row("A", vec![]);
        a.interface = 30301;
        let rows = vec![a];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("INTERFACE_VERSION")
        );
        assert_eq!(can_load(&rows, 0, false, false), Verdict::Loadable);

        let mut b = row("B", vec![]);
        b.interface = 30200;
        let rows = vec![b];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("INTERFACE_VERSION")
        );
        assert_eq!(can_load(&rows, 0, false, false), Verdict::Loadable);
    }

    #[test]
    fn the_wrath_interface_floor_is_a_hard_incompatible() {
        for interface in [0, 11200, 19999] {
            let mut a = row("A", vec![]);
            a.interface = interface;
            let rows = vec![a];
            assert_eq!(
                can_load(&rows, 0, false, true).token().as_deref(),
                Some("INCOMPATIBLE"),
                "interface {interface} is below the 3.3.5a floor"
            );
            assert_eq!(
                can_load(&rows, 0, false, false).token().as_deref(),
                Some("INCOMPATIBLE"),
                "checkAddonVersion does not override the floor"
            );
        }
        // 30300 itself loads.
        assert_eq!(
            can_load(&[row("A", vec![])], 0, false, true),
            Verdict::Loadable
        );
    }

    #[test]
    fn the_wrath_demand_gate_refuses_a_normal_load_and_allows_load_addon() {
        let mut a = row("A", vec![]);
        a.load_on_demand = true;
        let rows = vec![a];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("DEMAND_LOADED")
        );
        assert_eq!(can_load(&rows, 0, true, true), Verdict::Loadable);
    }

    #[test]
    fn a_wrath_dependency_may_be_load_on_demand() {
        let a = row("A", vec!["B"]);
        let mut b = row("B", vec![]);
        b.load_on_demand = true;
        let rows = vec![a, b];
        assert_eq!(
            can_load(&rows, 0, false, true),
            Verdict::Loadable,
            "the dependency recursion sets demandLoad"
        );
    }

    #[test]
    fn dep_prefix_applies_once_at_any_depth() {
        let a = row("A", vec!["B"]);
        let b = row("B", vec!["C"]);
        let mut c = row("C", vec![]);
        c.enabled = false;
        let rows = vec![a, b, c];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("DEP_DISABLED"),
            "not DEP_DEP_DISABLED — the shared out-param collapses the nesting"
        );
        let rows = vec![row("A", vec!["Ghost"])];
        assert_eq!(
            can_load(&rows, 0, false, true).token().as_deref(),
            Some("DEP_MISSING")
        );
    }

    #[test]
    fn a_cycle_is_loadable_to_the_query() {
        let a = row("Ping", vec!["Pong"]);
        let b = row("Pong", vec!["Ping"]);
        let rows = vec![a, b];
        assert_eq!(can_load(&rows, 0, false, true), Verdict::Loadable);
    }

    /// The donor 1.12 law survives only for a 11200 client: exact 11200, no floor, and the
    /// original demand direction.
    #[test]
    fn the_donor_law_stays_on_11200() {
        let a = donor_row("A", vec![]);
        assert_eq!(
            can_load_for_interface(&[a], 0, false, true, DONOR_CLIENT_INTERFACE),
            Verdict::Loadable
        );
        let mut old = donor_row("Old", vec![]);
        old.interface = 11100;
        assert_eq!(
            can_load_for_interface(&[old], 0, false, true, DONOR_CLIENT_INTERFACE)
                .token()
                .as_deref(),
            Some("INTERFACE_VERSION")
        );
        // The donor demand query refuses a normal addon it has not loaded.
        assert_eq!(
            can_load_for_interface(
                &[donor_row("Normal", vec![])],
                0,
                true,
                true,
                DONOR_CLIENT_INTERFACE
            )
            .token()
            .as_deref(),
            Some("NOT_DEMAND_LOADED")
        );
        let mut lod = donor_row("Lod", vec![]);
        lod.load_on_demand = true;
        assert_eq!(
            can_load_for_interface(&[lod], 0, true, true, DONOR_CLIENT_INTERFACE),
            Verdict::Loadable
        );
    }

    #[test]
    fn a_loaded_dependency_satisfies_the_wrath_demand_query() {
        let mut a = row("A", vec!["B"]);
        a.load_on_demand = true;
        let mut b = row("B", vec![]);
        b.loaded = true;
        let rows = vec![a, b];
        assert_eq!(can_load(&rows, 0, true, true), Verdict::Loadable);
    }
}
