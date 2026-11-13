//! The automated profile compilers: plain data in, validated [`ActionSpec`]s out.
//!
//! The app extracts the summaries from what it already loads — M2 sequences with their event
//! keyframes, the equipped weapon, `Spell.dbc` through its cast-time and duration tables — and
//! these pure functions turn them into the Souls action schema. Nothing here reads a file,
//! renders, or mutates live state; the authority contract of the combat seam (`combat_mode.rs`
//! in the app) is unchanged: no compiler output is a command.
//!
//! All timing is integer ticks at the caller's [`TickRate`], quantised **up** so a window never
//! starts earlier than the authored data. Every compiler validates its spec before returning.

use crate::action::{ActionId, ActionKind, ActionSpec};
use crate::error::{CombatError, CombatResult};
use crate::policy::{permille_of, ProfilePolicy};
use crate::time::TickRate;

/// One clip's timing facts, as extracted from a loaded model. `anim_id` is the
/// `AnimationData.dbc` id the rig plays; `contact_ms` is the first attack-hit key
/// (`$AH0..$AH3`/`$CAH`) the clip authors, the same tag set the impact router fires on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClipSummary {
    pub anim_id: u16,
    pub duration_ms: u32,
    pub contact_ms: Option<u32>,
}

/// One spell's timing facts, as extracted from `Spell.dbc` and its cast-time and duration
/// tables. Power cost, cooldown and GCD stay with their existing owners; only the action clock
/// is profiled here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpellSummary {
    pub spell_id: u32,
    /// The level-solved cast time; 0 is instant.
    pub cast_ms: u32,
    /// The channel duration when the spell is channeled (`channel_interrupt_flags != 0`).
    pub channel_ms: Option<u32>,
    /// `InterruptFlags`: nonzero means the windup is interruptible.
    pub interrupt_flags: u32,
}

/// Milliseconds quantised up to whole ticks; the only ms→tick conversion the compilers use.
fn ticks(rate: TickRate, ms: u32) -> CombatResult<u64> {
    rate.ticks_for_millis_ceil(ms as u64)
        .ok_or(CombatError::TickArithmeticOverflow)
}

/// Compile a player melee action from one swing clip.
///
/// The contact window opens at the clip's first attack-hit key when it authors one, and at the
/// policy's windup fraction when it does not; its length is the policy's contact window. Input
/// is heard from the end of contact, and the swing commits at contact: the windup is
/// interruptible, the active arc is not. The caller decides light versus heavy from the wielded
/// weapon; the stamina cost follows the kind.
pub fn melee_action(
    id: ActionId,
    kind: ActionKind,
    clip: ClipSummary,
    policy: &ProfilePolicy,
    rate: TickRate,
) -> CombatResult<ActionSpec> {
    match kind {
        ActionKind::MeleeLight | ActionKind::MeleeHeavy => {}
        _ => {
            return Err(CombatError::InvalidConfig(
                "melee_action needs a melee kind",
            ))
        }
    }
    policy.validate()?;
    if clip.duration_ms == 0 {
        return Err(CombatError::InvalidConfig("clip duration must be non-zero"));
    }
    let total = ticks(rate, clip.duration_ms)?;
    let (from, to) = match clip.contact_ms {
        Some(contact_ms) => {
            let from = ticks(rate, contact_ms)?.min(total);
            let to = (from + ticks(rate, policy.melee_contact_window_ms)?).min(total);
            (from, to)
        }
        None => {
            let from = permille_of(policy.melee_windup_permille, total);
            let to = (from + permille_of(policy.melee_contact_permille, total)).min(total);
            (from, to)
        }
    };
    let (from, to) = open_window(from, to, total)?;
    let stamina_cost = match kind {
        ActionKind::MeleeHeavy => policy.melee_heavy_stamina,
        _ => policy.melee_light_stamina,
    };
    let spec = ActionSpec {
        id,
        kind,
        total_ticks: total,
        input_from_ticks: to,
        cancel_from_ticks: Some(to),
        iframes: None,
        contact: Some(crate::time::TickWindow { from, to }),
        stamina_cost,
        // The windup is interruptible; the committed arc is hyperarmour.
        interruptible_until: Some(from),
        followup: None,
    };
    spec.validate()?;
    Ok(spec)
}

/// Compile a telegraphed enemy strike from the enemy's own attack clip.
///
/// The telegraph is the windup to the clip's contact key (or the policy's fraction without one).
/// Enemy strikes carry no player-buffer semantics — no cancel window, no follow-up — and the
/// enemy's poise/stagger policy is deliberately absent here: the enemy stays hyperarmoured until
/// the stagger slice names its rules. Cadence (`UNIT_FIELD_BASEATTACKTIME`) is not part of the
/// profile; the enemy FSM consumes it when that slice lands.
pub fn enemy_action(
    id: ActionId,
    clip: ClipSummary,
    policy: &ProfilePolicy,
    rate: TickRate,
) -> CombatResult<ActionSpec> {
    policy.validate()?;
    if clip.duration_ms == 0 {
        return Err(CombatError::InvalidConfig("clip duration must be non-zero"));
    }
    let total = ticks(rate, clip.duration_ms)?;
    let (from, to) = match clip.contact_ms {
        Some(contact_ms) => {
            let from = ticks(rate, contact_ms)?.min(total);
            let to = (from + ticks(rate, policy.enemy_contact_window_ms)?).min(total);
            (from, to)
        }
        None => {
            let from = permille_of(policy.enemy_windup_permille, total);
            let to = (from + permille_of(policy.enemy_contact_permille, total)).min(total);
            (from, to)
        }
    };
    let (from, to) = open_window(from, to, total)?;
    let spec = ActionSpec {
        id,
        kind: ActionKind::EnemyStrike,
        total_ticks: total,
        input_from_ticks: total,
        cancel_from_ticks: None,
        iframes: None,
        contact: Some(crate::time::TickWindow { from, to }),
        // Enemy stamina is not modelled; the swing costs the enemy nothing here.
        stamina_cost: 0,
        // Hyperarmour until the stagger slice: nothing interrupts an enemy strike yet.
        interruptible_until: None,
        followup: None,
    };
    spec.validate()?;
    Ok(spec)
}

/// Compile a spell action from its timing facts.
///
/// - **Channel** (`channel_ms` is `Some`): windup is the cast time, the contact window is the
///   release at the cast's end, and the sustain runs to the channel's end. A channel with
///   interrupt flags is interruptible throughout; without them it is hyperarmoured.
/// - **Timed cast** (`cast_ms > 0`): windup is the cast time, the contact window is the release,
///   and the recovery is the release window alone. The stock GCD and cooldown stay with their
///   existing owners; the action clock ends at the release.
/// - **Instant** (`cast_ms == 0`): no windup; the total is the policy's instant minimum and the
///   contact window is the release at the start.
pub fn spell_action(
    id: ActionId,
    spell: SpellSummary,
    policy: &ProfilePolicy,
    rate: TickRate,
) -> CombatResult<ActionSpec> {
    policy.validate()?;
    let release = ticks(rate, policy.spell_release_window_ms)?;
    let spec = if let Some(channel_ms) = spell.channel_ms {
        let cast = ticks(rate, spell.cast_ms)?;
        let channel = ticks(rate, channel_ms)?;
        let total = cast.checked_add(channel).ok_or(CombatError::TickArithmeticOverflow)?;
        let to = (cast + release).min(total);
        let (from, to) = open_window(cast, to, total)?;
        ActionSpec {
            id,
            kind: ActionKind::SpellChannel,
            total_ticks: total,
            input_from_ticks: to,
            cancel_from_ticks: None,
            iframes: None,
            contact: Some(crate::time::TickWindow { from, to }),
            stamina_cost: 0,
            // A channeled spell with interrupt flags breaks on movement and CC through the
            // sustain; without them it commits.
            interruptible_until: (spell.interrupt_flags != 0).then_some(total),
            followup: None,
        }
    } else if spell.cast_ms > 0 {
        let cast = ticks(rate, spell.cast_ms)?;
        let total = cast + release;
        let (from, to) = open_window(cast, total, total)?;
        ActionSpec {
            id,
            kind: ActionKind::SpellCast,
            total_ticks: total,
            input_from_ticks: total,
            cancel_from_ticks: None,
            iframes: None,
            contact: Some(crate::time::TickWindow { from, to }),
            stamina_cost: 0,
            // The windup is interruptible when the spell's flags say so; the release commits.
            interruptible_until: (spell.interrupt_flags != 0).then_some(cast),
            followup: None,
        }
    } else {
        let minimum = ticks(rate, policy.spell_instant_minimum_ms)?;
        let total = minimum.max(release);
        let to = release.min(total);
        let (from, to) = open_window(0, to, total)?;
        ActionSpec {
            id,
            kind: ActionKind::SpellInstant,
            total_ticks: total,
            input_from_ticks: to,
            cancel_from_ticks: None,
            iframes: None,
            contact: Some(crate::time::TickWindow { from, to }),
            stamina_cost: 0,
            // No windup, nothing to interrupt.
            interruptible_until: None,
            followup: None,
        }
    };
    spec.validate()?;
    Ok(spec)
}

/// Turn a computed `(from, to)` pair into a half-open window inside `total`, refusing the empty
/// or inverted pairs a zero-length clip or a mis-tuned policy could produce.
fn open_window(from: u64, to: u64, total: u64) -> CombatResult<(u64, u64)> {
    if from < to && to <= total {
        Ok((from, to))
    } else {
        Err(CombatError::InvalidConfig(
            "the compiled contact window is empty or leaves the action",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TickWindow;

    fn rate() -> TickRate {
        TickRate::new(100).expect("100 Hz")
    }

    fn clip(anim_id: u16, duration_ms: u32, contact_ms: Option<u32>) -> ClipSummary {
        ClipSummary {
            anim_id,
            duration_ms,
            contact_ms,
        }
    }

    /// A clip with a contact key: the contact window opens at the key (quantised up) and runs
    /// the policy's window; input is heard from the window's end; the swing commits at contact.
    #[test]
    fn melee_contact_opens_at_the_clip_key() {
        let spec = melee_action(
            ActionId(1),
            ActionKind::MeleeLight,
            clip(17, 1_000, Some(400)),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.total_ticks, 100);
        assert_eq!(spec.contact, Some(TickWindow { from: 40, to: 60 }));
        assert_eq!(spec.input_from_ticks, 60);
        assert_eq!(spec.cancel_from_ticks, Some(60));
        assert_eq!(spec.stamina_cost, 20);
        assert_eq!(spec.interruptible_until, Some(40));
        assert_eq!(spec.recovery_ticks(), 40);
    }

    /// A clip with no key falls back to the policy fractions: 400‰ windup, 150‰ contact.
    #[test]
    fn melee_without_a_key_uses_the_policy_fractions() {
        let spec = melee_action(
            ActionId(1),
            ActionKind::MeleeLight,
            clip(16, 1_000, None),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.contact, Some(TickWindow { from: 40, to: 55 }));
    }

    /// A heavy swing costs the heavy budget and keeps the same shape.
    #[test]
    fn heavy_swings_cost_more() {
        let spec = melee_action(
            ActionId(2),
            ActionKind::MeleeHeavy,
            clip(18, 1_000, Some(400)),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.stamina_cost, 35);
        assert!(matches!(spec.kind, ActionKind::MeleeHeavy));
    }

    /// A contact key near the clip's end keeps the window inside the action, and a non-melee kind
    /// is refused.
    #[test]
    fn melee_windows_stay_inside_and_kinds_are_checked() {
        let spec = melee_action(
            ActionId(1),
            ActionKind::MeleeLight,
            clip(17, 1_000, Some(990)),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.contact, Some(TickWindow { from: 99, to: 100 }));
        assert_eq!(spec.recovery_ticks(), 0);

        assert!(melee_action(
            ActionId(1),
            ActionKind::Dodge,
            clip(17, 1_000, Some(400)),
            &ProfilePolicy::default(),
            rate(),
        )
        .is_err());
    }

    /// A timed cast: windup is the cast time, the release window follows, and the windup is
    /// interruptible only when the spell's flags say so.
    #[test]
    fn timed_casts_wind_up_for_the_cast_time() {
        let interruptible = spell_action(
            ActionId(1_001_337),
            SpellSummary {
                spell_id: 1337,
                cast_ms: 3_000,
                channel_ms: None,
                interrupt_flags: 0xf,
            },
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(interruptible.kind, ActionKind::SpellCast);
        assert_eq!(interruptible.total_ticks, 320);
        assert_eq!(interruptible.contact, Some(TickWindow { from: 300, to: 320 }));
        assert_eq!(interruptible.interruptible_until, Some(300));
        assert_eq!(interruptible.stamina_cost, 0);

        let protected = spell_action(
            ActionId(1_001_338),
            SpellSummary {
                spell_id: 1338,
                cast_ms: 3_000,
                channel_ms: None,
                interrupt_flags: 0,
            },
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(protected.interruptible_until, None);
    }

    /// A channel: windup, release at the cast's end, sustain to the channel's end, and
    /// interruptibility through the sustain when the flags allow it.
    #[test]
    fn channels_sustain_after_the_windup() {
        let spec = spell_action(
            ActionId(1_000_120),
            SpellSummary {
                spell_id: 120,
                cast_ms: 500,
                channel_ms: Some(5_000),
                interrupt_flags: 0x18,
            },
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.kind, ActionKind::SpellChannel);
        assert_eq!(spec.total_ticks, 550);
        assert_eq!(spec.contact, Some(TickWindow { from: 50, to: 70 }));
        assert_eq!(spec.interruptible_until, Some(550));

        let armored = spell_action(
            ActionId(1_000_121),
            SpellSummary {
                spell_id: 121,
                cast_ms: 500,
                channel_ms: Some(5_000),
                interrupt_flags: 0,
            },
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(armored.interruptible_until, None);
    }

    /// An instant: no windup, the policy's minimum total, the release window at the start.
    #[test]
    fn instants_release_at_the_start() {
        let spec = spell_action(
            ActionId(1_000_566),
            SpellSummary {
                spell_id: 566,
                cast_ms: 0,
                channel_ms: None,
                interrupt_flags: 0,
            },
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.kind, ActionKind::SpellInstant);
        assert_eq!(spec.total_ticks, 40, "the 400 ms instant minimum at 100 Hz");
        assert_eq!(spec.contact, Some(TickWindow { from: 0, to: 20 }));
        assert_eq!(spec.interruptible_until, None);
        assert_eq!(spec.input_from_ticks, 20);
    }

    /// An enemy clip: the telegraph runs to the contact key, the contact window follows, and the
    /// strike is hyperarmoured with no cancel or follow-up.
    #[test]
    fn enemy_telegraphs_read_their_contact_key() {
        let spec = enemy_action(
            ActionId(1_000),
            clip(17, 2_000, Some(1_200)),
            &ProfilePolicy::default(),
            rate(),
        )
        .expect("compile");
        assert_eq!(spec.kind, ActionKind::EnemyStrike);
        assert_eq!(spec.total_ticks, 200);
        assert_eq!(spec.windup_ticks(), 120, "the telegraph is the windup");
        assert_eq!(spec.contact, Some(TickWindow { from: 120, to: 145 }));
        assert_eq!(spec.recovery_ticks(), 55);
        assert_eq!(spec.interruptible_until, None);
        assert_eq!(spec.cancel_from_ticks, None);
        assert_eq!(spec.input_from_ticks, 200);
    }

    /// Quantisation rounds up: 1_150 ms is 12 whole ticks at 10 Hz, so no authored window starts
    /// earlier than its data.
    #[test]
    fn millisecond_windows_quantise_up() {
        let spec = melee_action(
            ActionId(1),
            ActionKind::MeleeLight,
            clip(17, 1_150, Some(0)),
            &ProfilePolicy::default(),
            TickRate::new(10).expect("10 Hz"),
        )
        .expect("compile");
        assert_eq!(spec.total_ticks, 12);
        assert_eq!(spec.contact, Some(TickWindow { from: 0, to: 2 }));
    }
}
