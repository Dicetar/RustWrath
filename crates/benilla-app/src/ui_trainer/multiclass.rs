//! WM's ClassifyTrainerSpell fallback, using the loaded DBCs rather than a generated header.

use std::collections::{HashMap, HashSet};

use benilla_formats::{SkillLineCatalog, SpellCatalog, SPELL_EFFECT_LEARN_SPELL};

pub(crate) struct TrainerClasses(HashMap<u32, Vec<(u16, u32)>>);

impl TrainerClasses {
    pub(super) fn new(lines: &SkillLineCatalog) -> Self {
        Self(lines.class_training_lines())
    }

    pub(super) fn skill(&self, spell: u32, owned: u16, spells: &SpellCatalog) -> Option<u32> {
        if owned == 0 { return None; }
        let mut work = vec![spell];
        let mut seen = HashSet::new();
        let mut best = None;
        while let Some(spell) = work.pop() {
            if !seen.insert(spell) { continue; }
            if let Some(rows) = self.0.get(&spell) {
                for &(mask, line) in rows {
                    if mask & owned != 0 {
                        let candidate = (mask, line);
                        best = Some(best.map_or(candidate, |old| std::cmp::min(old, candidate)));
                    }
                }
            }
            // The generator propagates class labels through all LEARN_SPELL wrappers.
            // A visited set handles malformed cyclic DBCs without recursion or invented ranks.
            if let Some(display) = spells.get(spell) {
                for (effect, taught) in display.effects.iter().zip(display.effect_trigger_spell) {
                    if *effect == SPELL_EFFECT_LEARN_SPELL && taught != 0 { work.push(taught); }
                }
            }
        }
        best.map(|(_, line)| line)
    }
}
