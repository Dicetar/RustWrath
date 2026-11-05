//! Project the scoped `SMSG_PARTYKILLLOG` into the Wrath VM's `COMBAT_LOG_EVENT_UNFILTERED`
//! contract. The decoded kill is proof of a `PARTY_KILL`; names come from the VM's own unit views
//! or the existing name cache, never from a guess. An unresolved endpoint suppresses the event
//! instead of emitting a fabricated name the bridge would trust.

use bevy::prelude::*;
use benilla_protocol::guid;
use benilla_ui::script::{ScriptValue, UiTarget};

use crate::names::NameCache;
use crate::query_cache::Ask;
use crate::script_calls::VmWith;
use crate::ui_script::UiClock;
use crate::wrath::WrathWorldView;

/// The VM a kill was last signalled to and the highest revision already fired. A rebuilt VM starts
/// at the current revision so a reconnect never replays a kill the previous VM already saw.
#[derive(Default)]
pub(crate) struct CombatLogMemo {
    vm: Option<u64>,
    revision: u64,
}

pub(crate) fn feed(
    mut vm: VmWith<(
        Option<Res<'static, WrathWorldView>>,
        Option<Res<'static, UiClock>>,
        Option<Res<'static, NameCache>>,
    )>,
    mut memo: Local<CombatLogMemo>,
) {
    vm.feed(|script, (world, clock, names), fires| {
        let Some(script) = script.filter(|script| script.target() == UiTarget::Wrath335) else {
            *memo = CombatLogMemo::default();
            return;
        };
        let Some(world) = world.as_deref() else {
            *memo = CombatLogMemo::default();
            return;
        };
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld)
            || world.player_guid == 0
        {
            *memo = CombatLogMemo::default();
            return;
        }

        if memo.vm != Some(script.session()) {
            *memo = CombatLogMemo {
                vm: Some(script.session()),
                revision: world.party_kill.map_or(0, |kill| kill.revision),
            };
        }

        let Some(kill) = world.party_kill else {
            return;
        };
        if kill.revision <= memo.revision
            || kill.session_epoch != world.session_epoch
            || kill.world_generation != world.world_generation
        {
            return;
        }
        memo.revision = kill.revision;

        // The bridge compares the source name against `UnitName("player")`, so the VM's own seat
        // is authoritative; the cache is the fallback for a frame before the unit feed lands.
        let player_name = script
            .unit_view("player")
            .and_then(|unit| unit.name)
            .filter(|name| !name.is_empty())
            .or_else(|| {
                names
                    .as_deref()
                    .and_then(|names| names.resolve_player(world.player_guid, Ask::Read))
                    .map(str::to_owned)
            });
        let Some(player_name) = player_name else {
            return;
        };

        let victim_name = script
            .unit_view_by_guid(kill.victim)
            .and_then(|unit| unit.name)
            .filter(|name| !name.is_empty())
            .or_else(|| {
                names.as_deref().and_then(|names| {
                    if guid::is_player(kill.victim) {
                        names
                            .resolve_player(kill.victim, Ask::Read)
                            .map(str::to_owned)
                    } else {
                        let entry = world
                            .entities
                            .get(&kill.victim)
                            .and_then(|unit| unit.entry)
                            .or_else(|| guid::entry(kill.victim))?;
                        names
                            .resolve_creature(entry, kill.victim, Ask::Read)
                            .map(str::to_owned)
                    }
                })
            });
        let Some(victim_name) = victim_name else {
            return;
        };

        let timestamp = clock.as_deref().map_or(0.0, |clock| clock.ui_now);
        fires.post(
            "COMBAT_LOG_EVENT_UNFILTERED",
            combat_log_args(
                timestamp,
                kill.killer,
                kill.victim,
                &player_name,
                &victim_name,
            ),
        );
    });
}

/// The exactly eight values `CombatLogEntry::PushEvent` sends for a spell-less event
/// (`UnitCombatLog_C.cpp:1241-1276`): an unresolved endpoint is dropped by the caller, never
/// replaced with a placeholder.
fn combat_log_args(
    timestamp: f64,
    killer: u64,
    victim: u64,
    player_name: &str,
    victim_name: &str,
) -> Vec<ScriptValue> {
    vec![
        ScriptValue::Number(timestamp),
        ScriptValue::Str("PARTY_KILL".to_owned()),
        ScriptValue::Str(guid_hex(killer)),
        ScriptValue::Str(player_name.to_owned()),
        ScriptValue::Int(combat_log_flags(killer)),
        ScriptValue::Str(guid_hex(victim)),
        ScriptValue::Str(victim_name.to_owned()),
        ScriptValue::Int(combat_log_flags(victim)),
    ]
}

/// `UnitGUID`'s 3.3.5 shape (`0x` plus sixteen uppercase hex digits).
fn guid_hex(guid: u64) -> String {
    format!("0x{guid:016X}")
}

/// The type bits of `GetCombatLogFlags` (`UnitCombatLog_C.cpp:713-731`). Group affiliation bits
/// need the party/raid owner and are deliberately absent rather than guessed; the bridge checks
/// names and guids, not these flags.
fn combat_log_flags(guid: u64) -> i64 {
    if guid::is_player(guid) {
        0x400
    } else if guid::is_pet(guid) {
        0x1000
    } else if guid::is_creature_or_pet(guid) {
        0x800
    } else if guid::is_gameobject(guid) {
        0x4000
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_ui::script::UiScript;

    #[test]
    fn party_kill_event_has_the_eight_wrath_combat_log_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathCombatLogTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            combat_log_args(
                1234.5,
                0x0000_0000_0000_0011,
                0xF130_0000_0000_0022,
                "Firna",
                "Kobold Vermin",
            ),
        );

        let event: String = scope.eval("return Fired[1]").unwrap();
        let timestamp: f64 = scope.eval("return Fired[2]").unwrap();
        let subevent: String = scope.eval("return Fired[3]").unwrap();
        let source_guid: String = scope.eval("return Fired[4]").unwrap();
        let source_name: String = scope.eval("return Fired[5]").unwrap();
        let source_flags: i64 = scope.eval("return Fired[6]").unwrap();
        let dest_guid: String = scope.eval("return Fired[7]").unwrap();
        let dest_name: String = scope.eval("return Fired[8]").unwrap();
        let dest_flags: i64 = scope.eval("return Fired[9]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "PARTY_KILL");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Kobold Vermin");
        assert_eq!(dest_flags, 0x800);
    }

    #[test]
    fn combat_log_flags_name_only_the_guid_family_they_can_prove() {
        assert_eq!(combat_log_flags(0x0000_0000_0000_0011), 0x400);
        assert_eq!(combat_log_flags(0xF140_0000_0000_0011), 0x1000);
        assert_eq!(combat_log_flags(0xF130_0000_0000_0011), 0x800);
        assert_eq!(combat_log_flags(0xF110_0000_0000_0011), 0x4000);
        assert_eq!(combat_log_flags(0), 0);
    }
}
