//! Replay the live login-burst fixture through the reducer: every packet must parse, the
//! world must fill, and three runs must produce identical state (H2 determinism).

use benilla_protocol_wrath::fields;
use benilla_protocol_wrath::opcode;
use benilla_protocol_wrath::reducer::{ChangeEvent, WorldState};

fn read_fixture() -> Vec<(u16, Vec<u8>)> {
    let bytes = include_bytes!("../fixtures/login-burst.bin");
    let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let mut at = 4usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let op = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 2..at + 6].try_into().unwrap()) as usize;
        at += 6;
        out.push((op, bytes[at..at + len].to_vec()));
        at += len;
    }
    assert_eq!(at, bytes.len(), "fixture fully consumed");
    out
}

fn run_once(packets: &[(u16, Vec<u8>)]) -> (WorldState, Vec<ChangeEvent>) {
    let mut state = WorldState::new();
    let mut events = Vec::new();
    for (op, payload) in packets {
        events.extend(state.reduce(*op, payload).expect("every packet reduces cleanly"));
    }
    (state, events)
}

#[test]
fn the_live_login_burst_reduces_deterministically() {
    let packets = read_fixture();
    assert!(packets.len() > 1000, "fixture carries the full burst");

    let (state, events) = run_once(&packets);

    assert!(events.iter().any(|e| matches!(e, ChangeEvent::Created { .. })), "creates");
    assert!(events.iter().any(|e| matches!(e, ChangeEvent::Updated { .. })), "value updates");
    assert!(state.len() > 5, "a populated world, got {}", state.len());

    let player = state
        .iter()
        .find(|(_, e)| e.self_guid(fields::OBJECT_FIELD_GUID).is_some())
        .map(|(_, e)| e)
        .expect("an object carrying a self GUID");
    assert!(player.field(fields::UNIT_FIELD_HEALTH).unwrap_or(0) > 0, "player health set");
    assert!(player.movement.is_some(), "player has a movement snapshot");

    let run2 = run_once(&packets);
    let run3 = run_once(&packets);
    assert!(state == run2.0 && run2.0 == run3.0);
    assert!(events == run2.1 && run2.1 == run3.1);
}

#[test]
fn update_packets_present_in_the_fixture() {
    let packets = read_fixture();
    let plain = packets.iter().filter(|(op, _)| *op == opcode::SMSG_UPDATE_OBJECT).count();
    let compressed = packets
        .iter()
        .filter(|(op, _)| *op == opcode::SMSG_COMPRESSED_UPDATE_OBJECT)
        .count();
    assert!(plain > 0 && compressed > 0, "both update paths exercised ({plain}/{compressed})");
}
