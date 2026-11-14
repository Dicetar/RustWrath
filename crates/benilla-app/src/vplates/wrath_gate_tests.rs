//! 12340's `ShouldShowNamePlate` from the category on (`Unit_C.cpp:20451`-`:20505`), over the
//! resolved facts [`wrath_plate_gate`] takes.

use super::*;

const BOTH: VPlateMode = VPlateMode {
    enemies: true,
    friends: true,
};

fn npc(friendly: bool) -> WrathPlateUnit {
    WrathPlateUnit {
        friendly,
        attackable: !friendly,
        creature_type: 7,
        ..Default::default()
    }
}

/// A unit a player owns: a pet carries SUMMONEDBY, a guardian CREATEDBY alone.
fn owned(friendly: bool, creature_type: u32, guardian: bool) -> WrathPlateUnit {
    WrathPlateUnit {
        owner_is_player: true,
        has_creator: true,
        has_summoner: !guardian,
        creature_type,
        ..npc(friendly)
    }
}

/// The two category toggles each gate their own side (`:20459`, `:20463`).
#[test]
fn each_category_toggle_gates_its_own_side() {
    let rows = WrathPlateRows::default();
    let enemies_only = VPlateMode {
        enemies: true,
        friends: false,
    };
    assert!(wrath_plate_gate(&npc(false), enemies_only, rows));
    assert!(!wrath_plate_gate(&npc(true), enemies_only, rows));
    assert!(wrath_plate_gate(&npc(true), BOTH, rows));
}

/// Critters and non-combat pets never get one (`:20467`, `:20471`), whatever the toggles.
#[test]
fn critters_and_companions_have_no_plate() {
    let rows = WrathPlateRows::default();
    for ty in [CREATURE_TYPE_CRITTER, CREATURE_TYPE_NON_COMBAT_PET] {
        let unit = WrathPlateUnit {
            creature_type: ty,
            ..npc(false)
        };
        assert!(!wrath_plate_gate(&unit, BOTH, rows), "type {ty}");
    }
}

/// A dead-looking unit keeps its plate only as a raid member or pet (`:20475`-`:20480`), on
/// either side.
#[test]
fn a_dead_looking_unit_keeps_its_plate_only_in_the_raid() {
    let rows = WrathPlateRows::default();
    let feigning = WrathPlateUnit {
        dynflag_dead: true,
        ..npc(true)
    };
    assert!(!wrath_plate_gate(&feigning, BOTH, rows));
    let member = WrathPlateUnit {
        raid_member_or_pet: true,
        ..feigning
    };
    assert!(wrath_plate_gate(&member, BOTH, rows));
}

/// The creep bit hides only an enemy; the record's `0x100000` hides either (`:20451`, `:20455`).
#[test]
fn creep_hides_enemies_and_the_record_flag_hides_both() {
    let rows = WrathPlateRows::default();
    let creep = |friendly| WrathPlateUnit {
        creep: true,
        ..npc(friendly)
    };
    assert!(!wrath_plate_gate(&creep(false), BOTH, rows));
    assert!(wrath_plate_gate(&creep(true), BOTH, rows));
    let flagged = WrathPlateUnit {
        record_hides_plate: true,
        ..npc(true)
    };
    assert!(!wrath_plate_gate(&flagged, BOTH, rows));
}

/// A player's totem, guardian and pet each answer to their own row on their own side
/// (`:20490`-`:20500`), and an NPC-owned totem keeps its plate.
#[test]
fn player_owned_units_answer_to_their_own_rows() {
    let all = WrathPlateRows::default();
    let totem = owned(true, CREATURE_TYPE_TOTEM, true);
    let guardian = owned(false, 7, true);
    let pet = owned(false, 1, false);
    for unit in [totem, guardian, pet] {
        assert!(wrath_plate_gate(&unit, BOTH, all));
    }
    let no_friendly_totems = WrathPlateRows {
        friendly_totems: false,
        ..all
    };
    assert!(!wrath_plate_gate(&totem, BOTH, no_friendly_totems));
    assert!(wrath_plate_gate(
        &owned(false, CREATURE_TYPE_TOTEM, true),
        BOTH,
        no_friendly_totems
    ));
    let no_enemy_guardians = WrathPlateRows {
        enemy_guardians: false,
        ..all
    };
    assert!(!wrath_plate_gate(&guardian, BOTH, no_enemy_guardians));
    assert!(wrath_plate_gate(&pet, BOTH, no_enemy_guardians));
    let no_enemy_pets = WrathPlateRows {
        enemy_pets: false,
        ..all
    };
    assert!(!wrath_plate_gate(&pet, BOTH, no_enemy_pets));
    let npc_totem = WrathPlateUnit {
        creature_type: CREATURE_TYPE_TOTEM,
        has_creator: true,
        ..npc(true)
    };
    assert!(wrath_plate_gate(&npc_totem, BOTH, no_friendly_totems));
}

/// A charmed unit with a creator is a pet, not a guardian: `GetCreatorGuid` answers 0 under a
/// charmer (`Unit_C.cpp:6689`).
#[test]
fn a_charmed_unit_reads_as_a_pet() {
    let charmed = WrathPlateUnit {
        has_charmer: true,
        ..owned(false, 7, true)
    };
    let no_enemy_guardians = WrathPlateRows {
        enemy_guardians: false,
        ..WrathPlateRows::default()
    };
    assert!(wrath_plate_gate(&charmed, BOTH, no_enemy_guardians));
}

/// A player-owned unit with CREATEDBY, no SUMMONEDBY and flags `0x200` has no plate
/// (`:20486`); the same unit under an NPC owner keeps it.
#[test]
fn the_created_flag_rule_applies_only_to_player_owned_units() {
    let rows = WrathPlateRows::default();
    let flagged = WrathPlateUnit {
        unit_flags: 0x200,
        ..owned(false, 7, true)
    };
    assert!(!wrath_plate_gate(&flagged, BOTH, rows));
    let npc_owned = WrathPlateUnit {
        owner_is_player: false,
        ..flagged
    };
    assert!(wrath_plate_gate(&npc_owned, BOTH, rows));
}

/// An enemy-side player that cannot be attacked has no plate (`:20503`).
#[test]
fn an_unattackable_enemy_player_has_no_plate() {
    let rows = WrathPlateRows::default();
    let player = WrathPlateUnit {
        is_player: true,
        attackable: false,
        ..npc(false)
    };
    assert!(!wrath_plate_gate(&player, BOTH, rows));
    let flagged = WrathPlateUnit {
        attackable: true,
        ..player
    };
    assert!(wrath_plate_gate(&flagged, BOTH, rows));
}
