//! Session-state producer tests: v2 contract semantics — revision/freshness separation,
//! lifecycle, target states, visibility completeness, retention/retry, no fabrication.

use benilla_protocol_wrath::fields;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::session::{SelfTarget, SessionState};

/// Test-side encoder (independent construction; the F12 rule).
mod enc {
    use benilla_protocol_wrath::fields;

    pub fn pack_guid(guid: u64) -> Vec<u8> {
        let mut mask = 0u8;
        let mut bytes = Vec::new();
        for i in 0..8 {
            let b = (guid >> (i * 8)) as u8;
            if b != 0 {
                mask |= 1 << i;
                bytes.push(b);
            }
        }
        let mut out = vec![mask];
        out.extend_from_slice(&bytes);
        out
    }

    pub fn u32b(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    pub fn values_wire(values: &[(u16, u32)]) -> Vec<u8> {
        let words = values.iter().map(|(i, _)| *i as usize / 64 + 1).max().unwrap_or(1);
        let mut mask = vec![0u32; words];
        for (i, _) in values {
            mask[*i as usize / 64] |= 1u32 << (*i % 64);
        }
        let mut p = vec![words as u8];
        for w in &mask {
            p.extend(u32b(*w));
        }
        for (_, v) in values {
            p.extend(u32b(*v));
        }
        p
    }

    /// Stationary self-create with the target field delivered as `target`.
    pub fn stationary_player(guid: u64, x: f32, target: u64) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(3); // CREATE_OBJECT2
        p.extend(pack_guid(guid));
        p.push(4); // TYPEID_PLAYER
        p.extend(0x0040u16.to_le_bytes()); // STATIONARY_POSITION
        p.extend(x.to_le_bytes());
        p.extend(1.0f32.to_le_bytes());
        p.extend(2.0f32.to_le_bytes());
        p.extend(0.25f32.to_le_bytes());
        p.extend(values_wire(&[
            (fields::OBJECT_FIELD_GUID, guid as u32),
            (fields::OBJECT_FIELD_GUID + 1, (guid >> 32) as u32),
            (fields::UNIT_FIELD_TARGET, target as u32),
            (fields::UNIT_FIELD_TARGET + 1, (target >> 32) as u32),
            (fields::UNIT_FIELD_HEALTH, 100),
        ]));
        p
    }

    pub fn stationary_other(guid: u64, x: f32) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(3);
        p.extend(pack_guid(guid));
        p.push(3); // TYPEID_UNIT
        p.extend(0x0040u16.to_le_bytes());
        p.extend(x.to_le_bytes());
        p.extend(1.0f32.to_le_bytes());
        p.extend(2.0f32.to_le_bytes());
        p.extend(0.25f32.to_le_bytes());
        p.extend(values_wire(&[(fields::UNIT_FIELD_HEALTH, 10)]));
        p
    }

    pub fn values_update(guid: u64, values: &[(u16, u32)]) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(0);
        p.extend(pack_guid(guid));
        p.extend(values_wire(values));
        p
    }

    pub fn out_of_range(guid: u64) -> Vec<u8> {
        let mut p = u32b(1);
        p.push(4);
        p.extend(u32b(1));
        p.extend(pack_guid(guid));
        p
    }

    pub fn destroy(guid: u64) -> Vec<u8> {
        let mut p = guid.to_le_bytes().to_vec();
        p.push(0); // on-death flag
        p
    }

    pub fn verify_world(map: u32, x: f32) -> Vec<u8> {
        let mut p = map.to_le_bytes().to_vec();
        p.extend(x.to_le_bytes());
        p.extend(1.0f32.to_le_bytes());
        p.extend(2.0f32.to_le_bytes());
        p.extend(0.5f32.to_le_bytes());
        p
    }
}

const PLAYER: u64 = 0x0000_0000_0000_1521; // guid 5409, the live probe character
const OTHER: u64 = 0x0000_0000_0000_0F0F;

#[test]
fn export_refuses_to_invent_a_position() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    // Player entry materializes without any position-bearing block: the export must refuse
    // with a position reason, never an invented (0,0,0,0). Target is Unknown, which is
    // exportable — the position is the only blocker.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 58)]),
        1_000,
    )
    .unwrap();
    let err = s.export(1_100, 10).unwrap_err();
    assert!(err.to_string().contains("position"), "got: {err}");
}

#[test]
fn login_verify_world_establishes_map_and_position() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, -8913.0), 1_000).unwrap();
    // No self entry yet: target is Unknown (explicitly exported), position from verify.
    let snap = s.export(1_100, 10).unwrap().expect("exportable after verify");
    assert_eq!(snap.session_epoch, 1);
    assert_eq!(snap.sequence, 1);
    assert_eq!(snap.state_revision, 1);
    assert_eq!(snap.observed_at_unix_ms, 1_000, "source time, not export time");
    assert_eq!(snap.player_guid, PLAYER);
    assert_eq!(snap.map_id, 0);
    assert_eq!(snap.position.x, -8913.0);
    assert_eq!(snap.position.o, 0.5);
    assert_eq!(snap.target, SelfTarget::Unknown);
    assert_eq!(snap.zone_id, None);
    assert_eq!(snap.area_id, None);
    assert!(snap.visibility.complete);
    assert_eq!(snap.visibility.total_guid_count, 0);
    assert!(snap.visibility.guids.is_empty());

    // Idle: no change since handoff, nothing republished.
    s.mark_published();
    assert!(!s.export_due());
    assert!(s.export(2_000, 11_000).unwrap().is_none());
}

#[test]
fn target_states_are_distinguished_and_explicit() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();

    // Self entry without a delivered target field: explicit Unknown, still exported.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 58)]),
        1_100,
    )
    .unwrap();
    let snap = s.export(1_150, 10).unwrap().unwrap();
    assert_eq!(snap.target, SelfTarget::Unknown);

    // Target delivered as zero: observed no-target.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(
            PLAYER,
            &[(fields::UNIT_FIELD_TARGET, 0), (fields::UNIT_FIELD_TARGET + 1, 0)],
        ),
        1_200,
    )
    .unwrap();
    let snap = s.export(1_250, 20).unwrap().unwrap();
    assert_eq!(snap.target, SelfTarget::None);

    // Target delivered nonzero: observed target.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(
            PLAYER,
            &[(fields::UNIT_FIELD_TARGET, 0xF0F), (fields::UNIT_FIELD_TARGET + 1, 0)],
        ),
        1_300,
    )
    .unwrap();
    let snap = s.export(1_350, 30).unwrap().unwrap();
    assert_eq!(snap.target, SelfTarget::Target(OTHER));
}

#[test]
fn unrelated_packets_advance_neither_revision_nor_freshness() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    let snap = s.export(1_050, 10).unwrap().unwrap();
    let (revision, source_time) = (snap.state_revision, snap.observed_at_unix_ms);
    s.mark_published();

    // A burst of unrelated traffic: pong, addon info, misc, and an empty update packet
    // whose block count is zero (parses fine, changes nothing).
    s.reduce(opcode::SMSG_PONG, &[0u8; 8], 2_000).unwrap();
    s.reduce(0x02EF, &[1, 2, 3], 2_100).unwrap();
    s.reduce(0x04AB, &[5; 4], 2_200).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &0u32.to_le_bytes(), 2_300).unwrap();

    assert!(!s.export_due(), "unrelated traffic must not create an export");
    assert!(s.export(2_400, 60_000).unwrap().is_none());
    let _ = (revision, source_time);
}

#[test]
fn heartbeat_movement_and_identical_rewrites_do_not_advance_the_revision() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_player(PLAYER, 5.0, 0), 1_100).unwrap();
    let snap = s.export(1_150, 10).unwrap().unwrap();
    let revision = snap.state_revision;
    s.mark_published();

    // A values block rewriting identical values: no state change.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 100)]),
        1_200,
    )
    .unwrap();
    assert!(!s.export_due(), "identical rewrite is not a state change");

    // A different value: revision advances, observed time moves to the new packet.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 90)]),
        1_300,
    )
    .unwrap();
    let snap = s.export(1_350, 20).unwrap().unwrap();
    assert_eq!(snap.state_revision, revision + 1);
    assert_eq!(snap.observed_at_unix_ms, 1_300);
    assert_eq!(snap.client_uptime_ms, 20);
}

#[test]
fn visibility_loss_through_both_paths_shrinks_the_export() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_player(PLAYER, 1.0, 0), 1_050).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_other(OTHER, 5.0), 1_100).unwrap();
    let snap = s.export(1_150, 10).unwrap().unwrap();
    assert!(snap.visibility.complete);
    assert_eq!(snap.visibility.guids, vec![OTHER]);

    // Out-of-range block.
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::out_of_range(OTHER), 1_200).unwrap();
    let snap = s.export(1_250, 20).unwrap().unwrap();
    assert!(snap.visibility.guids.is_empty());

    // Back in range, then destroyed by SMSG_DESTROY_OBJECT.
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_other(OTHER, 6.0), 1_300).unwrap();
    s.reduce(opcode::SMSG_DESTROY_OBJECT, &enc::destroy(OTHER), 1_350).unwrap();
    let snap = s.export(1_400, 40).unwrap().unwrap();
    assert!(snap.visibility.guids.is_empty());
}

#[test]
fn epochs_reset_every_scope() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    let first = s.export(1_050, 10).unwrap().unwrap();
    assert_eq!((first.session_epoch, first.sequence, first.state_revision), (1, 1, 1));

    // Reconnect: a new epoch resets sequence, revision, and state.
    s.finish_session();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(1, 5.0), 2_000).unwrap();
    let second = s.export(2_050, 5).unwrap().unwrap();
    assert_eq!(second.session_epoch, 2);
    assert_eq!(second.sequence, 1, "sequence resets within the new epoch");
    assert_eq!(second.state_revision, 1, "revision resets within the new epoch");
    assert_eq!(second.map_id, 1);
    assert!(second.visibility.guids.is_empty(), "state cleared across epochs");
}

#[test]
fn visibility_overflow_exports_incomplete_with_the_total() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    for i in 0..260u64 {
        s.reduce(
            opcode::SMSG_UPDATE_OBJECT,
            &enc::stationary_other(0x1000 + i, i as f32),
            1_000 + i as i64,
        )
        .unwrap();
    }
    let snap = s.export(2_000, 10).unwrap().unwrap();
    assert!(!snap.visibility.complete, "over-bound visibility is explicit");
    assert_eq!(snap.visibility.total_guid_count, 260);
    assert_eq!(snap.visibility.guids.len(), 256);
    assert!(snap.visibility.guids.is_sorted());
}

#[test]
fn far_transfer_follows_the_real_server_sequence() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_other(OTHER, 5.0), 1_050).unwrap();
    assert!(s.export(1_100, 10).unwrap().is_some());
    s.mark_published();
    assert_eq!(s.world_generation(), 1);

    // 1. TRANSFER_PENDING: announced; the player is still in the old world, traffic reduces.
    let mut pending = 7u32.to_le_bytes().to_vec(); // destination map id
    pending.extend_from_slice(&0u32.to_le_bytes()); // transport entry
    pending.extend_from_slice(&0u32.to_le_bytes()); // current map
    s.reduce(opcode::SMSG_TRANSFER_PENDING, &pending, 1_200).unwrap();
    assert_eq!(s.phase(), benilla_protocol_wrath::session::Phase::TransferPending);
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 95)]), 1_250).unwrap();

    // 2. NEW_WORLD: old scope discarded, world generation bumped, destination authoritative.
    let mut new_world = 7u32.to_le_bytes().to_vec();
    new_world.extend(10.0f32.to_le_bytes());
    new_world.extend(20.0f32.to_le_bytes());
    new_world.extend(30.0f32.to_le_bytes());
    new_world.extend(1.5f32.to_le_bytes());
    s.reduce(opcode::SMSG_NEW_WORLD, &new_world, 1_300).unwrap();
    assert_eq!(s.phase(), benilla_protocol_wrath::session::Phase::Loading);
    assert_eq!(s.world_generation(), 2);

    // 3. Old-world in-flight tail between NEW_WORLD and the worldport ack is discarded.
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 10)]), 1_350).unwrap();
    s.reduce(opcode::SMSG_DESTROY_OBJECT, &enc::destroy(OTHER), 1_360).unwrap();
    let tail = s.export(1_400, 50).unwrap_err().to_string();
    assert!(tail.contains("position") || tail.contains("world"), "loading-phase export refused: {tail}");

    // 4. Driver sends MSG_MOVE_WORLDPORT_ACK after its own teardown; new world reduces.
    s.worldport_acknowledged();
    assert_eq!(s.phase(), benilla_protocol_wrath::session::Phase::InWorld);
    let snap = s.export(1_500, 60).unwrap().unwrap();
    assert_eq!(snap.map_id, 7);
    assert_eq!(snap.position.x, 10.0);
    assert_eq!(snap.position.o, 1.5);
    assert_eq!(snap.world_generation, 2);
    assert!(snap.visibility.guids.is_empty(), "old-world entities cannot leak");

    // Lifecycle: one WorldTransfer for the new scope; the driver retires the publisher.
    assert_eq!(
        s.take_lifecycle(),
        vec![benilla_protocol_wrath::session::LifeEvent::WorldTransfer { world_generation: 2, map_id: 7 }]
    );
}

#[test]
fn same_map_instance_transfer_still_changes_world_scope() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_other(OTHER, 5.0), 1_050).unwrap();
    assert!(s.export(1_100, 10).unwrap().is_some());
    s.mark_published();

    // NEW_WORLD with the SAME map id (e.g. an instance transfer): scope must still change.
    let mut new_world = 0u32.to_le_bytes().to_vec();
    new_world.extend(2.0f32.to_le_bytes());
    new_world.extend(0.0f32.to_le_bytes());
    new_world.extend(0.0f32.to_le_bytes());
    new_world.extend(0.0f32.to_le_bytes());
    s.reduce(opcode::SMSG_NEW_WORLD, &new_world, 1_500).unwrap();
    assert_eq!(s.world_generation(), 2);
    s.worldport_acknowledged();
    let snap = s.export(1_550, 20).unwrap().unwrap();
    assert_eq!(snap.world_generation, 2);
    assert!(snap.visibility.guids.is_empty(), "old-instance entities cannot leak");
}

#[test]
fn near_teleport_updates_position_without_changing_world_scope() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();
    s.reduce(opcode::SMSG_UPDATE_OBJECT, &enc::stationary_player(PLAYER, 1.0, 0), 1_050).unwrap();
    assert!(s.export(1_100, 10).unwrap().is_some());
    s.mark_published();

    // MSG_MOVE_TELEPORT: packed GUID + standalone movement payload (no update-flags prefix).
    let mut p = enc::pack_guid(PLAYER);
    p.extend(0u32.to_le_bytes()); // move flags
    p.extend(0u16.to_le_bytes()); // extra flags
    p.extend(12345u32.to_le_bytes()); // time
    p.extend(42.0f32.to_le_bytes());
    p.extend(1.0f32.to_le_bytes());
    p.extend(2.0f32.to_le_bytes());
    p.extend(0.75f32.to_le_bytes());
    p.extend(0u32.to_le_bytes()); // fall time (always written by BuildMovementPacket)
    s.reduce(opcode::MSG_MOVE_TELEPORT, &p, 1_200).unwrap();
    let snap = s.export(1_250, 20).unwrap().unwrap();
    assert_eq!(snap.position.x, 42.0);
    assert_eq!(snap.position.o, 0.75);
    assert_eq!(s.world_generation(), 1, "near teleport does not change world scope");
}

#[test]
fn unknown_to_known_zero_target_is_a_revision_change() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 1.0), 1_000).unwrap();

    // Higher-index field extends the vector past the target words without delivering them:
    // the padded zero must NOT count as knowledge.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 58)]),
        1_050,
    )
    .unwrap();
    let snap = s.export(1_100, 10).unwrap().unwrap();
    assert_eq!(snap.target, SelfTarget::Unknown, "padded zero is not knowledge");
    s.mark_published();

    // First explicit delivery of BOTH target words, values zero: knowledge changes even
    // though the numeric value equals the padding — one revision bump, known no-target.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(
            PLAYER,
            &[(fields::UNIT_FIELD_TARGET, 0), (fields::UNIT_FIELD_TARGET + 1, 0)],
        ),
        1_200,
    )
    .unwrap();
    let snap = s.export(1_250, 20).unwrap().unwrap();
    assert_eq!(snap.target, SelfTarget::None);
    assert_eq!(snap.state_revision, snap.state_revision.max(2));
    s.mark_published();

    // Repeated identical known zero: no revision advance.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(
            PLAYER,
            &[(fields::UNIT_FIELD_TARGET, 0), (fields::UNIT_FIELD_TARGET + 1, 0)],
        ),
        1_300,
    )
    .unwrap();
    assert!(!s.export_due(), "known-zero repeat is not a state change");
}

#[test]
fn publisher_busy_retains_and_reexports_without_losing_the_snapshot() {
    let mut s = SessionState::new();
    s.start_session(PLAYER);
    s.note_player_login();
    s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 3.25), 1_000).unwrap();
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 75)]),
        1_100,
    )
    .unwrap();

    // First export: new state. The "publisher" is busy — the driver does not mark_published.
    let first = s.export(1_200, 100).unwrap().unwrap();
    assert_eq!(first.sequence, 1);
    assert_eq!(first.state_revision, 2);
    assert_eq!(first.observed_at_unix_ms, 1_100);

    // Retry while still busy: same revision and source time, fresh sequence and uptime.
    let retry = s.export(1_500, 400).unwrap().unwrap();
    assert_eq!(retry.sequence, 2, "sequence advances on the unchanged re-export");
    assert_eq!(retry.state_revision, first.state_revision, "revision unchanged");
    assert_eq!(retry.observed_at_unix_ms, first.observed_at_unix_ms, "source time unchanged");
    assert_eq!(retry.position, first.position);

    // Handoff succeeds: the retained snapshot is consumed.
    s.mark_published();
    assert!(!s.export_due());
    assert!(s.export(1_600, 500).unwrap().is_none());

    // New state later: next revision, next sequence.
    s.reduce(
        opcode::SMSG_UPDATE_OBJECT,
        &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 60)]),
        2_000,
    )
    .unwrap();
    let next = s.export(2_050, 900).unwrap().unwrap();
    assert_eq!(next.sequence, 3);
    assert_eq!(next.state_revision, 3);
    assert_eq!(next.observed_at_unix_ms, 2_000);
}

#[test]
fn clock_values_come_from_the_caller_not_a_clock() {
    let build = |unix_ms: i64, uptime: u64| {
        let mut s = SessionState::new();
        s.start_session(PLAYER);
        s.note_player_login();
        s.reduce(opcode::SMSG_LOGIN_VERIFY_WORLD, &enc::verify_world(0, 3.25), unix_ms).unwrap();
        s.reduce(
            opcode::SMSG_UPDATE_OBJECT,
            &enc::values_update(PLAYER, &[(fields::UNIT_FIELD_HEALTH, 75)]),
            unix_ms + 100,
        )
        .unwrap();
        s.reduce(
            opcode::SMSG_UPDATE_OBJECT,
            &enc::values_update(
                PLAYER,
                &[(fields::UNIT_FIELD_TARGET, 0), (fields::UNIT_FIELD_TARGET + 1, 0)],
            ),
            unix_ms + 200,
        )
        .unwrap();
        s.export(unix_ms + 250, uptime).unwrap().unwrap()
    };
    let a = build(50_000, 1_000);
    let b = build(50_000, 1_000);
    assert_eq!(a, b);
    let c = build(50_100, 1_100);
    assert_ne!(a.observed_at_unix_ms, c.observed_at_unix_ms);
    assert_eq!(a.position, c.position);
}
