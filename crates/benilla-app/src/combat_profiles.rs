//! The client-side half of the automated action-profile compiler.
//!
//! [`benilla_combat_core`]'s pure compilers turn plain summaries into validated action specs;
//! this module extracts those summaries from the data the client already loads:
//!
//! - [`clip_summary`] reads a rig clip's length and its first attack-hit key (`$AH0..$AH3`,
//!   `$CAH`) — the same tag set the impact router fires victim feedback on;
//! - [`player_melee_action`] joins the main-hand weapon with the swing-clip LUT the driver
//!   already uses ([`crate::creature_anim::swing_anim_main`]) and the rig's resolved clip;
//! - [`spell_summary`] reads `Spell.dbc` through the cast-time and duration tables the app loads
//!   at startup.
//!
//! Everything here is offline and pure: no system, no traffic, no resource mutation. The
//! consumers are the ones named in [the plan](../../docs/architecture/souls-combat-action-profiles.md):
//! action-instance playback (slice 2) and combat-host staging (slice 3), neither of which is
//! wired yet. [`CombatProfiles`] is the typed hand-off target for those slices; until then the
//! app keeps no instance of it.

use std::collections::HashMap;

use benilla_assets::{AnimClip, ModelAnimations};
use benilla_combat_core::{
    dodge_from_clip, melee_action, spell_action, ActionId, ActionKind, ActionSpec, ActionTable,
    ClipSummary, CombatResult, DodgeSpec, ProfilePolicy, SpellSummary, TickRate,
};
use benilla_formats::{
    AnimDataCatalog, SpellCastTimeCatalog, SpellDisplay, SpellDurationCatalog,
};

/// The dodge action every encounter shares; the host range `1..=999`.
pub(crate) const HOST_DODGE: ActionId = ActionId(1);
/// The `AnimationData.dbc` id of the Dodge clip (`Dodge`, the defense reaction anim 30).
const DODGE_ANIM: u16 = 30;
/// Enemy actions are encounter-assigned in `ENEMY_ACTION_BASE..=999_999` (slice 4).
pub(crate) const ENEMY_ACTION_BASE: u32 = 1_000;
/// Spell actions are offset by the `Spell.dbc` id from `SPELL_ACTION_BASE` upward.
pub(crate) const SPELL_ACTION_BASE: u32 = 1_000_000;

/// The stable action id a learned spell compiles to.
pub(crate) fn spell_action_id(spell_id: u32) -> ActionId {
    ActionId(SPELL_ACTION_BASE.wrapping_add(spell_id))
}

/// Extract one clip's timing facts. The contact key is the first attack-hit event the clip
/// authors; `None` when the clip carries no duration or no usable key, leaving the compiler's
/// policy fractions to place the window.
pub(crate) fn clip_summary(clip: &AnimClip) -> Option<ClipSummary> {
    if clip.duration <= 0.0 {
        return None;
    }
    let contact_ms = clip.events.iter().find_map(|event| {
        crate::creature_anim::impact_tag(&event.ident).map(|_| {
            // Event times are seconds on the clip clock; round to whole ms and let the
            // compiler's tick quantisation round up from there.
            (event.time * 1000.0).round().max(0.0) as u32
        })
    });
    Some(ClipSummary {
        anim_id: clip.anim_id,
        duration_ms: (clip.duration * 1000.0).round() as u32,
        contact_ms,
    })
}

/// The melee class a main-hand weapon compiles to: the 2H families (2H axe/mace/sword/exotic,
/// polearm/staff/spear) are heavy, everything else — 1H, dagger, fist, empty hands — is light.
/// The grouping mirrors the swing LUT's Attack2H/Attack2HL rows.
pub(crate) fn melee_kind(main: Option<(u8, u8)>) -> ActionKind {
    match main {
        Some((class, subclass)) if class == 2 => {
            if matches!(subclass, 0x1 | 0x5 | 0x8 | 0xc | 0x6 | 0xa | 0x11) {
                ActionKind::MeleeHeavy
            } else {
                ActionKind::MeleeLight
            }
        }
        _ => ActionKind::MeleeLight,
    }
}

/// Compile the player's main-hand melee action from the rig the driver plays.
///
/// The swing clip id comes from the driver's own weapon LUT, resolved through the model's
/// playable lookup and the `AnimationData` fallback chain; a rig that falls all the way back to
/// a non-swing clip carries no melee action (`None`), and the caller leaves the slot empty
/// rather than profiling a Stand as a swing. The first variation of the clip is profiled; the
/// driver rolls among weighted variations, a documented gap in
/// [the plan](../../docs/architecture/souls-combat-action-profiles.md).
pub(crate) fn player_melee_action(
    id: ActionId,
    main: Option<(u8, u8)>,
    anims: &ModelAnimations,
    anim_data: &AnimDataCatalog,
    policy: &ProfilePolicy,
    rate: TickRate,
) -> CombatResult<Option<(ActionSpec, u16)>> {
    let requested = crate::creature_anim::swing_anim_main(main);
    let resolved = anims.resolve(requested, anim_data);
    if !crate::creature_anim::is_swing_id(resolved.id) {
        return Ok(None);
    }
    let Some(clip) = anims.clips.iter().find(|clip| clip.anim_id == resolved.id) else {
        return Ok(None);
    };
    let Some(summary) = clip_summary(clip) else {
        return Ok(None);
    };
    let spec = melee_action(id, melee_kind(main), summary, policy, rate)?;
    Ok(Some((spec, resolved.id)))
}

/// Extract one spell's timing facts from `Spell.dbc` through the tables the app loads.
///
/// The cast time is level-solved exactly as the reference resolves it. A spell is channeled
/// when its `ChannelInterruptFlags` are set; its sustain is the `SpellDuration` row's base
/// value, so an unbounded (permanent) channel profiles as its cast alone — the sustain has no
/// authored end for the action clock to run to. The base-only channel duration names the
/// per-level scaling of `SpellDuration` as a refinement, not a silent decision.
pub(crate) fn spell_summary(
    display: &SpellDisplay,
    cast_times: &SpellCastTimeCatalog,
    durations: &SpellDurationCatalog,
    caster_level: u32,
) -> Option<SpellSummary> {
    let cast_ms = cast_times
        .get(display.casting_time_index)?
        .resolved_ms(caster_level, display.base_level)
        .max(0) as u32;
    let channel_ms = (display.channel_interrupt_flags != 0)
        .then(|| {
            durations
                .get(display.duration_index)
                .filter(|duration| !duration.is_permanent() && duration.base_ms > 0)
                .map(|duration| duration.base_ms as u32)
        })
        .flatten();
    Some(SpellSummary {
        spell_id: display.id,
        cast_ms,
        channel_ms,
        interrupt_flags: display.interrupt_flags,
    })
}

/// Compile the player's dodge from the rig's Dodge clip (`AnimationData` id 30, the defense
/// reaction the victim's feedback already plays). A rig that ships no Dodge clip carries no
/// dodge action (`None`): no substitution clip is invented for the first authored dodge, a
/// named gap in [the plan](../../docs/architecture/souls-combat-action-profiles.md).
pub(crate) fn player_dodge_spec(
    anims: &ModelAnimations,
    anim_data: &AnimDataCatalog,
    policy: &ProfilePolicy,
    rate: TickRate,
    revision: benilla_combat_core::ActionRevision,
) -> CombatResult<Option<(DodgeSpec, u16)>> {
    let resolved = anims.resolve(DODGE_ANIM, anim_data);
    if resolved.id != DODGE_ANIM {
        return Ok(None);
    }
    let Some(clip) = anims.clips.iter().find(|clip| clip.anim_id == resolved.id) else {
        return Ok(None);
    };
    let Some(summary) = clip_summary(clip) else {
        return Ok(None);
    };
    let spec = dodge_from_clip(summary, policy, rate, revision)?;
    Ok(Some((spec, resolved.id)))
}

/// The compiled profile set the combat host consumes once slices 2–3 land: the validated action
/// table plus the binding from action id to the clip id the local rig plays for it. No system
/// fills or reads this yet; both consumers are named in
/// [the plan](../../docs/architecture/souls-combat-action-profiles.md).
#[derive(Debug, Default)]
pub(crate) struct CombatProfiles {
    /// The compiled action table; empty until a world build fills it.
    pub(crate) table: Option<ActionTable>,
    /// Action id → the `AnimationData.dbc` id the local rig plays for that action.
    pub(crate) anims: HashMap<ActionId, u16>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::animation::graph::AnimationNodeIndex;
    use bevy::prelude::{Handle, Vec3};

    fn clip(anim_id: u16, duration_s: f32, events: &[(&[u8; 4], f32)]) -> AnimClip {
        AnimClip {
            anim_id,
            seq_index: 0,
            node: AnimationNodeIndex::new(0),
            blend_node: None,
            looping: false,
            duration: duration_s,
            move_speed: 0.0,
            blend_time: 0.0,
            bounds_center: Vec3::ZERO,
            bounds_radius: 0.0,
            bounds_min: Vec3::ZERO,
            bounds_max: Vec3::ZERO,
            events: events
                .iter()
                .map(|&(ident, time)| benilla_assets::ClipEvent {
                    time,
                    ident: *ident,
                    data: 0,
                    bone: 0,
                    offset: Vec3::ZERO,
                    point: Vec3::ZERO,
                })
                .collect::<Vec<_>>()
                .into(),
            arm_nodes: None,
            upper_node: None,
            frequency: 0,
            replay: (0, 0),
            poses_bones: true,
        }
    }

    fn rig(clips: Vec<AnimClip>) -> ModelAnimations {
        ModelAnimations {
            graph: Handle::default(),
            clips,
            hand_close: [None, None],
            playable_animation_lookup: Vec::new(),
            animation_lookup: Vec::new(),
            global_bones: Vec::new(),
            first_seq: None,
            pose: Default::default(),
        }
    }

    fn rate() -> TickRate {
        TickRate::new(100).expect("100 Hz")
    }

    /// The summary reads the clip's length and its first attack-hit key; other keys — sound,
    /// footfall, the inert `$HIT` — are not contact.
    #[test]
    fn clip_summary_reads_duration_and_the_first_contact_key() {
        let swing = clip(
            17,
            1.0,
            &[(b"$SND", 0.1), (b"$AH1", 0.4), (b"$AH2", 0.6), (b"$HIT", 0.5)],
        );
        assert_eq!(
            clip_summary(&swing),
            Some(ClipSummary {
                anim_id: 17,
                duration_ms: 1_000,
                contact_ms: Some(400),
            })
        );

        let keyless = clip(16, 1.0, &[]);
        assert_eq!(
            clip_summary(&keyless).map(|summary| summary.contact_ms),
            Some(None)
        );
    }

    /// The 2H families compile heavy; the 1H family, dagger, fist and empty hands compile light.
    #[test]
    fn melee_kind_splits_the_two_handed_family() {
        assert_eq!(melee_kind(Some((2, 0x7))), ActionKind::MeleeLight, "1H sword");
        assert_eq!(melee_kind(Some((2, 0xf))), ActionKind::MeleeLight, "dagger");
        assert_eq!(melee_kind(Some((2, 0x1))), ActionKind::MeleeHeavy, "2H axe");
        assert_eq!(melee_kind(Some((2, 0xa))), ActionKind::MeleeHeavy, "staff");
        assert_eq!(melee_kind(None), ActionKind::MeleeLight, "empty hands");
    }

    /// A sword main hand profiles the rig's Attack1H: the contact window opens at the clip's key
    /// and runs the policy's window, at the clip's own length.
    #[test]
    fn player_melee_action_profiles_the_rigs_swing() {
        let rig = rig(vec![
            clip(0, 1.0, &[]),
            clip(17, 1.0, &[(b"$AH0", 0.4)]),
            clip(17, 1.0, &[(b"$AH0", 0.5)]), // a second variation; the first is profiled
        ]);
        let (spec, anim_id) = player_melee_action(
            ActionId(2),
            Some((2, 0x7)),
            &rig,
            &AnimDataCatalog::from_rows([]),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile")
        .expect("the rig swings");
        assert_eq!(anim_id, 17);
        assert_eq!(spec.kind, ActionKind::MeleeLight);
        assert_eq!(spec.total_ticks, 100);
        assert_eq!(spec.contact.map(|window| (window.from, window.to)), Some((40, 60)));
        assert_eq!(spec.stamina_cost, 20);
    }

    /// A rig with no attack clip at all — the fallback chain lands on a non-swing clip — carries
    /// no melee action, rather than profiling its Stand as a swing.
    #[test]
    fn player_melee_action_refuses_a_rig_without_a_swing() {
        let rig = rig(vec![clip(0, 1.0, &[])]);
        assert_eq!(
            player_melee_action(
                ActionId(2),
                Some((2, 0x7)),
                &rig,
                &AnimDataCatalog::from_rows([]),
                &ProfilePolicy::default(),
                rate(),
            )
            .expect("compile"),
            None
        );
    }

    /// A timed, interruptible cast with a channel duration profiles as a channel; the same spell
    /// without channel flags profiles as its cast.
    #[test]
    fn spell_summary_reads_cast_channel_and_flags() {
        let cast_times = SpellCastTimeCatalog::from_rows([(
            16,
            benilla_formats::SpellCastTime {
                base_ms: 3_000,
                per_level_ms: 0,
                minimum_ms: 3_000,
            },
        )]);
        let durations = SpellDurationCatalog::from_rows([(
            1,
            benilla_formats::SpellDuration {
                base_ms: 10_000,
                per_level_ms: 0,
                max_ms: 10_000,
            },
        )]);
        let mut display = SpellDisplay::default();
        display.id = 1_337;
        display.casting_time_index = 16;
        display.interrupt_flags = 0xf;
        display.channel_interrupt_flags = 0x18;
        display.duration_index = 1;

        let summary = spell_summary(&display, &cast_times, &durations, 80)
            .expect("the cast-time row resolves");
        assert_eq!(
            summary,
            SpellSummary {
                spell_id: 1_337,
                cast_ms: 3_000,
                channel_ms: Some(10_000),
                interrupt_flags: 0xf,
            }
        );

        let spec = spell_action(
            spell_action_id(1_337),
            summary,
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.kind, ActionKind::SpellChannel);

        display.channel_interrupt_flags = 0;
        let summary = spell_summary(&display, &cast_times, &durations, 80)
            .expect("the cast-time row resolves");
        assert_eq!(summary.channel_ms, None);
        let spec = spell_action(
            spell_action_id(1_337),
            summary,
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.kind, ActionKind::SpellCast);

        // A spell whose cast-time row is missing from the table is not profiled, not invented.
        display.casting_time_index = 99;
        assert_eq!(
            spell_summary(&display, &cast_times, &durations, 80),
            None,
            "a missing cast-time row refuses the profile"
        );
    }

    /// The dodge compiles from the rig's own Dodge clip; a rig that ships none carries no dodge
    /// action, and no substitute clip is invented.
    #[test]
    fn player_dodge_spec_profiles_the_rigs_dodge_clip() {
        let dodging = rig(vec![clip(0, 1.0, &[]), clip(30, 0.5, &[])]);
        let (spec, anim_id) = player_dodge_spec(
            &dodging,
            &AnimDataCatalog::from_rows([]),
            &ProfilePolicy::default(),
            rate(),
            benilla_combat_core::ActionRevision(1),
        )
        .expect("compile")
        .expect("the rig dodges");
        assert_eq!(anim_id, 30);
        assert_eq!(spec.total_ticks, 50);
        assert_eq!(spec.iframes.from, 10);
        assert_eq!(spec.iframes.to, 40);
        assert_eq!(spec.stamina_cost, 25);

        let bare = rig(vec![clip(0, 1.0, &[])]);
        assert_eq!(
            player_dodge_spec(
                &bare,
                &AnimDataCatalog::from_rows([]),
                &ProfilePolicy::default(),
                rate(),
                benilla_combat_core::ActionRevision(1),
            )
            .expect("compile"),
            None,
            "a rig without a Dodge clip carries no dodge action"
        );
    }
}
