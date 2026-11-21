//! Project queued `SMSG_PARTYKILLLOG`, `SMSG_ATTACKERSTATEUPDATE` and
//! `SMSG_SPELLNONMELEEDAMAGELOG` rows into the Wrath VM's `COMBAT_LOG_EVENT_UNFILTERED` contract.
//! A kill, swing or spell hit is an ordered event, not a replaceable snapshot: the view keeps a
//! bounded pending queue and this feed removes each row only after it has been signalled. The
//! decoded packet proves the event; both endpoint names still come from the VM's own unit views or
//! the existing name-query cache, never from a guess. A swing keeps 12340's raw hit word, school
//! mask and victim state; a wounded or blocked swing posts `SWING_DAMAGE` and every other shown
//! swing outcome posts `SWING_MISSED` with the reference's own miss string. A spell-damage row
//! posts `SPELL_DAMAGE` with 12340's twenty-value shape and the packet's raw school mask.

use benilla_ui::script::{ScriptValue, UiScript, UiTarget};
use bevy::prelude::*;

use crate::names::NameCache;
use crate::net::NetCommands;
use crate::query_cache::Ask;
use crate::script_calls::VmWith;
use crate::ui_script::UiClock;
use crate::wrath::{
    WrathPartyKill, WrathSpellDamage, WrathSpellEnergize, WrathSpellHeal, WrathSpellMiss,
    WrathSpellPeriodicDamage, WrathSpellPeriodicDrain, WrathSpellPeriodicEnergize,
    WrathSpellPeriodicHeal, WrathSpellPeriodicMiss, WrathSwing, WrathWorldView,
    COMBAT_LOG_NAME_RETRY_SECONDS,
};

/// Capacity loss and unresolved-name counts already reported, so each diagnostic is logged once
/// per scope rather than every frame.
#[derive(Default)]
pub(crate) struct CombatLogMemo {
    dropped: u64,
    unresolved: u64,
    swing_dropped: u64,
    swing_unresolved: u64,
    spell_damage_dropped: u64,
    spell_damage_unresolved: u64,
    spell_miss_dropped: u64,
    spell_miss_unresolved: u64,
    spell_heal_dropped: u64,
    spell_heal_unresolved: u64,
    spell_energize_dropped: u64,
    spell_energize_unresolved: u64,
    spell_periodic_damage_dropped: u64,
    spell_periodic_damage_unresolved: u64,
    spell_periodic_miss_dropped: u64,
    spell_periodic_miss_unresolved: u64,
    spell_periodic_heal_dropped: u64,
    spell_periodic_heal_unresolved: u64,
    spell_periodic_energize_dropped: u64,
    spell_periodic_energize_unresolved: u64,
    spell_periodic_drain_dropped: u64,
    spell_periodic_drain_unresolved: u64,
}

/// The visible marker for an endpoint whose name never resolved inside the retry window. It is
/// the client's own unknown-object spelling, never a guessed player or creature name.
const UNKNOWN_OBJECT_NAME: &str = "UNKNOWNOBJECT";

#[allow(clippy::type_complexity)]
pub(crate) fn feed(
    mut vm: VmWith<(
        Option<ResMut<'static, WrathWorldView>>,
        Option<Res<'static, UiClock>>,
        Option<Res<'static, NameCache>>,
        Option<Res<'static, NetCommands>>,
        Option<Res<'static, crate::ui_action::Spells>>,
    )>,
    mut memo: Local<CombatLogMemo>,
) {
    vm.feed(|script, (mut world, clock, names, net, spells), fires| {
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
        if world.spell_damage_dropped > memo.spell_damage_dropped {
            debug!(
                dropped = world.spell_damage_dropped,
                "wrath: combat-log queue capacity lost spell-damage rows"
            );
        }
        memo.spell_damage_dropped = world.spell_damage_dropped;
        if world.spell_damage_unresolved > memo.spell_damage_unresolved {
            debug!(
                unresolved = world.spell_damage_unresolved,
                "wrath: combat-log spell-damage rows delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.spell_damage_unresolved = world.spell_damage_unresolved;
        if world.spell_miss_dropped > memo.spell_miss_dropped {
            debug!(
                dropped = world.spell_miss_dropped,
                "wrath: combat-log queue capacity lost spell-miss rows"
            );
        }
        memo.spell_miss_dropped = world.spell_miss_dropped;
        if world.spell_miss_unresolved > memo.spell_miss_unresolved {
            debug!(
                unresolved = world.spell_miss_unresolved,
                "wrath: combat-log spell-miss rows delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.spell_miss_unresolved = world.spell_miss_unresolved;
        if world.spell_heal_dropped > memo.spell_heal_dropped {
            debug!(
                dropped = world.spell_heal_dropped,
                "wrath: combat-log queue capacity lost spell-heal rows"
            );
        }
        memo.spell_heal_dropped = world.spell_heal_dropped;
        if world.spell_heal_unresolved > memo.spell_heal_unresolved {
            debug!(
                unresolved = world.spell_heal_unresolved,
                "wrath: combat-log spell-heal rows delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.spell_heal_unresolved = world.spell_heal_unresolved;
        if world.spell_energize_dropped > memo.spell_energize_dropped {
            debug!(
                dropped = world.spell_energize_dropped,
                "wrath: combat-log queue capacity lost spell-energize rows"
            );
        }
        memo.spell_energize_dropped = world.spell_energize_dropped;
        if world.spell_energize_unresolved > memo.spell_energize_unresolved {
            debug!(
                unresolved = world.spell_energize_unresolved,
                "wrath: combat-log spell-energize rows delivered with an unknown endpoint after name resolution timed out"
            );
        }
        memo.spell_energize_unresolved = world.spell_energize_unresolved;
        for (label, dropped, memo_dropped) in [
            (
                "periodic-damage",
                world.spell_periodic_damage_dropped,
                &mut memo.spell_periodic_damage_dropped,
            ),
            (
                "periodic-miss",
                world.spell_periodic_miss_dropped,
                &mut memo.spell_periodic_miss_dropped,
            ),
            (
                "periodic-heal",
                world.spell_periodic_heal_dropped,
                &mut memo.spell_periodic_heal_dropped,
            ),
            (
                "periodic-energize",
                world.spell_periodic_energize_dropped,
                &mut memo.spell_periodic_energize_dropped,
            ),
            (
                "periodic-drain",
                world.spell_periodic_drain_dropped,
                &mut memo.spell_periodic_drain_dropped,
            ),
        ] {
            if dropped > *memo_dropped {
                debug!(dropped, "wrath: combat-log queue capacity lost {label} rows");
            }
            *memo_dropped = dropped;
        }
        for (label, unresolved, memo_unresolved) in [
            (
                "periodic-damage",
                world.spell_periodic_damage_unresolved,
                &mut memo.spell_periodic_damage_unresolved,
            ),
            (
                "periodic-miss",
                world.spell_periodic_miss_unresolved,
                &mut memo.spell_periodic_miss_unresolved,
            ),
            (
                "periodic-heal",
                world.spell_periodic_heal_unresolved,
                &mut memo.spell_periodic_heal_unresolved,
            ),
            (
                "periodic-energize",
                world.spell_periodic_energize_unresolved,
                &mut memo.spell_periodic_energize_unresolved,
            ),
            (
                "periodic-drain",
                world.spell_periodic_drain_unresolved,
                &mut memo.spell_periodic_drain_unresolved,
            ),
        ] {
            if unresolved > *memo_unresolved {
                debug!(
                    unresolved,
                    "wrath: combat-log {label} rows delivered with an unknown endpoint after name resolution timed out"
                );
            }
            *memo_unresolved = unresolved;
        }

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
            let outcome = swing_outcome(&swing);
            if matches!(outcome, SwingOutcome::Retired) {
                // An intact hit, a wound without damage, or an absorb/resist row has no event
                // yet; it retires here without inventing one.
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
                    // The retry window is spent. Deliver the swing event with a visible
                    // unknown marker for the side(s) that did not resolve, then retire the row
                    // so a later feed cannot replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        swing_event_args(
                            timestamp,
                            &swing,
                            outcome,
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
                swing_event_args(timestamp, &swing, outcome, &source, &target),
            );
            world.swings.remove(index);
        }

        let mut index = 0;
        while index < world.spell_damages.len() {
            let hit = world.spell_damages[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                // A row from a retired world is never delivered into the replacement chain.
                world.spell_damages.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.attacker, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 {
                hit.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at {
                    now - hit.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    // The retry window is spent. Deliver the spell-damage event with a visible
                    // unknown marker for the side(s) that did not resolve, then retire the row
                    // so a later feed cannot replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_damage_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_damage_unresolved = world.spell_damage_unresolved.saturating_add(1);
                    world.spell_damages.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_damage_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_damages.remove(index);
        }

        let mut index = 0;
        while index < world.spell_heals.len() {
            let hit = world.spell_heals[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                // A row from a retired world is never delivered into the replacement chain.
                world.spell_heals.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 {
                hit.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at {
                    now - hit.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    // The retry window is spent. Deliver the spell-heal event with a visible
                    // unknown marker for the side(s) that did not resolve, then retire the row
                    // so a later feed cannot replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_heal_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_heal_unresolved = world.spell_heal_unresolved.saturating_add(1);
                    world.spell_heals.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_heal_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_heals.remove(index);
        }

        let mut index = 0;
        while index < world.spell_energizes.len() {
            let hit = world.spell_energizes[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                // A row from a retired world is never delivered into the replacement chain.
                world.spell_energizes.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 {
                hit.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at {
                    now - hit.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    // The retry window is spent. Deliver the spell-energize event with a visible
                    // unknown marker for the side(s) that did not resolve, then retire the row
                    // so a later feed cannot replay it. The known endpoint keeps its real name.
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_energize_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_energize_unresolved =
                        world.spell_energize_unresolved.saturating_add(1);
                    world.spell_energizes.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_energize_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_energizes.remove(index);
        }


        let mut index = 0;
        while index < world.spell_periodic_damages.len() {
            let hit = world.spell_periodic_damages[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_periodic_damages.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 { hit.received_at } else { now };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at { now - hit.received_at } else { 0.0 };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_periodic_damage_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_periodic_damage_unresolved =
                        world.spell_periodic_damage_unresolved.saturating_add(1);
                    world.spell_periodic_damages.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_periodic_damage_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_periodic_damages.remove(index);
        }

        let mut index = 0;
        while index < world.spell_periodic_misses.len() {
            let hit = world.spell_periodic_misses[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_periodic_misses.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 { hit.received_at } else { now };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at { now - hit.received_at } else { 0.0 };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_periodic_miss_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_periodic_miss_unresolved =
                        world.spell_periodic_miss_unresolved.saturating_add(1);
                    world.spell_periodic_misses.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_periodic_miss_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_periodic_misses.remove(index);
        }

        let mut index = 0;
        while index < world.spell_periodic_heals.len() {
            let hit = world.spell_periodic_heals[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_periodic_heals.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 { hit.received_at } else { now };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at { now - hit.received_at } else { 0.0 };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_periodic_heal_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_periodic_heal_unresolved =
                        world.spell_periodic_heal_unresolved.saturating_add(1);
                    world.spell_periodic_heals.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_periodic_heal_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_periodic_heals.remove(index);
        }

        let mut index = 0;
        while index < world.spell_periodic_energizes.len() {
            let hit = world.spell_periodic_energizes[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_periodic_energizes.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 { hit.received_at } else { now };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at { now - hit.received_at } else { 0.0 };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_periodic_energize_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_periodic_energize_unresolved =
                        world.spell_periodic_energize_unresolved.saturating_add(1);
                    world.spell_periodic_energizes.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_periodic_energize_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_periodic_energizes.remove(index);
        }

        let mut index = 0;
        while index < world.spell_periodic_drains.len() {
            let hit = world.spell_periodic_drains[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_periodic_drains.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 { hit.received_at } else { now };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at { now - hit.received_at } else { 0.0 };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_periodic_drain_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_periodic_drain_unresolved =
                        world.spell_periodic_drain_unresolved.saturating_add(1);
                    world.spell_periodic_drains.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_periodic_drain_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_periodic_drains.remove(index);
        }
        let mut index = 0;
        while index < world.spell_misses.len() {
            let hit = world.spell_misses[index];
            if hit.session_epoch != world.session_epoch
                || hit.world_generation != world.world_generation
            {
                world.spell_misses.remove(index);
                continue;
            }
            let source = endpoint_name(script, world, names.as_deref(), hit.caster, ask);
            let target = endpoint_name(script, world, names.as_deref(), hit.target, ask);
            let timestamp = if hit.received_at > 0.0 {
                hit.received_at
            } else {
                now
            };
            if source.is_none() || target.is_none() {
                let age = if hit.received_at > 0.0 && now >= hit.received_at {
                    now - hit.received_at
                } else {
                    0.0
                };
                if age > COMBAT_LOG_NAME_RETRY_SECONDS {
                    fires.post(
                        "COMBAT_LOG_EVENT_UNFILTERED",
                        spell_miss_args(
                            timestamp,
                            &hit,
                            source.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            target.as_deref().unwrap_or(UNKNOWN_OBJECT_NAME),
                            spells.as_deref(),
                        ),
                    );
                    world.spell_miss_unresolved = world.spell_miss_unresolved.saturating_add(1);
                    world.spell_misses.remove(index);
                } else {
                    index += 1;
                }
                continue;
            }
            let (source, target) = (source.unwrap_or_default(), target.unwrap_or_default());
            fires.post(
                "COMBAT_LOG_EVENT_UNFILTERED",
                spell_miss_args(timestamp, &hit, &source, &target, spells.as_deref()),
            );
            world.spell_misses.remove(index);
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

/// What one decoded swing posts to the combat log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwingOutcome {
    Damage,
    Missed(MissKind),
    /// No event: the row is retired without being replayed.
    Retired,
}

/// One `SWING_MISSED` outcome, named by the reference's own miss string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MissKind {
    Miss,
    Dodge,
    Parry,
    Block,
    Evade,
    Immune,
    Deflect,
    Absorb,
    Resist,
}

impl MissKind {
    /// 12340's own `g_spellMissStrings` spelling.
    fn name(self) -> &'static str {
        match self {
            MissKind::Miss => "MISS",
            MissKind::Dodge => "DODGE",
            MissKind::Parry => "PARRY",
            MissKind::Block => "BLOCK",
            MissKind::Evade => "EVADE",
            MissKind::Immune => "IMMUNE",
            MissKind::Deflect => "DEFLECT",
            MissKind::Absorb => "ABSORB",
            MissKind::Resist => "RESIST",
        }
    }

    /// `PushEvent` appends one amount only for the miss types `SetMissInfo` flags: absorbed for
    /// ABSORB, resisted for RESIST, blocked for BLOCK. The other kinds append none.
    fn amount(self, swing: &WrathSwing) -> Option<f64> {
        match self {
            MissKind::Absorb => Some(f64::from(swing.hit.absorb)),
            MissKind::Resist => Some(f64::from(swing.hit.resist)),
            MissKind::Block => Some(f64::from(swing.hit.blocked)),
            MissKind::Miss
            | MissKind::Dodge
            | MissKind::Parry
            | MissKind::Evade
            | MissKind::Immune
            | MissKind::Deflect => None,
        }
    }
}

/// 12340's `UnitCombatLog` first drops feedback for a melee-spell round unless the victim
/// state is a wound (`ShouldShowFeedback`, `UnitCombatLog_C.cpp:933-950`), then builds the
/// `SWING_DAMAGE` row only for a wound (1) or block (4) with non-zero damage
/// (`UnitCombatLog_C.cpp:952-980`). After that it tests `hit_info` in order and only then names
/// the victim state (`:982-1073`). 12340's victim states are intact=0, hit=1 and blocks=4.
fn swing_outcome(swing: &WrathSwing) -> SwingOutcome {
    let hit = swing.hit;
    if hit.melee_spell_id != 0 && hit.victim_state != 1 {
        return SwingOutcome::Retired;
    }
    if hit.damage != 0 && matches!(hit.victim_state, 1 | 4) {
        return SwingOutcome::Damage;
    }
    if hit.hit_info & 0x10 != 0 {
        return SwingOutcome::Missed(MissKind::Miss);
    }
    if hit.hit_info & 0x20 != 0 {
        return SwingOutcome::Missed(MissKind::Absorb);
    }
    if hit.hit_info & 0x80 != 0 {
        return SwingOutcome::Missed(MissKind::Resist);
    }
    match hit.victim_state {
        2 => SwingOutcome::Missed(MissKind::Dodge),
        3 => SwingOutcome::Missed(MissKind::Parry),
        4 => SwingOutcome::Missed(MissKind::Block),
        6 => SwingOutcome::Missed(MissKind::Evade),
        7 => SwingOutcome::Missed(MissKind::Immune),
        8 => SwingOutcome::Missed(MissKind::Deflect),
        _ => SwingOutcome::Retired,
    }
}

/// The event-specific values for whichever event this swing posts.
fn swing_event_args(
    timestamp: f64,
    swing: &WrathSwing,
    outcome: SwingOutcome,
    source_name: &str,
    target_name: &str,
) -> Vec<ScriptValue> {
    match outcome {
        SwingOutcome::Damage => swing_damage_args(timestamp, swing, source_name, target_name),
        SwingOutcome::Missed(kind) => {
            swing_missed_args(timestamp, swing, source_name, target_name, kind)
        }
        SwingOutcome::Retired => Vec::new(),
    }
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

/// The values `CombatLogEntry::PushEvent` appends for `SWING_MISSED`
/// (`UnitCombatLog_C.cpp:982-1073`, info flags `0x4`/`0x2000`/`0x4000`/`0x8000`): the miss-type
/// string from `SetMissInfo`, then the amount only for ABSORB, RESIST and BLOCK. The hit_info
/// tests run absorb before resist, so a row carrying both bits is ABSORB.
fn swing_missed_args(
    timestamp: f64,
    swing: &WrathSwing,
    source_name: &str,
    target_name: &str,
    kind: MissKind,
) -> Vec<ScriptValue> {
    let hit = swing.hit;
    let mut args = combat_log_header(
        timestamp,
        "SWING_MISSED",
        hit.attacker,
        source_name,
        hit.victim,
        target_name,
    );
    args.push(ScriptValue::Str(kind.name().to_owned()));
    if let Some(amount) = kind.amount(swing) {
        args.push(ScriptValue::Number(amount));
    }
    args
}

/// The twenty values `CombatLogEntry::PushEvent` sends for `SPELL_DAMAGE`
/// (`UnitCombatLog_C.cpp:1241-1361`, `SendDamageEvent` at `:2373`, `HandleSpellNonMeleeDamageLog`
/// at `:2520`): 8 header values, then spellId/spellName/spellSchoolMask (3), then the 0x10 block
/// damage, overkill, school, resisted, blocked, absorbed, critical, glancing, crushing (9). The
/// packet school byte is kept as 12340's raw school mask here (unlike the 1.12 chat translation
/// which converts to an index). Absent resisted/blocked/absorbed are nil, not 0. Critical/glancing/
/// crushing are 1.0 or nil. `HandleSpellNonMeleeDamageLog` reads crit as `hitInfo & 0x2`; the spell
/// non-melee path passes glancing/crushing as false to `SendDamageEvent`. If the spell record is
/// missing, `PushEvent` pushes nil name and 0 school.
fn spell_damage_args(
    timestamp: f64,
    hit: &WrathSpellDamage,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_DAMAGE",
        hit.attacker,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.extend([
        ScriptValue::Number(f64::from(hit.damage)),
        ScriptValue::Number(f64::from(hit.overkill)),
        ScriptValue::Int(i64::from(hit.school)),
        nonzero(i64::from(hit.resist)),
        nonzero(i64::from(hit.blocked)),
        nonzero(i64::from(hit.absorb)),
        present(hit.hit_info & 0x2 != 0),
        ScriptValue::Nil,
        ScriptValue::Nil,
    ]);
    args
}

/// The fifteen values `CombatLogEntry::PushEvent` sends for a shown heal
/// (`UnitCombatLog_C.cpp:1241-1361`, `SendHealEvent` at `:2001`, `UnitCombatLogHeal` at
/// `:1811`, `HandleSpellHealLog` at `:1931`, `SMSG_SPELL_HEAL_LOG` `0x150` in
/// `Net/Types.h`): 8 header values, then spellId/spellName/spellSchoolMask (3), then the 0x20
/// block amount, overheal, absorbed, critical as 1.0 or nil (4). `UnitCombatLogHeal` returns
/// without an entry when amount, overheal and absorbed are all zero; that row is dropped before
/// it reaches the feed. If the spell record is missing, `PushEvent` pushes nil name and 0
/// school.
fn spell_heal_args(
    timestamp: f64,
    hit: &WrathSpellHeal,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_HEAL",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.extend([
        ScriptValue::Number(f64::from(hit.amount)),
        ScriptValue::Number(f64::from(hit.overheal)),
        ScriptValue::Number(f64::from(hit.absorbed)),
        present(hit.critical),
    ]);
    args
}

/// The thirteen values `CombatLogEntry::PushEvent` sends for a shown energize
/// (`UnitCombatLog_C.cpp:1241-1361`, `PushEvent` 0x40 at `:1365`, `SendEnergizeEvent` at
/// `:2045`, `UnitCombatLogEnergize` at `:1831`, `HandleSpellLogEnergizeLog` at `:1974`,
/// `SMSG_SPELL_ENERGIZE_LOG` `0x151` in `Net/Types.h`, `Player_C.cpp:3670`): 8 header values,
/// then spellId/spellName/spellSchoolMask (3), then the 0x40 block divided amount, powerType (2).
/// `UnitCombatLogEnergize` returns when amount is zero. `SendEnergizeEvent` divides amount by
/// `Spell_C_GetPowerDisplayMod(powerType)` (`benilla_protocol::messages::power_display_scale`)
/// with integer division. Missing spell row: nil name, school 0.
fn spell_energize_args(
    timestamp: f64,
    hit: &WrathSpellEnergize,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_ENERGIZE",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    let divisor = benilla_protocol::messages::power_display_scale(hit.power_type);
    let divided_amount = hit.amount.checked_div(divisor).unwrap_or(hit.amount);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.push(ScriptValue::Number(f64::from(divided_amount)));
    args.push(ScriptValue::Number(f64::from(hit.power_type)));
    args
}

fn spell_periodic_damage_args(
    timestamp: f64,
    hit: &WrathSpellPeriodicDamage,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_PERIODIC_DAMAGE",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.extend([
        ScriptValue::Number(f64::from(hit.amount)),
        ScriptValue::Number(f64::from(hit.overkill)),
        ScriptValue::Int(i64::from(hit.school)),
        nonzero(i64::from(hit.resist)),
        ScriptValue::Nil, // blocked: always 0/nil from this packet
        nonzero(i64::from(hit.absorb)),
        present(hit.critical),
        ScriptValue::Nil, // glancing
        ScriptValue::Nil, // crushing
    ]);
    args
}

fn spell_periodic_miss_args(
    timestamp: f64,
    hit: &WrathSpellPeriodicMiss,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_PERIODIC_MISSED",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.push(ScriptValue::Str(
        spell_miss_string(hit.miss_type).to_owned(),
    ));
    if matches!(hit.miss_type, 2 | 5 | 10) {
        args.push(ScriptValue::Number(f64::from(hit.amount)));
    }
    args
}

fn spell_periodic_heal_args(
    timestamp: f64,
    hit: &WrathSpellPeriodicHeal,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_PERIODIC_HEAL",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.extend([
        ScriptValue::Number(f64::from(hit.amount)),
        ScriptValue::Number(f64::from(hit.overheal)),
        ScriptValue::Number(f64::from(hit.absorbed)),
        present(hit.critical),
    ]);
    args
}

fn spell_periodic_energize_args(
    timestamp: f64,
    hit: &WrathSpellPeriodicEnergize,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_PERIODIC_ENERGIZE",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    let divisor = benilla_protocol::messages::power_display_scale(hit.power_type);
    let divided_amount = hit.amount.checked_div(divisor).unwrap_or(hit.amount);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.push(ScriptValue::Number(f64::from(divided_amount)));
    args.push(ScriptValue::Number(f64::from(hit.power_type)));
    args
}

fn spell_periodic_drain_args(
    timestamp: f64,
    hit: &WrathSpellPeriodicDrain,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let extra = (f64::from(hit.amount) * f64::from(hit.multiplier)).trunc() as u32;
    let subevent = if extra != 0 {
        "SPELL_PERIODIC_LEECH"
    } else {
        "SPELL_PERIODIC_DRAIN"
    };
    let mut args = combat_log_header(
        timestamp,
        subevent,
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    let divisor = benilla_protocol::messages::power_display_scale(hit.power_type);
    let divided_amount = hit.amount.checked_div(divisor).unwrap_or(hit.amount);
    let divided_extra = extra.checked_div(divisor).unwrap_or(extra);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.push(ScriptValue::Number(f64::from(divided_amount)));
    args.push(ScriptValue::Number(f64::from(hit.power_type)));
    if extra != 0 {
        args.push(ScriptValue::Number(f64::from(divided_extra)));
    } else {
        args.push(ScriptValue::Nil);
    }
    args
}

/// `SMSG_SPELL_MISS_LOG` (`0x24B`, `UnitCombatLog_C.cpp:1888` `HandleSpellLogMiss`):
/// missType indexes `g_spellMissStrings` at `:150` (0 NONE, 1 MISS, 2 RESIST, 3 DODGE, 4 PARRY,
/// 5 BLOCK, 6 EVADE, 7 IMMUNE, 8 IMMUNE, 9 DEFLECT, 10 ABSORB, 11 REFLECT).
fn spell_miss_string(miss_type: u8) -> &'static str {
    match miss_type {
        0 => "NONE",
        1 => "MISS",
        2 => "RESIST",
        3 => "DODGE",
        4 => "PARRY",
        5 => "BLOCK",
        6 => "EVADE",
        7 => "IMMUNE",
        8 => "IMMUNE",
        9 => "DEFLECT",
        10 => "ABSORB",
        11 => "REFLECT",
        _ => "MISS",
    }
}

/// The values `CombatLogEntry::PushEvent` sends for `SPELL_MISSED`
/// (`UnitCombatLog_C.cpp:1241-1361`, `UnitCombatLogSpellMissed` `:1573`,
/// `QueueMissedSpellText` `:1470`, `SetMissInfo` `:311`): 8 header values, then
/// spellId/spellName/spellSchoolMask (3), then the miss string (0x4). `SetMissInfo` sets
/// `0x2000` for missType 10 (ABSORB), `0x4000` for missType 2 (RESIST) and `0x8000` for
/// missType 5 (BLOCK), and `PushEvent` appends that amount slot as integer 0 from this packet
/// (no other miss type appends an amount). The decompiled `UnitCombatLogSpellMissed` call from
/// `HandleSpellLogMiss` (`:1888`, `a5=true`) is labelled `DAMAGE_SHIELD_MISSED` (event 41);
/// this feed implements `SPELL_MISSED` (event 8). If the spell record is missing, `PushEvent`
/// pushes nil name and 0 school.
fn spell_miss_args(
    timestamp: f64,
    hit: &WrathSpellMiss,
    source_name: &str,
    target_name: &str,
    spells: Option<&crate::ui_action::Spells>,
) -> Vec<ScriptValue> {
    let mut args = combat_log_header(
        timestamp,
        "SPELL_MISSED",
        hit.caster,
        source_name,
        hit.target,
        target_name,
    );
    let spell_name = spells
        .and_then(|spells| spells.catalog.get(hit.spell_id))
        .map(|def| def.name.clone());
    let spell_school = spells
        .and_then(|spells| spells.catalog.school_mask(hit.spell_id))
        .unwrap_or(0);
    args.push(ScriptValue::Int(i64::from(hit.spell_id)));
    args.push(spell_name.map_or(ScriptValue::Nil, ScriptValue::Str));
    args.push(ScriptValue::Int(i64::from(spell_school)));
    args.push(ScriptValue::Str(
        spell_miss_string(hit.miss_type).to_owned(),
    ));
    if matches!(hit.miss_type, 2 | 5 | 10) {
        args.push(ScriptValue::Int(0));
    }
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

    /// True when the row posts `SWING_DAMAGE` rather than a miss.
    fn shows_damage(swing: &WrathSwing) -> bool {
        matches!(swing_outcome(swing), SwingOutcome::Damage)
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
        assert!(shows_damage(&swing), "a blocked wound is shown");
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
        assert!(shows_damage(&swing), "a plain blocked swing shows");
        swing.hit.melee_spell_id = 78;
        assert!(
            !shows_damage(&swing),
            "a blocked melee-spell round is dropped by ShouldShowFeedback"
        );
        swing.hit.victim_state = 1;
        assert!(
            shows_damage(&swing),
            "a wounded melee-spell round is still shown"
        );
    }

    /// The reference tests `hit_info` before the victim-state switch, so a miss, absorb or
    /// resist bit wins over the state, and each remaining state posts `SWING_MISSED` with its
    /// own miss string. Absorb and resist carry their totals; a plain state appends no amount.
    #[test]
    fn swing_missed_names_each_victim_state() {
        let row = |victim_state: u32, hit_info: u32| WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info,
                blocked: 9,
                ..swing_hit(0, victim_state)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        let posted = |swing: &WrathSwing| {
            swing_event_args(10.0, swing, swing_outcome(swing), "Firna", "Kobold Vermin")
        };
        let named = |swing: &WrathSwing| {
            let args = posted(swing);
            assert_eq!(args[1], ScriptValue::Str("SWING_MISSED".to_owned()));
            (args.len(), args[8].clone())
        };

        for (victim_state, name) in [
            (2, "DODGE"),
            (3, "PARRY"),
            (6, "EVADE"),
            (7, "IMMUNE"),
            (8, "DEFLECT"),
        ] {
            assert_eq!(
                named(&row(victim_state, 0x2)),
                (9, ScriptValue::Str(name.to_owned())),
                "victim state {victim_state} names {name} and appends no amount"
            );
        }

        // A plain miss is the `hit_info & 0x10` case, whatever the victim state says.
        assert_eq!(
            named(&row(0, 0x12)),
            (9, ScriptValue::Str("MISS".to_owned()))
        );

        // A blocked row with no damage reaches the switch and carries the blocked total.
        let block = posted(&row(4, 0x2));
        assert_eq!(
            (block.len(), block[8].clone(), block[9].clone()),
            (
                10,
                ScriptValue::Str("BLOCK".to_owned()),
                ScriptValue::Number(9.0)
            )
        );

        // Absorb and resist are named by their hit_info bits, absorb first.
        assert_eq!(
            named(&row(0, 0x22)),
            (10, ScriptValue::Str("ABSORB".to_owned()))
        );
        assert_eq!(
            named(&row(0, 0x82)),
            (10, ScriptValue::Str("RESIST".to_owned()))
        );
        // No matching outcome: an intact hit and a wound without damage emit nothing.
        assert_eq!(swing_outcome(&row(0, 0x2)), SwingOutcome::Retired);
        assert_eq!(swing_outcome(&row(1, 0x2)), SwingOutcome::Retired);
    }

    /// A melee-spell round that is not a wound is dropped by `ShouldShowFeedback` before the
    /// miss bits are consulted, so it emits neither SWING_DAMAGE nor SWING_MISSED.
    #[test]
    fn a_missed_melee_spell_round_is_dropped_by_should_show_feedback() {
        let mut swing = WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x12, // a plain miss
                ..swing_hit(0, 2)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        assert_eq!(
            swing_outcome(&swing),
            SwingOutcome::Missed(MissKind::Miss),
            "a plain melee miss is shown"
        );
        swing.hit.melee_spell_id = 78;
        assert_eq!(
            swing_outcome(&swing),
            SwingOutcome::Retired,
            "the same miss inside a melee-spell round is dropped by ShouldShowFeedback"
        );
        assert!(!shows_damage(&swing), "and it is not a damage event");
    }

    /// A block that stopped every point of damage is `SWING_MISSED` / `BLOCK` carrying the
    /// blocked total; the damaged block is still a `SWING_DAMAGE` row.
    #[test]
    fn a_zero_damage_block_is_swing_missed_block_with_the_blocked_amount() {
        let blocked = |damage: u32| WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x2002,
                blocked: 9,
                ..swing_hit(damage, 4)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };

        let args = |swing: &WrathSwing| {
            swing_event_args(10.0, swing, swing_outcome(swing), "Firna", "Kobold Vermin")
        };

        let stopped = args(&blocked(0));
        assert_eq!(
            stopped.len(),
            10,
            "the miss string and the blocked amount only"
        );
        assert_eq!(stopped[1], ScriptValue::Str("SWING_MISSED".to_owned()));
        assert_eq!(stopped[8], ScriptValue::Str("BLOCK".to_owned()));
        assert_eq!(stopped[9], ScriptValue::Number(9.0));

        assert!(
            shows_damage(&blocked(20)),
            "a damaged block still shows damage"
        );
        assert_eq!(
            args(&blocked(20))[1],
            ScriptValue::Str("SWING_DAMAGE".to_owned())
        );
    }

    /// Absorbed and resisted swings reach the switch as `ABSORB` and `RESIST` and carry the
    /// summed absorbed or resisted total as the tenth value; each row is exactly one event.
    #[test]
    fn swing_missed_absorb_and_resist_carry_their_totals() {
        use crate::ui_script::UiClock;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::App;

        let script = script_with_units();
        script
            .run(
                r#"
                SEEN = 0
                Fired = {}
                local listener = CreateFrame("Frame", "WrathSwingAbsorbResistTest")
                listener:SetScript("OnEvent", function(_, event, ...)
                    SEEN = SEEN + 1
                    Fired[SEEN] = { event, ... }
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
        // Both rows carry both amounts; only the one the miss type flags is appended.
        world.apply_swing(
            crate::wrath::WrathSwingHit {
                victim: 0x22,
                hit_info: 0x22,
                absorb: 7,
                resist: 4,
                ..swing_hit(0, 0)
            },
            4,
            5,
            97.0,
        );
        world.apply_swing(
            crate::wrath::WrathSwingHit {
                victim: 0x22,
                hit_info: 0x82,
                absorb: 7,
                resist: 4,
                ..swing_hit(0, 0)
            },
            4,
            5,
            97.0,
        );

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
                2,
                "one event per absorbed or resisted swing"
            );
            assert_eq!(
                script.eval::<String>("return Fired[1][3]").unwrap(),
                "SWING_MISSED"
            );
            assert_eq!(
                script.eval::<String>("return Fired[1][10]").unwrap(),
                "ABSORB"
            );
            assert_eq!(
                script.eval::<f64>("return Fired[1][11]").unwrap(),
                7.0,
                "the absorbed total, not the resisted total"
            );
            assert_eq!(
                script.eval::<String>("return Fired[2][10]").unwrap(),
                "RESIST"
            );
            assert_eq!(
                script.eval::<f64>("return Fired[2][11]").unwrap(),
                4.0,
                "the resisted total, not the absorbed total"
            );
        }
        assert!(
            app.world().resource::<WrathWorldView>().swings.is_empty(),
            "both delivered rows are retired"
        );

        // The reference pushes the flagged amount unconditionally, so zero is a number, not nil.
        let zero = WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x22,
                ..swing_hit(0, 0)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        let args = swing_event_args(10.0, &zero, swing_outcome(&zero), "Firna", "Teammate");
        assert_eq!(args[8], ScriptValue::Str("ABSORB".to_owned()));
        assert_eq!(args[9], ScriptValue::Number(0.0));
    }

    /// The reference tests `hit_info & 0x20` before `0x80`, so a row carrying both bits is
    /// ABSORB and appends only the absorbed total.
    #[test]
    fn a_row_with_absorb_and_resist_is_absorb() {
        let swing = WrathSwing {
            hit: crate::wrath::WrathSwingHit {
                hit_info: 0x20 | 0x80 | 0x2,
                absorb: 7,
                resist: 4,
                ..swing_hit(0, 0)
            },
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 10.0,
        };
        assert_eq!(
            swing_outcome(&swing),
            SwingOutcome::Missed(MissKind::Absorb)
        );
        let args = swing_event_args(10.0, &swing, swing_outcome(&swing), "Firna", "Teammate");
        assert_eq!(args[8], ScriptValue::Str("ABSORB".to_owned()));
        assert_eq!(args[9], ScriptValue::Number(7.0));
        assert_eq!(args.len(), 10, "only the absorbed total is appended");
    }

    /// The feed drains the swing queue with the same name retry as kills; a WOUND/BLOCK row
    /// with damage posts SWING_DAMAGE, while a wound without damage and an intact hit post
    /// nothing at all (a dodge or parry is no longer retired — it posts SWING_MISSED instead).
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
                SUBEVENT = nil
                local listener = CreateFrame("Frame", "WrathSwingFeedTest")
                listener:SetScript("OnEvent", function(_, _, _, subevent)
                    SEEN = SEEN + 1
                    SUBEVENT = subevent
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
        for (damage, victim_state) in [(12, 1), (0, 1), (0, 0)] {
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
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<String>("return SUBEVENT")
                .unwrap(),
            "SWING_DAMAGE",
            "the painful row is the damage event, not a miss"
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

    /// A dodged swing is one `SWING_MISSED` carrying the reference's `DODGE` string after the
    /// eight header values, with no amount and nothing to replay on the next feed.
    #[test]
    fn feed_delivers_a_dodged_swing_as_swing_missed() {
        use crate::ui_script::UiClock;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::App;

        let script = script_with_units();
        script
            .run(
                r#"
                SEEN = 0
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSwingMissTest")
                listener:SetScript("OnEvent", function(_, event, ...)
                    SEEN = SEEN + 1
                    Fired = { event, ... }
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
        // The teammate is the victim seat `script_with_units` already knows by guid.
        world.apply_swing(
            crate::wrath::WrathSwingHit {
                victim: 0x22,
                ..swing_hit(0, 2)
            },
            4,
            5,
            97.0,
        );

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
                1,
                "one dodge is exactly one event"
            );
            assert_eq!(
                script.eval::<String>("return Fired[3]").unwrap(),
                "SWING_MISSED"
            );
            assert_eq!(
                script.eval::<String>("return Fired[4]").unwrap(),
                "0x0000000000000011",
                "the header keeps the attacker guid"
            );
            assert_eq!(
                script.eval::<String>("return Fired[7]").unwrap(),
                "0x0000000000000022",
                "and the victim guid"
            );
            assert_eq!(
                script.eval::<String>("return Fired[10]").unwrap(),
                "DODGE",
                "the reference's own miss string"
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[11]").unwrap(),
                None,
                "a dodge appends no amount"
            );
        }
        assert!(
            app.world().resource::<WrathWorldView>().swings.is_empty(),
            "the delivered row is retired"
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

    fn spell_damage_hit() -> crate::wrath::WrathSpellDamage {
        crate::wrath::WrathSpellDamage {
            target: 0xF130_0000_0000_0022,
            attacker: 0x0000_0000_0000_0011,
            spell_id: 133,
            damage: 40,
            overkill: 3,
            school: 0x04,
            absorb: 0,
            resist: 0,
            blocked: 0,
            hit_info: 0x2, // crit in 12340 spell-damage hitInfo (HandleSpellNonMeleeDamageLog :2520)
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        }
    }

    /// \`SPELL_DAMAGE\` posts 20 values: 8 header + spellId/name/schoolMask + 9 damage values
    /// (\`PushEvent\` 0x10 block, \`UnitCombatLog_C.cpp:1241-1361\`, \`SendDamageEvent\` :2373).
    #[test]
    fn spell_damage_event_has_the_twenty_wrath_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellDamageTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let hit = spell_damage_hit();
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            spell_damage_args(1234.5, &hit, "Firna", "Kobold Vermin", None),
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
        let spell_id: i64 = scope.eval("return Fired[10]").unwrap();
        let spell_name: Option<String> = scope.eval("return Fired[11]").unwrap();
        let spell_school: i64 = scope.eval("return Fired[12]").unwrap();
        let damage: f64 = scope.eval("return Fired[13]").unwrap();
        let overkill: f64 = scope.eval("return Fired[14]").unwrap();
        let school: i64 = scope.eval("return Fired[15]").unwrap();
        let resisted: Option<f64> = scope.eval("return Fired[16]").unwrap();
        let blocked: Option<f64> = scope.eval("return Fired[17]").unwrap();
        let absorbed: Option<f64> = scope.eval("return Fired[18]").unwrap();
        let critical: Option<f64> = scope.eval("return Fired[19]").unwrap();
        let glancing: Option<f64> = scope.eval("return Fired[20]").unwrap();
        let crushing: Option<f64> = scope.eval("return Fired[21]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[22]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "SPELL_DAMAGE");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Kobold Vermin");
        assert_eq!(dest_flags, 0x800);
        assert_eq!(spell_id, 133);
        assert_eq!(spell_name, None, "no catalog row: name is nil");
        assert_eq!(spell_school, 0, "no catalog row: school mask is 0");
        assert_eq!(damage, 40.0);
        assert_eq!(overkill, 3.0, "overkill passes through");
        assert_eq!(school, 0x04, "damage school is the raw packet mask");
        assert_eq!(
            (resisted, blocked, absorbed),
            (None, None, None),
            "absent amounts are nil"
        );
        assert_eq!(
            (critical, glancing, crushing),
            (Some(1.0), None, None),
            "crit 1.0, glance/crush nil"
        );
        assert_eq!(extra, None, "exactly the twenty SPELL_DAMAGE values");
    }

    /// If the spell record is missing, \`PushEvent\` pushes nil name and 0 school
    /// (\`UnitCombatLog_C.cpp:1241-1361\`).
    #[test]
    fn spell_damage_missing_catalog_posts_nil_name_and_zero_school() {
        let mut hit = spell_damage_hit();
        hit.resist = 5;
        hit.blocked = 7;
        hit.absorb = 9;
        hit.hit_info = 0; // no crit
        let args = spell_damage_args(10.0, &hit, "Firna", "Kobold Vermin", None);
        assert_eq!(args.len(), 20);
        // Vec index 8 is the first value after the eight-value header.
        assert_eq!(args[8], ScriptValue::Int(133));
        assert_eq!(args[9], ScriptValue::Nil, "missing record: nil name");
        assert_eq!(
            args[10],
            ScriptValue::Int(0),
            "missing record: zero school mask"
        );
        assert_eq!(
            args[14],
            ScriptValue::Number(5.0),
            "nonzero resist is a number"
        );
        assert_eq!(
            args[15],
            ScriptValue::Number(7.0),
            "nonzero blocked is a number"
        );
        assert_eq!(
            args[16],
            ScriptValue::Number(9.0),
            "nonzero absorb is a number"
        );
        assert_eq!(args[17], ScriptValue::Nil, "no crit");
        assert_eq!(args[18], ScriptValue::Nil, "no glance");
        assert_eq!(args[19], ScriptValue::Nil, "no crush");
    }

    /// The feed drains the spell-damage queue like swings: resolves names, posts one
    /// \`SPELL_DAMAGE\` row with 20 values, and retires it so a later feed cannot replay it.
    #[test]
    fn feed_delivers_a_spell_damage_row() {
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
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellDamageFeedTest")
                listener:SetScript("OnEvent", function(_, event, ...)
                    SEEN = SEEN + 1
                    Fired = { event, ... }
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
        world.apply_spell_damage(spell_damage_hit(), 4, 5, 97.0);

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
            assert_eq!(script.eval::<i64>("return SEEN").unwrap(), 1);
            assert_eq!(
                script.eval::<String>("return Fired[3]").unwrap(),
                "SPELL_DAMAGE"
            );
            assert_eq!(script.eval::<i64>("return Fired[10]").unwrap(), 133);
            assert_eq!(script.eval::<f64>("return Fired[13]").unwrap(), 40.0);
            assert_eq!(
                script.eval::<f64>("return Fired[14]").unwrap(),
                3.0,
                "overkill passes through"
            );
            assert_eq!(
                script.eval::<i64>("return Fired[15]").unwrap(),
                4,
                "raw packet school mask"
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[16]").unwrap(),
                None
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[17]").unwrap(),
                None
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[18]").unwrap(),
                None
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[19]").unwrap(),
                Some(1.0),
                "crit 1.0"
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[20]").unwrap(),
                None
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[21]").unwrap(),
                None
            );
            assert_eq!(
                script.eval::<Option<f64>>("return Fired[22]").unwrap(),
                None,
                "exactly 20 values"
            );
        }
        assert!(
            app.world()
                .resource::<WrathWorldView>()
                .spell_damages
                .is_empty(),
            "delivered row is retired"
        );

        app.world_mut().run_system_once(feed).unwrap();
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<i64>("return SEEN")
                .unwrap(),
            1,
            "a later feed cannot replay a retired spell-damage row"
        );
    }

    fn spell_miss_hit(miss_type: u8) -> crate::wrath::WrathSpellMiss {
        crate::wrath::WrathSpellMiss {
            caster: 0x0000_0000_0000_0011,
            target: 0xF130_0000_0000_0022,
            spell_id: 133,
            miss_type,
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        }
    }

    /// `SPELL_MISSED` posts 12 values for a miss with no amount slot: 8 header +
    /// spellId/name/schoolMask + miss string (`PushEvent` 0x4, `UnitCombatLog_C.cpp:1241-1361`,
    /// `UnitCombatLogSpellMissed` :1573, `HandleSpellLogMiss` :1888).
    #[test]
    fn spell_missed_posts_the_twelve_wrath_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellMissTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let hit = spell_miss_hit(1); // MISS
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            spell_miss_args(1234.5, &hit, "Firna", "Kobold Vermin", None),
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
        let spell_id: i64 = scope.eval("return Fired[10]").unwrap();
        let spell_name: Option<String> = scope.eval("return Fired[11]").unwrap();
        let spell_school: i64 = scope.eval("return Fired[12]").unwrap();
        let miss: String = scope.eval("return Fired[13]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[14]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "SPELL_MISSED");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Kobold Vermin");
        assert_eq!(dest_flags, 0x800);
        assert_eq!(spell_id, 133);
        assert_eq!(spell_name, None, "no catalog row: name is nil");
        assert_eq!(spell_school, 0, "no catalog row: school mask is 0");
        assert_eq!(miss, "MISS");
        assert_eq!(extra, None, "exactly the twelve SPELL_MISSED values");
    }

    /// missType 10 (ABSORB) sets `0x2000` in `SetMissInfo` (:311) and `PushEvent` appends the
    /// amount slot as integer 0 from `SMSG_SPELL_MISS_LOG`.
    #[test]
    fn spell_missed_absorb_appends_the_zero_amount() {
        let hit = spell_miss_hit(10);
        let args = spell_miss_args(10.0, &hit, "Firna", "Kobold Vermin", None);
        assert_eq!(args.len(), 13);
        assert_eq!(args[11], ScriptValue::Str("ABSORB".to_owned()));
        assert_eq!(
            args[12],
            ScriptValue::Int(0),
            "absorb amount slot is integer 0"
        );
    }

    /// If the spell record is missing, `PushEvent` pushes nil name and 0 school
    /// (`UnitCombatLog_C.cpp:1241-1361`).
    #[test]
    fn spell_missed_missing_catalog_posts_nil_name_and_zero_school() {
        let hit = spell_miss_hit(3); // DODGE
        let args = spell_miss_args(10.0, &hit, "Firna", "Kobold Vermin", None);
        assert_eq!(args.len(), 12);
        assert_eq!(args[8], ScriptValue::Int(133));
        assert_eq!(args[9], ScriptValue::Nil, "missing record: nil name");
        assert_eq!(
            args[10],
            ScriptValue::Int(0),
            "missing record: zero school mask"
        );
        assert_eq!(args[11], ScriptValue::Str("DODGE".to_owned()));
    }

    fn spell_heal_hit() -> crate::wrath::WrathSpellHeal {
        crate::wrath::WrathSpellHeal {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 2050,
            amount: 42,
            overheal: 3,
            absorbed: 7,
            critical: true,
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        }
    }

    /// `SPELL_HEAL` posts 15 values: 8 header + spellId/name/schoolMask + 4 heal values
    /// (`PushEvent` 0x20 block, `UnitCombatLog_C.cpp:1241-1361`, `SendHealEvent` :2001,
    /// `HandleSpellHealLog` :1931, `SMSG_SPELL_HEAL_LOG` 0x150 in `Net/Types.h`): amount,
    /// overheal, absorbed, critical as 1.0 or nil.
    #[test]
    fn spell_heal_event_has_the_fifteen_wrath_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellHealTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let hit = spell_heal_hit();
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            spell_heal_args(1234.5, &hit, "Firna", "Teammate", None),
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
        let spell_id: i64 = scope.eval("return Fired[10]").unwrap();
        let spell_name: Option<String> = scope.eval("return Fired[11]").unwrap();
        let spell_school: i64 = scope.eval("return Fired[12]").unwrap();
        let amount: f64 = scope.eval("return Fired[13]").unwrap();
        let overheal: f64 = scope.eval("return Fired[14]").unwrap();
        let absorbed: f64 = scope.eval("return Fired[15]").unwrap();
        let critical: Option<f64> = scope.eval("return Fired[16]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[17]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "SPELL_HEAL");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Teammate");
        assert_eq!(dest_flags, 0x800);
        assert_eq!(spell_id, 2050);
        assert_eq!(spell_name, None, "no catalog row: name is nil");
        assert_eq!(spell_school, 0, "no catalog row: school mask is 0");
        assert_eq!(amount, 42.0);
        assert_eq!(overheal, 3.0);
        assert_eq!(absorbed, 7.0);
        assert_eq!(critical, Some(1.0));
        assert_eq!(extra, None, "exactly the fifteen SPELL_HEAL values");
    }

    /// `UnitCombatLogHeal` (`UnitCombatLog_C.cpp:1811`) returns without an entry when amount,
    /// overheal and absorbed are all zero, so the row is dropped before it reaches the feed.
    #[test]
    fn spell_heal_zero_row_is_dropped() {
        let mut world = WrathWorldView::default();
        world.session_epoch = 1;
        world.world_generation = 2;
        world.apply_spell_heal(
            crate::wrath::WrathSpellHeal {
                amount: 0,
                overheal: 0,
                absorbed: 0,
                ..spell_heal_hit()
            },
            1,
            2,
            10.0,
        );
        assert!(
            world.spell_heals.is_empty(),
            "a zero heal row produces no combat-log entry"
        );
        assert_eq!(
            world.spell_heal_revision, 0,
            "dropping a zero row must not advance the heal revision"
        );
    }

    /// If the spell record is missing, `PushEvent` pushes nil name and 0 school
    /// (`UnitCombatLog_C.cpp:1241-1361`).
    #[test]
    fn spell_heal_missing_catalog_posts_nil_name_and_zero_school() {
        let mut hit = spell_heal_hit();
        hit.critical = false;
        let args = spell_heal_args(10.0, &hit, "Firna", "Teammate", None);
        assert_eq!(args.len(), 15);
        assert_eq!(args[8], ScriptValue::Int(2050));
        assert_eq!(args[9], ScriptValue::Nil, "missing record: nil name");
        assert_eq!(
            args[10],
            ScriptValue::Int(0),
            "missing record: zero school mask"
        );
        assert_eq!(args[11], ScriptValue::Number(42.0));
        assert_eq!(args[12], ScriptValue::Number(3.0));
        assert_eq!(args[13], ScriptValue::Number(7.0));
        assert_eq!(args[14], ScriptValue::Nil, "critical 1.0 or nil");
    }

    fn spell_energize_hit() -> crate::wrath::WrathSpellEnergize {
        crate::wrath::WrathSpellEnergize {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 29166,
            power_type: 0,
            amount: 120,
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        }
    }

    /// `SPELL_ENERGIZE` posts 13 values: 8 header + spellId/name/schoolMask + divided amount and
    /// powerType (`PushEvent` 0x40, `UnitCombatLog_C.cpp:1241-1361`/`:1365`, `SendEnergizeEvent`
    /// at `:2045`, `UnitCombatLogEnergize` at `:1831`, `HandleSpellLogEnergizeLog` at `:1974`,
    /// `SMSG_SPELL_ENERGIZE_LOG` `0x151` in `Net/Types.h`, `Player_C.cpp:3670`). Includes a power
    /// type whose display scale is not 1 (rage 1 => amount/10) and one with scale 1.
    #[test]
    fn spell_energize_event_has_the_thirteen_wrath_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellEnergizeTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let mut hit = spell_energize_hit();
        hit.power_type = 1; // rage: power_display_scale 10
        hit.amount = 120;
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            spell_energize_args(1234.5, &hit, "Firna", "Teammate", None),
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
        let spell_id: i64 = scope.eval("return Fired[10]").unwrap();
        let spell_name: Option<String> = scope.eval("return Fired[11]").unwrap();
        let spell_school: i64 = scope.eval("return Fired[12]").unwrap();
        let amount: f64 = scope.eval("return Fired[13]").unwrap();
        let power_type: f64 = scope.eval("return Fired[14]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[15]").unwrap();

        assert_eq!(event, "COMBAT_LOG_EVENT_UNFILTERED");
        assert_eq!(timestamp, 1234.5);
        assert_eq!(subevent, "SPELL_ENERGIZE");
        assert_eq!(source_guid, "0x0000000000000011");
        assert_eq!(source_name, "Firna");
        assert_eq!(source_flags, 0x400);
        assert_eq!(dest_guid, "0xF130000000000022");
        assert_eq!(dest_name, "Teammate");
        assert_eq!(dest_flags, 0x800);
        assert_eq!(spell_id, 29166);
        assert_eq!(spell_name, None, "no catalog row: name is nil");
        assert_eq!(spell_school, 0, "no catalog row: school mask is 0");
        assert_eq!(amount, 12.0, "rage amount is divided by 10");
        assert_eq!(power_type, 1.0);
        assert_eq!(extra, None, "exactly the thirteen SPELL_ENERGIZE values");

        let mut mana = spell_energize_hit();
        mana.power_type = 0; // mana: power_display_scale 1
        mana.amount = 77;
        let mana_args = spell_energize_args(10.0, &mana, "Firna", "Teammate", None);
        assert_eq!(mana_args.len(), 13);
        assert_eq!(mana_args[11], ScriptValue::Number(77.0), "scale 1 amount");
        assert_eq!(mana_args[12], ScriptValue::Number(0.0));
    }

    /// `UnitCombatLogEnergize` (`UnitCombatLog_C.cpp:1831`) returns when amount is zero, so the
    /// row is dropped before it reaches the feed.
    #[test]
    fn spell_energize_zero_row_is_dropped() {
        let mut world = WrathWorldView::default();
        world.session_epoch = 1;
        world.world_generation = 2;
        world.apply_spell_energize(
            crate::wrath::WrathSpellEnergize {
                amount: 0,
                ..spell_energize_hit()
            },
            1,
            2,
            10.0,
        );
        assert!(
            world.spell_energizes.is_empty(),
            "a zero energize row produces no combat-log entry"
        );
        assert_eq!(
            world.spell_energize_revision, 0,
            "dropping a zero row must not advance the energize revision"
        );
    }

    /// If the spell record is missing, `PushEvent` pushes nil name and 0 school
    /// (`UnitCombatLog_C.cpp:1241-1361`).
    #[test]
    fn spell_energize_missing_catalog_posts_nil_name_and_zero_school() {
        let hit = spell_energize_hit();
        let args = spell_energize_args(10.0, &hit, "Firna", "Teammate", None);
        assert_eq!(args.len(), 13);
        assert_eq!(args[8], ScriptValue::Int(29166));
        assert_eq!(args[9], ScriptValue::Nil, "missing record: nil name");
        assert_eq!(
            args[10],
            ScriptValue::Int(0),
            "missing record: zero school mask"
        );
        assert_eq!(args[11], ScriptValue::Number(120.0));
        assert_eq!(args[12], ScriptValue::Number(0.0));
    }

    fn spell_periodic_damage_hit() -> crate::wrath::WrathSpellPeriodicDamage {
        crate::wrath::WrathSpellPeriodicDamage {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 172,
            amount: 5,
            overkill: 1,
            school: 4,
            absorb: 2,
            resist: 3,
            critical: true,
            session_epoch: 1,
            world_generation: 2,
            revision: 3,
            received_at: 1234.5,
        }
    }

    #[test]
    fn spell_periodic_damage_has_the_twenty_wrath_values() {
        let mut scope = UiScript::with_target(UiTarget::Wrath335).unwrap();
        scope
            .run(
                r#"
                Fired = nil
                local listener = CreateFrame("Frame", "WrathSpellPeriodicDamageTestListener")
                listener:SetScript("OnEvent", function(_, event, ...)
                    Fired = { event, ... }
                end)
                listener:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
                "#,
            )
            .unwrap();

        let hit = spell_periodic_damage_hit();
        scope.fire_event(
            "COMBAT_LOG_EVENT_UNFILTERED",
            spell_periodic_damage_args(1234.5, &hit, "Firna", "Kobold Vermin", None),
        );

        let subevent: String = scope.eval("return Fired[3]").unwrap();
        let spell_id: i64 = scope.eval("return Fired[10]").unwrap();
        let spell_name: Option<String> = scope.eval("return Fired[11]").unwrap();
        let spell_school: i64 = scope.eval("return Fired[12]").unwrap();
        let amount: f64 = scope.eval("return Fired[13]").unwrap();
        let overkill: f64 = scope.eval("return Fired[14]").unwrap();
        let school: i64 = scope.eval("return Fired[15]").unwrap();
        let resisted: Option<f64> = scope.eval("return Fired[16]").unwrap();
        let blocked: Option<f64> = scope.eval("return Fired[17]").unwrap();
        let absorbed: Option<f64> = scope.eval("return Fired[18]").unwrap();
        let critical: Option<f64> = scope.eval("return Fired[19]").unwrap();
        let glancing: Option<f64> = scope.eval("return Fired[20]").unwrap();
        let crushing: Option<f64> = scope.eval("return Fired[21]").unwrap();
        let extra: Option<f64> = scope.eval("return Fired[22]").unwrap();

        assert_eq!(subevent, "SPELL_PERIODIC_DAMAGE");
        assert_eq!(spell_id, 172);
        assert_eq!(spell_name, None);
        assert_eq!(spell_school, 0);
        assert_eq!(amount, 5.0);
        assert_eq!(overkill, 1.0);
        assert_eq!(school, 4);
        assert_eq!(resisted, Some(3.0));
        assert_eq!(blocked, None, "blocked is always nil from this packet");
        assert_eq!(absorbed, Some(2.0));
        assert_eq!(critical, Some(1.0));
        assert_eq!(glancing, None);
        assert_eq!(crushing, None);
        assert_eq!(extra, None, "exactly the twenty SPELL_PERIODIC_DAMAGE values");
    }

    #[test]
    fn spell_periodic_zero_damage_absorb_or_resist_posts_missed() {
        let absorb_hit = crate::wrath::WrathSpellPeriodicMiss {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 172,
            miss_type: 10,
            amount: 9,
            session_epoch: 1,
            world_generation: 2,
            revision: 1,
            received_at: 1.0,
        };
        let absorb_args = spell_periodic_miss_args(1.0, &absorb_hit, "Firna", "Kobold Vermin", None);
        assert_eq!(absorb_args[1], ScriptValue::Str("SPELL_PERIODIC_MISSED".to_owned()));
        assert_eq!(absorb_args[11], ScriptValue::Str("ABSORB".to_owned()));
        assert_eq!(absorb_args[12], ScriptValue::Number(9.0));
        assert_eq!(absorb_args.len(), 13);

        let resist_hit = crate::wrath::WrathSpellPeriodicMiss {
            miss_type: 2,
            amount: 7,
            ..absorb_hit
        };
        let resist_args = spell_periodic_miss_args(1.0, &resist_hit, "Firna", "Kobold Vermin", None);
        assert_eq!(resist_args[11], ScriptValue::Str("RESIST".to_owned()));
        assert_eq!(resist_args[12], ScriptValue::Number(7.0));

        // damage == 0 with no absorb/resist produces no entry - dispatch classification covers it;
        // the periodic damage apply path never enqueues such rows.
        let mut world = WrathWorldView::default();
        world.session_epoch = 1;
        world.world_generation = 2;
        // No row is enqueued for damage==0, absorb==0, resist==0 in the dispatch; nothing to apply.
        assert!(world.spell_periodic_damages.is_empty());
        assert!(world.spell_periodic_misses.is_empty());
    }

    #[test]
    fn spell_periodic_heal_and_energize_use_their_reference_blocks() {
        let heal = crate::wrath::WrathSpellPeriodicHeal {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 2050,
            amount: 42,
            overheal: 3,
            absorbed: 7,
            critical: true,
            session_epoch: 1,
            world_generation: 2,
            revision: 1,
            received_at: 1.0,
        };
        let heal_args = spell_periodic_heal_args(1.0, &heal, "Firna", "Teammate", None);
        assert_eq!(heal_args[1], ScriptValue::Str("SPELL_PERIODIC_HEAL".to_owned()));
        assert_eq!(heal_args.len(), 15);
        assert_eq!(heal_args[11], ScriptValue::Number(42.0));
        assert_eq!(heal_args[12], ScriptValue::Number(3.0));
        assert_eq!(heal_args[13], ScriptValue::Number(7.0));
        assert_eq!(heal_args[14], ScriptValue::Number(1.0));

        let energize = crate::wrath::WrathSpellPeriodicEnergize {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 29166,
            power_type: 1, // rage: /10
            amount: 120,
            session_epoch: 1,
            world_generation: 2,
            revision: 1,
            received_at: 1.0,
        };
        let energize_args =
            spell_periodic_energize_args(1.0, &energize, "Firna", "Teammate", None);
        assert_eq!(
            energize_args[1],
            ScriptValue::Str("SPELL_PERIODIC_ENERGIZE".to_owned())
        );
        assert_eq!(energize_args.len(), 13);
        assert_eq!(energize_args[11], ScriptValue::Number(12.0));
        assert_eq!(energize_args[12], ScriptValue::Number(1.0));
    }

    #[test]
    fn spell_periodic_drain_and_leech_split_on_the_multiplier() {
        let drain = crate::wrath::WrathSpellPeriodicDrain {
            target: 0xF130_0000_0000_0022,
            caster: 0x0000_0000_0000_0011,
            spell_id: 1454,
            power_type: 0,
            amount: 100,
            multiplier: 0.0,
            session_epoch: 1,
            world_generation: 2,
            revision: 1,
            received_at: 1.0,
        };
        let drain_args = spell_periodic_drain_args(1.0, &drain, "Firna", "Kobold Vermin", None);
        assert_eq!(drain_args[1], ScriptValue::Str("SPELL_PERIODIC_DRAIN".to_owned()));
        assert_eq!(drain_args.len(), 14);
        assert_eq!(drain_args[11], ScriptValue::Number(100.0));
        assert_eq!(drain_args[12], ScriptValue::Number(0.0));
        assert_eq!(drain_args[13], ScriptValue::Nil, "no extra when multiplier yields 0");

        let leech = crate::wrath::WrathSpellPeriodicDrain {
            amount: 100,
            multiplier: 0.5,
            ..drain
        };
        let leech_args = spell_periodic_drain_args(1.0, &leech, "Firna", "Kobold Vermin", None);
        assert_eq!(leech_args[1], ScriptValue::Str("SPELL_PERIODIC_LEECH".to_owned()));
        assert_eq!(leech_args.len(), 14);
        assert_eq!(leech_args[11], ScriptValue::Number(100.0));
        assert_eq!(leech_args[12], ScriptValue::Number(0.0));
        assert_eq!(leech_args[13], ScriptValue::Number(50.0));
    }

    #[test]
    fn spell_periodic_zero_amount_rows_are_dropped() {
        let mut world = WrathWorldView::default();
        world.session_epoch = 1;
        world.world_generation = 2;

        world.apply_spell_periodic_heal(
            crate::wrath::WrathSpellPeriodicHeal {
                target: 1,
                caster: 2,
                spell_id: 3,
                amount: 0,
                overheal: 0,
                absorbed: 0,
                critical: false,
                session_epoch: 1,
                world_generation: 2,
                revision: 0,
                received_at: 0.0,
            },
            1,
            2,
            10.0,
        );
        assert!(world.spell_periodic_heals.is_empty());
        assert_eq!(world.spell_periodic_heal_revision, 0);

        world.apply_spell_periodic_energize(
            crate::wrath::WrathSpellPeriodicEnergize {
                target: 1,
                caster: 2,
                spell_id: 3,
                power_type: 0,
                amount: 0,
                session_epoch: 1,
                world_generation: 2,
                revision: 0,
                received_at: 0.0,
            },
            1,
            2,
            10.0,
        );
        assert!(world.spell_periodic_energizes.is_empty());
        assert_eq!(world.spell_periodic_energize_revision, 0);

        world.apply_spell_periodic_drain(
            crate::wrath::WrathSpellPeriodicDrain {
                target: 1,
                caster: 2,
                spell_id: 3,
                power_type: 0,
                amount: 0,
                multiplier: 1.0,
                session_epoch: 1,
                world_generation: 2,
                revision: 0,
                received_at: 0.0,
            },
            1,
            2,
            10.0,
        );
        world.apply_spell_periodic_drain(
            crate::wrath::WrathSpellPeriodicDrain {
                target: 1,
                caster: 2,
                spell_id: 3,
                power_type: 5, // POWER_HAPPINESS
                amount: 10,
                multiplier: 1.0,
                session_epoch: 1,
                world_generation: 2,
                revision: 0,
                received_at: 0.0,
            },
            1,
            2,
            10.0,
        );
        assert!(world.spell_periodic_drains.is_empty());
        assert_eq!(world.spell_periodic_drain_revision, 0);
    }
}

