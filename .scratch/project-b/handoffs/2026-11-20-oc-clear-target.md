# Clear target: SMSG_CLEAR_TARGET (0x3BF)

Status: ready to merge
Branch: oc/clear-target

## What changed

- crates/benilla-protocol-wrath/src/opcode.rs: added SMSG_CLEAR_TARGET = 0x3BF.
- crates/benilla-protocol-wrath/src/gameplay.rs: added ServerEvent::ClearTarget { guid }, dispatch, read_clear_target (one raw u64 LE), and test wrath_clear_target_reads_the_raw_guid.
- crates/benilla-protocol/src/events.rs: added SessionEvent::ClearTarget { guid }.
- crates/benilla-app/src/wrath.rs: translate WrathGameplayEvent::ClearTarget -> SessionEvent::ClearTarget, plus test clear_target_translates_to_the_stock_event.
- crates/benilla-app/src/death/net.rs: registered K::ClearTarget handler on_clear_target writing DeselectGuid(guid) (message resource added); plus test clear_target_writes_a_guid_scoped_deselect.

The existing clear_target_requests drain already ignores a guid that is not the current selection, so no unrelated target is cleared and no packet is sent.

## Named tests

- benilla-protocol-wrath: gameplay::tests::wrath_clear_target_reads_the_raw_guid
- benilla-app: wrath::tests::clear_target_translates_to_the_stock_event
- benilla-app: death::net::tests::clear_target_writes_a_guid_scoped_deselect

## CI

CI 44479269404: the Windows check (and clippy, which compiles tests) failed on this branch's own
test — `DeselectGuid` had no `PartialEq`. The derive was added; the follow-up push re-runs CI.
No Cargo build, test or client run was made in this session.

## What is not proven in game

- Forced deselect receipt in a live owner session (EffectForceDeselect path) is unverified; the change is source-reviewed only.
