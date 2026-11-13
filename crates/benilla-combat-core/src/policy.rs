//! Authored tuning for the automated profile compilers.
//!
//! No value here is an extracted Elden Ring number; every field is a labelled Project B policy,
//! in the same position as [`StaminaConfig::regen_delay_ticks`](crate::data::StaminaConfig).
//! Fractions are per-mille integers so no float enters the tick math, and each can be overridden
//! per revision once WM publishes revisions under the RFC's content contract.

use crate::error::{CombatError, CombatResult};

/// The one tuning set the compilers read. Every window it places is a policy decision, named
/// here so tuning changes have exactly one place to diff.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProfilePolicy {
    /// A melee clip with no contact key: windup length as per-mille of the clip.
    pub melee_windup_permille: u32,
    /// A melee clip with no contact key: contact-window length as per-mille of the clip.
    pub melee_contact_permille: u32,
    /// A melee clip with a contact key: contact-window length after the key, in ms.
    pub melee_contact_window_ms: u32,
    /// Stamina a light swing spends on acceptance.
    pub melee_light_stamina: u32,
    /// Stamina a heavy (2H family) swing spends on acceptance.
    pub melee_heavy_stamina: u32,
    /// The spell release window in ms: the contact window at a cast's end.
    pub spell_release_window_ms: u32,
    /// An instant's total length in ms, when the release window alone would be shorter.
    pub spell_instant_minimum_ms: u32,
    /// An enemy clip with no contact key: telegraph length as per-mille of the clip.
    pub enemy_windup_permille: u32,
    /// An enemy clip with no contact key: contact-window length as per-mille of the clip.
    pub enemy_contact_permille: u32,
    /// An enemy clip with a contact key: contact-window length after the key, in ms.
    pub enemy_contact_window_ms: u32,
}

impl Default for ProfilePolicy {
    /// The first authored tuning set. The per-mille windows approximate a committed swing
    /// (windup just under half the clip, contact a short arc); the costs echo the fixture's
    /// 25-stamina dodge on a lighter scale. All subject to encounter tuning; none is an
    /// extracted value.
    fn default() -> Self {
        Self {
            melee_windup_permille: 400,
            melee_contact_permille: 150,
            melee_contact_window_ms: 200,
            melee_light_stamina: 20,
            melee_heavy_stamina: 35,
            spell_release_window_ms: 200,
            spell_instant_minimum_ms: 400,
            enemy_windup_permille: 500,
            enemy_contact_permille: 150,
            enemy_contact_window_ms: 250,
        }
    }
}

impl ProfilePolicy {
    /// Policy bounds: per-mille fractions stay within `1..=1000`, and every ms window is
    /// non-zero, so a compiled window can never be empty or extend past every clip.
    pub fn validate(&self) -> CombatResult<()> {
        let permille = [
            self.melee_windup_permille,
            self.melee_contact_permille,
            self.enemy_windup_permille,
            self.enemy_contact_permille,
        ];
        if permille.iter().any(|&value| value == 0 || value > 1000) {
            return Err(CombatError::InvalidConfig(
                "policy permilles must be within 1..=1000",
            ));
        }
        let windows = [
            self.melee_contact_window_ms,
            self.spell_release_window_ms,
            self.spell_instant_minimum_ms,
            self.enemy_contact_window_ms,
        ];
        if windows.iter().any(|&value| value == 0) {
            return Err(CombatError::InvalidConfig("policy ms windows must be non-zero"));
        }
        if self.melee_light_stamina == 0 || self.melee_heavy_stamina == 0 {
            return Err(CombatError::InvalidConfig("policy stamina costs must be non-zero"));
        }
        Ok(())
    }
}

/// `permille` of `total` ticks, rounded up, so a policy-placed window never starts earlier than
/// the authored fraction.
pub(crate) fn permille_of(permille: u32, total: u64) -> u64 {
    (total * permille as u64 + 999) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default set validates, and the per-mille helper rounds up: 400‰ of 100 ticks is 40
    /// exactly, 333‰ of 10 is 4 (3.33 → 4), and 1000‰ is the whole.
    #[test]
    fn default_policy_validates_and_permille_rounds_up() {
        ProfilePolicy::default().validate().expect("default policy");
        assert_eq!(permille_of(400, 100), 40);
        assert_eq!(permille_of(333, 10), 4);
        assert_eq!(permille_of(1000, 17), 17);
        assert_eq!(permille_of(1, 999), 1);
    }

    /// Out-of-range per-mille, a zero ms window and a zero cost are each refused.
    #[test]
    fn policy_bounds_are_enforced() {
        let mut policy = ProfilePolicy::default();
        policy.melee_windup_permille = 1001;
        assert!(policy.validate().is_err());

        let mut policy = ProfilePolicy::default();
        policy.melee_windup_permille = 0;
        assert!(policy.validate().is_err());

        let mut policy = ProfilePolicy::default();
        policy.spell_release_window_ms = 0;
        assert!(policy.validate().is_err());

        let mut policy = ProfilePolicy::default();
        policy.melee_light_stamina = 0;
        assert!(policy.validate().is_err());
    }
}
