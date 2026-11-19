//! Project queued `SMSG_PARTYKILLLOG` and `SMSG_ATTACKERSTATEUPDATE` rows into the Wrath VM's
//! `COMBAT_LOG_EVENT_UNFILTERED` contract. A kill or swing is an ordered event, not a replaceable
//! snapshot: the view keeps a bounded pending queue and this feed removes each row only after it
//! has been signalled. The decoded packet proves the event; both endpoint names still come from
//! the VM's own unit views or the existing name-query cache, never from a guess. A swing keeps
//! 12340's raw hit word, school mask and victim state; `SWING_MISSED` rows are a later slice.

use benilla_ui::script::{ScriptValue, UiScript, UiTarget};
use bevy::prelude::*;

use crate::names::NameCache;
use crate::net::NetCommands;
use crate::query_cache::Ask;
use crate::script_calls::VmWith;
use crate::ui_script::UiClock;
use crate::wrath::{WrathPartyKill, WrathSwing, WrathWorldView, COMBAT_LOG_NAME_RETRY_SECONDS};

/// Capacity loss and unresolved-name counts already reported, so each diagnostic is logged once
/// per scope rather than every frame.
#[derive(Default)]
pub(crate) struct CombatLogMemo {
    dropped: u64,
    unresolved: u64,
    swing_dropped: u64,
    swing_unresolved: u64,
}

/// The visible marker for an endpoint whose name never resolved inside the retry window. It is
/// the client's own unknown-object spelling, never a guessed player or creature name.
const UNKNOWN_OBJECT_NAME: &str = "UNKNOWNOBJECT";

pub(crate) fn feed(
    mut vm: VmWith<(
        Option<ResMut<'static, WrathWorldView>>,
        Option<Res<'static, UiClock>>,
        Option<Res<'static, NameCache>>,
        Option<Res<'static, NetCommands>>,
    )>,
    mut memo: Local<CombatLogMemo>,
) {
    vm.feed(|script, (mut world, clock, names, net), fires| {
        let Some(script) = script.filter(|script| script.target() == UiTarget::Wrath335) else {
            return;
        };
        let Some(world) = world.as_deref_mut() else {
            return;
        };
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld)
            || world.player_guid == 0
        {
            return;
        }

        if world.party_kill_dropped > memo.dropped {
            debug!(
                dropped = world.party_kill_dropped,
                "wrath: combat-log queue capacity lost kills"
            );
        }
        memo.dropped = world.party_kill_dropped;
        if world.party_kill_unresolved > memo.unresolved {
            debug!(
                unresolved = world.party_kill_unresolved,
                "wrath: combat-log kills delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.unresolved = world.party_kill_unresolved;
        if world.swing_dropped > memo.swing_dropped {
            debug!(
                dropped = world.swing_dropped,
                "wrath: combat-log queue capacity lost melee swings"
            );
        }
        memo.swing_dropped = world.swing_dropped;
        if world.swing_unresolved > memo.swing_unresolved {
            debug!(
                unresolved = world.swing_unresolved,
                "wrath: combat-log swings delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.swing_unresolved = world.swing_unresolved;

        let now = clock.as_deref().map_or(0.0, |clock| clock.ui_now);
        let ask = match net.as_deref() {
            Some(net) => Ask::New(net),
            None => Ask::Read,
        };
        let mut index = 0;
        while index < world.party_kills.len() {
            let kill = world.party_kills[index];
            if kill.session_epoch != world.session_epoch
                || kill.world_generation != world.world_generation
            {
                // A row from a retired world is never delivered into the replacement chain.
                world.party_kills.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), kill.killer, ask);
            let target = endpoint_name(script, world, names.as_deref(), kill.victim, ask);
            let timestamp = if kill.received_at > 0.0 {
                kill.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if kill.received_at > 0.0 && now >= kill.received_at {
                    now - kill.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    // The retry window is spent. Drop neither the kill nor a name we never
                    // learned: deliver the event with a visible unknown-endpoint marker for the
                    // side(s) that did not resolve, then retire the row so a later feed cannot
                    // replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        combat_log_args(
                            timestamp,
                            &kill,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                        ),
                    );
                    world.party_kill_unresolved = world.party_kill_unresolved.saturating_add(1);
                    world.party_kills.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                combat_log_args(timestamp, &kill, &source, &target),
            );
            world.party_kills.remove(index);
        }

        let mut index = 0;
        while index < world.swings.len() {
            let swing = world.swings[index];
            if swing.session_epoch != world.session_epoch
                || swing.world_generation != world.world_generation
            {
                // A row from a retired world is never delivered into the replacement chain.
                world.swings.remove(index);
                continue;
            }
            if !swing_reports_damage(&swing) {
                // SWING_MISSED is a later slice. An intact/miss/parry/dodge row retires here
                // without inventing a damage event.
                world.swings.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), swing.hit.attacker, ask);
            let target = endpoint_name(script, world, names.as_deref(), swing.hit.victim, ask);
            let timestamp = if swing.received_at > 0.0 {
                swing.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if swing.received_at > 0.0 && now >= swing.received_at {
                    now - swing.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    // The retry window is spent. Deliver the damage with a visible unknown
                    // marker for the side(s) that did not resolve, then retire the row so a
                    // later feed cannot replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        swing_damage_args(
                            timestamp,
                            &swing,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                        ),
                    );
                    world.swing_unresolved = world.swing_unresolved.saturating_add(1);
                    world.swings.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                swing_damage_args(timestamp, &swing, &source, &target),
            );
            world.swings.remove(index);
        }
    });
}

/// Resolve one endpoint for the event. The local player's seat has its own view; every other unit
/// is looked up by its own guid. The cache read asks through the existing name-query owner when
/// `ask` admits it, so a combat-log row whose name has not arrived yet stays pending instead of
/// being sent with the wrong name.
fn endpoint_name(
    script: &UiScript,
    world: &WrathWorldView,
    names: Option<&NameCache>,
    unit: u64,
    ask: Ask<'_>,
) -> Option<String> {
    if unit == world.player_guid {
        if let Some(name) = script
            .unit_view("player")
            .and_then(|player| player.name)
            .filter(|name| !name.is_empty())
        {
            return Some(name);
        }
    }
    if let Some(name) = script
        .unit_view_by_guid(unit)
        .and_then(|unit| unit.name)
        .filter(|name| !name.is_empty())
    {
        return Some(name);
    }
    names
        .and_then(|names| names.resolve_unit(unit, None, ask))
        .map(str::to_owned)
}

/// 12340's `UnitCombatLog` first drops feedback for a melee-spell round unless the victim
/// state is a wound (`ShouldShowFeedback`, `UnitCombatLog_C.cpp:933-950`), then builds the
/// `SWING_DAMAGE` row only for a wound (1) or block (4) with non-zero damage
/// (`UnitCombatLog_C.cpp:952-980`). 12340's victim states are intact=0, hit=1 and blocks=4.
fn swing_reports_damage(swing: &WrathSwing) -> bool {
    swing.hit.damage != 0
        && matches!(swing.hit.victim_state, 1 | 4)
        && (swing.hit.melee_spell_id == 0 || swing.hit.victim_state == 1)
}

/// The exactly eight values `CombatLogEntry::PushEvent` sends before an event-specific block
/// (`UnitCombatLog_C.cpp:1241-1276`).
fn combat_log_header(
    timestamp: f64,
    subevent: &str,
    source: u64,
    source_name: &str,
    target: u64,
    target_name: &str,
) -> Vec<ScriptValue> {
    vec![
        ScriptValue::Number(timestamp),
        ScriptValue::Str(subevent.to_owned()),
        ScriptValue::Str(guid_hex(source)),
        ScriptValue::Str(source_name.to_owned()),
        ScriptValue::Int(combat_log_flags(source)),
        ScriptValue::Str(guid_hex(target)),
        ScriptValue::Str(target_name.to_owned()),
        ScriptValue::Int(combat_log_flags(target)),
    ]
}

/// `PARTY_KILL` has no event-specific values.
fn combat_log_args(
    timestamp: f64,
    kill: &WrathPartyKill,
    source_name: &str,
    target_name: &str,
) -> Vec<ScriptValue> {
    combat_log_header(
        timestamp,
        "PARTY_KILL",
        kill.killer,
        source_name,
        kill.victim,
        target_name,
    )
}

/// The nine values `CombatLogEntry::PushEvent` appends for `SWING_DAMAGE`
/// (`UnitCombatLog_C.cpp:939-980`, `:1241-1361`): damage, overkill, school mask, resisted,
/// blocked, absorbed, then critical, glancing and crushing as 1.0 or nil. The bit tests are
/// 12340's own (`0x200`, bit 16, `0x20000`), not the 1.12 translated hit word.
fn swing_damage_args(
    timestamp: f64,
    swing: &WrathSwing,
    source_name: &str,
    target_name: &str,
) -> Vec<ScriptValue> {
    let hit = swing.hit;
    let mut args = combat_log_header(
        timestamp,
        "SWING_DAMAGE",
        hit.attacker,
        source_name,
        hit.victim,
        target_name,
    );
    args.extend([
        ScriptValue::Number(f64::from(hit.damage)),
        ScriptValue::Number(f64::from(hit.overkill)),
        ScriptValue::Int(i64::from(hit.school)),
        nonzero(i64::from(hit.resist)),
        nonzero(i64::from(hit.blocked)),
        nonzero(i64::from(hit.absorb)),
        present(hit.hit_info & 0x200 != 0),
        present((hit.hit_info >> 16) & 1 != 0),
        present(hit.hit_info & 0x20000 != 0),
    ]);
    args
}

/// The reference pushes nil, not zero, for an absent resisted/blocked/absorbed amount.
fn nonzero(value: i64) -> ScriptValue {
    if value == 0 {
        ScriptValue::Nil
    } else {
        ScriptValue::Number(value as f64)
    }
}

/// A presence flag is 1.0 or nil in the reference, never false.
fn present(on: bool) -> ScriptValue {
    if on {
        ScriptValue::Number(1.0)
    } else {
        ScriptValue::Nil
    }
}

/// `UnitGUID`'s 3.3.5 shape (`0x` plus sixteen uppercase hex digits).
fn guid_hex(guid: u64) -> String {
    format!("0x{guid:016X}")
}

/// The type bits of `GetCombatLogFlags` (`UnitCombatLog_C.cpp:713-731`). Group affiliation bits
/// need the party/raid owner and are deliberately absent rather than guessed; the bridge checks
/// names and guids, not these flags.
fn combat_log_flags(guid: u64) -> i64 {
    use benilla_protocol::guid;
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
    use benilla_ui::script::{UiScript, UnitState};

    fn script_with_units() -> UiScript {
        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let unit = |name: &str, guid: u64| UnitState {
            name: Some(name.to_owned()),
            guid,
            ..UnitState::default()
        };
        script.set_unit("player", Some(unit("Firna", 0x11)));
        script.set_unit("party1", Some(unit("Teammate", 0x22)));
        script.set_unit_by_guid(0x22, Some(unit("Teammate", 0x22)));
        script
    }

    fn world(player_guid: u64) -> WrathWorldView {
        let mut world = WrathWorldView::default();
        world.player_guid = player_guid;
        world
    }

    #[test]
    fn source_name_follows_the_killer_not_the_local_player() {
        let script = script_with_units();
        let world = world(0x11);
        assert_eq!(
            endpoint_name(&script, &world, None, 0x22, Ask::Read).as_deref(),
            Some("Teammate"),
            "a teammate's kill keeps the teammate's name"
        );
        assert_eq!(
            endpoint_name(&script, &world, None, 0x11, Ask::Read).as_deref(),
            Some("Firna"),
            "the local player falls back to the player seat"
        );
        assert_eq!(
            endpoint_name(&script, &world, None, 0x33, Ask::Read),
            None,
            "an unknown endpoint stays unresolved instead of borrowing a name"
        );
    }

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

        let kill = WrathPartyKill {
            killer: 0x0000_0000_0000_0011,
            victim: 0xF130_0000_0000_0022,
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        };
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            combat_log_args(1234.5, &kill, "Firna", "Kobold Vermin"),
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

    fn swing_hit(damage: u32, victim_state: u32) -> crate::wrath::WrathSwingHit {
        crate::wrath::WrathSwingHit {
            attacker: 0x0000_0000_0000_0011,
            victim: 0xF130_0000_0000_0022,
            hit_info: 0x2,
            damage,
            overkill: 0,
            victim_state,
            absorb: 0,
            resist: 0,
            blocked: 0,
            melee_spell_id: 0,
            school: 1,
        }
    }

    /// The stock combat log reads nine values after the eight-value header; a crit, glance and
    /// crush are 1.0, the absent resisted/blocked/absorbed values are nil, and nothing extra is
    /// appended.
    #[test]
    fn swing_damage_event_has_the_seventeen_wrath_combat_log_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSwingLogTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let swing = WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x0003_0200, // critical | glancing | crushing
                overkill: 3,
                ..swing_hit(37, 1)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        };
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            swing_damage_args(1234.5, &swing, "Firna", "Kobold Vermin"),
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
        let damage: f64 = scope.eval("return Fired[10]").unwrap();
        let overkill: f64 = scope.eval("return Fired[11]").unwrap();
        let school: i64 = scope.eval("return Fired[12]").unwrap();
        let resisted: Option<f64> = scope.eval("return Fired[13]").unwrap();
        let blocked: Option<f64> = scope.eval("return Fired[14]").unwrap();
        let absorbed: Option<f64> = scope.eval("return Fired[15]").unwrap();
        let critical: Option<f64> = scope.eval("return Fired[16]").unwrap();
        let glancing: Option<f64> = scope.eval("return Fired[17]").unwrap();
        let crushing: Option<f64> = scope.eval("return Fired[18]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[19]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "SWING_DAMAGE");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Kobold Vermin");
        assert_eq!(dest_flags, 0x800);
        assert_eq!(damage, 37.0);
        assert_eq!(overkill, 3.0);
        assert_eq!(school, 1);
        assert_eq!((resisted, blocked, absorbed), (None, None, None));
        assert_eq!(
            (critical, glancing, crushing),
            (Some(1.0), Some(1.0), Some(1.0))
        );
        assert_eq!(extra, None, "exactly the nine SWING_DAMAGE values follow");
    }

    /// A blocked swing's three amounts are carried as numbers when non-zero; the crit/glance/
    /// crush flags stay nil because their bits are clear.
    #[test]
    fn swing_damage_carries_nonzero_resist_block_and_absorb() {
        let swing = WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x2002, // normal hit | block
                overkill: 1,
                resist: 4,
                blocked: 11,
                absorb: 7,
                ..swing_hit(20, 4)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        assert!(swing_reports_damage(&swing), "a blocked wound is shown");
        assert_eq!(
            swing_damage_args(10.0, &swing, "Firna", "Kobold Vermin")[8..].to_vec(),
            vec![
                ScriptValue::Number(20.0),
                ScriptValue::Number(1.0),
                ScriptValue::Int(1),
                ScriptValue::Number(4.0),
                ScriptValue::Number(11.0),
                ScriptValue::Number(7.0),
                ScriptValue::Nil,
                ScriptValue::Nil,
                ScriptValue::Nil,
            ]
        );
    }

    /// `ShouldShowFeedback` drops a blocked melee-spell round before the SWING_DAMAGE block, while
    /// a wounded melee-spell round is still shown.
    #[test]
    fn a_blocked_melee_spell_round_is_not_a_swing_damage_event() {
        let mut swing = WrathSwing {
            hit: swing_hit(20, 4),
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        assert!(swing_reports_damage(&swing), "a plain blocked swing shows");
        swing.hit.melee_spell_id = 78;
        assert!(
            !swing_reports_damage(&swing),
            "a blocked melee-spell round is dropped by ShouldShowFeedback"
        );
        swing.hit.victim_state = 1;
        assert!(
            swing_reports_damage(&swing),
            "a wounded melee-spell round is still shown"
        );
    }

    /// The feed drains the swing queue with the same name retry as kills; only a WOUND/BLOCK row
    /// with damage is signalled, and every other row is retired without an event.
    #[test]
    fn feed_delivers_a_wounded_swing_and_silently_retires_the_rest() {
        use crate::ui_script::UiClock;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::App;

        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        script.set_unit(
            "player",
            Some(UnitState {
                name: Some("Firna".to_owned()),
                guid: 0x11,
                ..UnitState::default()
            }),
        );
        script.set_unit_by_guid(
            0xF130_0000_0000_0022,
            Some(UnitState {
                name: Some("Kobold Vermin".to_owned()),
                guid: 0xF130_0000_0000_0022,
                ..UnitState::default()
            }),
        );
        script
            .run(
                r#"
                SEEN = 0
                local listener = CreateFrame("Frame", "WrathSwingFeedTest")
                listener:SetScript("OnEvent", function() SEEN = SEEN + 1 end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let mut world = WrathWorldView::default();
        world.phase = Some(benilla_protocol_wrath::session::Phase::InWorld);
        world.player_guid = 0x11;
        world.session_epoch = 4;
        world.world_generation = 5;
        for (damage, victim_state) in [(12, 1), (0, 1), (12, 2)] {
            world.apply_swing(swing_hit(damage, victim_state), 4, 5, 97.0);
        }

        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(UiClock {
                ui_now: 100.0,
                ..Default::default()
            })
            .init_resource::<NameCache>()
            .insert_non_send_resource(script);
        app.world_mut().run_system_once(feed).unwrap();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<i64>("return SEEN")
                .unwrap(),
            1,
            "only the wounded swing with damage reaches the log"
        );
        assert!(
            app.world().resource::<WrathWorldView>().swings.is_empty(),
            "delivered and silently retired rows leave nothing to replay"
        );

        app.world_mut().run_system_once(feed).unwrap();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<i64>("return SEEN")
                .unwrap(),
            1,
            "a later feed cannot replay a retired swing"
        );
    }

    #[test]
    fn feed_delivers_a_burst_waits_for_late_names_and_retires_stale_scopes() {
        use crate::ui_script::UiClock;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::App;

        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let player = UnitState {
            name: Some("Firna".to_owned()),
            guid: 0x11,
            ..UnitState::default()
        };
        script.set_unit("player", Some(player));
        script
            .run(
                r#"
                SEEN = 0
                local listener = CreateFrame("Frame", "WrathCombatLogFeedTest")
                listener:SetScript("OnEvent", function() SEEN = SEEN + 1 end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();
        let mut world = WrathWorldView::default();
        world.phase = Some(benilla_protocol_wrath::session::Phase::InWorld);
        world.player_guid = 0x11;
        world.session_epoch = 4;
        world.world_generation = 5;
        world.party_kills.push_back(WrathPartyKill {
            killer: 0x11,
            victim: 0xF130_0000_0000_0022,
            session_epoch: 4,
            world_generation: 5,
            revision: 1,
            received_at: 97.0,
        });
        world.party_kills.push_back(WrathPartyKill {
            killer: 0x11,
            victim: 0xF130_0000_0000_0023,
            session_epoch: 4,
            world_generation: 5,
            revision: 2,
            received_at: 97.0,
        });
        // A retired-scope row is dropped, never delivered.
        world.party_kills.push_back(WrathPartyKill {
            killer: 0x11,
            victim: 0xF130_0000_0000_0024,
            session_epoch: 3,
            world_generation: 3,
            revision: 3,
            received_at: 97.0,
        });

        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(UiClock {
                ui_now: 100.0,
                ..Default::default()
            })
            .init_resource::<NameCache>()
            .insert_non_send_resource(script);
        app.world_mut().run_system_once(feed).unwrap();
        {
            let script = app.world().non_send_resource::<UiScript>();
            assert_eq!(
                script.eval::<i64>("return SEEN").unwrap(),
                0,
                "unknown victim names hold delivery"
            );
        }
        {
            let world = app.world().resource::<WrathWorldView>();
            assert_eq!(
                world.party_kills.len(),
                2,
                "the stale row is gone, the live burst remains"
            );
        }
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .set_unit_by_guid(
                0xF130_0000_0000_0022,
                Some(UnitState {
                    name: Some("Kobold Vermin".to_owned()),
                    guid: 0xF130_0000_0000_0022,
                    ..UnitState::default()
                }),
            );
        app.world_mut().run_system_once(feed).unwrap();
        {
            let script = app.world().non_send_resource::<UiScript>();
            assert_eq!(
                script.eval::<i64>("return SEEN").unwrap(),
                1,
                "the late name releases one kill"
            );
        }
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .set_unit_by_guid(
                0xF130_0000_0000_0023,
                Some(UnitState {
                    name: Some("Kobold Worker".to_owned()),
                    guid: 0xF130_0000_0000_0023,
                    ..UnitState::default()
                }),
            );
        app.world_mut().run_system_once(feed).unwrap();
        {
            let script = app.world().non_send_resource::<UiScript>();
            assert_eq!(
                script.eval::<i64>("return SEEN").unwrap(),
                2,
                "the second kill still arrives"
            );
            assert_eq!(
                app.world().resource::<WrathWorldView>().party_kills.len(),
                0,
                "delivered rows are removed, so a rebuilt VM cannot replay them"
            );
        }
    }

    /// A name that never resolves is not silently dropped after the retry window: the kill is
    /// delivered once with `UNKNOWNOBJECT` for that endpoint, and a later feed cannot replay it.
    #[test]
    fn a_never_resolving_name_falls_back_visibly_once() {
        use crate::ui_script::UiClock;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::App;

        let mut script = UiScript::with_target(UiTarget::Wrath335).unwrap();
        script.set_unit(
            "player",
            Some(UnitState {
                name: Some("Firna".to_owned()),
                guid: 0x11,
                ..UnitState::default()
            }),
        );
        script
            .run(
                r#"
                SEEN = 0
                SOURCE = nil
                DEST = nil
                local listener = CreateFrame("Frame", "WrathCombatLogFallbackTest")
                listener:SetScript("OnEvent", function(_, _, _, _, _, source, _, _, dest)
                    SEEN = SEEN + 1
                    SOURCE = source
                    DEST = dest
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();
        let mut world = WrathWorldView::default();
        world.phase = Some(benilla_protocol_wrath::session::Phase::InWorld);
        world.player_guid = 0x11;
        world.session_epoch = 4;
        world.world_generation = 5;
        world.party_kills.push_back(WrathPartyKill {
            killer: 0x11,
            victim: 0xF130_0000_0000_0022,
            session_epoch: 4,
            world_generation: 5,
            revision: 1,
            received_at: 97.0,
        });

        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(UiClock {
                ui_now: 97.0 + COMBAT_LOG_NAME_RETRY_SECONDS + 0.5,
                ..Default::default()
            })
            .init_resource::<NameCache>()
            .insert_non_send_resource(script);
        app.world_mut().run_system_once(feed).unwrap();
        {
            let script = app.world().non_send_resource::<UiScript>();
            assert_eq!(
                script.eval::<i64>("return SEEN").unwrap(),
                1,
                "the timed-out kill still reaches the combat log"
            );
            assert_eq!(
                script.eval::<String>("return SOURCE").unwrap(),
                "Firna",
                "the resolved endpoint keeps its real name"
            );
            assert_eq!(
                script.eval::<String>("return DEST").unwrap(),
                "UNKNOWNOBJECT",
                "the unresolved endpoint is marked, never borrowed or guessed"
            );
            assert_eq!(
                app.world().resource::<WrathWorldView>().party_kills.len(),
                0,
                "the delivered row is retired"
            );
            assert_eq!(
                app.world()
                    .resource::<WrathWorldView>()
                    .party_kill_unresolved,
                1,
                "the fallback counts as a resolved-with-fallback delivery"
            );
        }

        // A second feed has nothing left: the retry/fallback path must not duplicate the event.
        app.world_mut().run_system_once(feed).unwrap();
        {
            let script = app.world().non_send_resource::<UiScript>();
            assert_eq!(
                script.eval::<i64>("return SEEN").unwrap(),
                1,
                "the fallback event is delivered exactly once"
            );
        }
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
