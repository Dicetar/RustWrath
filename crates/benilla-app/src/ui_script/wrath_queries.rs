//! Synchronous UI queries over the existing catalog, known-spell and cast-cost owners.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_ui::script::{SpellInfoView, UnitCastView, WrathAuraView};

#[derive(SystemParam)]
pub(crate) struct SpellQueries<'w, 's> {
    spells: Option<Res<'w, crate::ui_action::Spells>>,
    known: Option<Res<'w, crate::ui_action::PlayerActions>>,
    pet: Option<Res<'w, crate::ui_pet::PetBar>>,
    world: Option<Res<'w, crate::wrath::WrathWorldView>>,
    aura_anchors: Option<Res<'w, crate::wrath::WrathAuraAnchors>>,
    player: Query<'w, 's, &'static crate::net::ObjectStore, With<crate::net::SelfPlayer>>,
    modifiers: Res<'w, crate::spell::SpellModifiers>,
    lines: Option<Res<'w, crate::ui_spellbook::SkillLines>>,
    items: Res<'w, crate::items::Items>,
    cooldowns: Option<Res<'w, crate::spell::Cooldowns>>,
    clock: Option<Res<'w, crate::ui_script::UiClock>>,
}

impl SpellQueries<'_, '_> {
    pub(crate) fn item_spell(&self, id: u32) -> Option<(String, u32)> {
        let item = self.items.template(id, 0, crate::query_cache::Ask::Read)?;
        let spell_id = item.use_spell?.spell_id;
        let spell = self.spells.as_deref()?.catalog.get(spell_id)?;
        Some((spell.name.clone(), spell_id))
    }

    pub(crate) fn item_cooldown(&self, id: u32) -> Option<(i64, u32, bool)> {
        let world = self.world.as_ref()?;
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) {
            return None;
        }
        let template = self.items.template(id, 0, crate::query_cache::Ask::Read)?;
        let Some(usage) = template.use_spell else {
            // Like the existing container query: a known non-use item has no cooldown leg.
            return Some((0, 0, false));
        };
        let definition = self.spells.as_deref()?.catalog.get(usage.spell_id)?;
        let clock = self.clock.as_deref()?;
        let cooldowns = self.cooldowns.as_deref()?;
        Some(
            cooldowns
                .info_item(usage.spell_id, id, usage.category, Some(definition), clock.anchor)
                .ui_triple(clock.anchor, clock.ui_now)
                .unwrap_or((0, 0, true)),
        )
    }

    pub(crate) fn cooldown(&self, id: u32, pet: bool) -> Option<(i64, u32, bool)> {
        let world = self.world.as_ref()?;
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) { return None; }
        let definition = self.spells.as_deref()?.catalog.get(id)?;
        let clock = self.clock.as_deref()?;
        let cooldowns = if pet {
            &self.pet.as_ref().filter(|pet| pet.has_bar())?.cooldowns
        } else {
            self.cooldowns.as_deref()?
        };
        Some(cooldowns.info(id, 0, Some(definition), clock.anchor)
            .ui_triple(clock.anchor, clock.ui_now).unwrap_or((0, 0, true)))
    }

    pub(crate) fn unit_auras(&self, guid: u64) -> Option<Vec<WrathAuraView>> {
        let world = self.world.as_ref()?;
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) { return None; }
        // Until the server has delivered an aura packet for this generation the slot set is
        // unavailable, not an observed empty list; the forced empty AURA_UPDATE_ALL is what
        // establishes the observed-empty state.
        let unit = world.entities.get(&guid).filter(|unit| unit.auras_observed)?;
        let catalog = &self.spells.as_deref()?.catalog;
        let mut result = Vec::new();
        for aura in world.auras(guid) {
            let Some(spell) = catalog.get(aura.spell_id) else { continue; };
            // AzerothCore SpellAuraDefines.h / AuraApplication::_InitFlags. These are not
            // the 1.12 descriptor nibble's effect/cancel flags or a sign implied by slot number.
            let helpful = match (aura.flags & 0x10 != 0, aura.flags & 0x80 != 0) {
                (true, false) => Some(true), (false, true) => Some(false), _ => None,
            };
            let timed = aura.flags & benilla_protocol_wrath::reducer::AFLAG_DURATION != 0;
            // An anchor belongs to one native receipt, not to the query or a movement digest.
            // Refuse a late/missing anchor instead of borrowing the prior occupant's timer.
            let expiration = if timed {
                unit.aura_receipts.get(&aura.slot).and_then(|receipt|
                    self.aura_anchors.as_ref()?.ui_receipt(guid, aura.slot)
                        .filter(|(revision, at)| *revision == receipt.revision && at.is_finite() && *at >= 0.0)
                        .map(|(_, at)| at + f64::from(aura.duration_ms) / 1000.0))
            } else {
                // Stock UIParent aura buttons subtract duration from expiration unconditionally.
                // 0/0 is the UI's no-timer tuple, not a claim that an omitted wire timer is permanent.
                Some(0.0)
            };
            result.push(WrathAuraView {
                slot: aura.slot, spell_id: aura.spell_id, name: spell.name.clone(),
                rank: spell.rank.clone(), icon: spell.icon.clone(), count: aura.charges, level: aura.level,
                dispel: catalog.dispel_name(spell).map(str::to_owned), helpful,
                caster: if aura.flags & benilla_protocol_wrath::reducer::AFLAG_CASTER != 0 {
                    Some(guid)
                } else { (aura.caster != 0).then_some(aura.caster) },
                duration: if timed { f64::from(aura.max_duration_ms) / 1000.0 } else { 0.0 },
                expiration,
                cancelable: helpful == Some(true) && spell.attributes & (0x8000_0000 | 0x40) == 0,
                stealable: None, consolidate: None,
            });
        }
        result.sort_by_key(|aura| aura.slot);
        Some(result)
    }

    pub(crate) fn unit_cast(&self, guid: u64, channel: bool) -> Option<UnitCastView> {
        let world = self.world.as_ref()?;
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) { return None; }
        let (scope, spell_id, start_ms, end_ms, cast_id, interruptible) = if channel {
            let active = world.channeling(guid)?;
            let start = active.start_ui_now_seconds? * 1000.0;
            let updated = active.last_update_ui_now_seconds? * 1000.0;
            let end = updated + f64::from(active.remaining_ms);
            (active.scope, active.spell_id, start, end, None, None)
        } else {
            let active = world.casting(guid)?;
            // START supplies time remaining, not the original server start or total haste-
            // adjusted duration. Display the observed interval without inventing either.
            let start = active.start_ui_now_seconds? * 1000.0 + f64::from(active.pushback_ms);
            let end = start + f64::from(active.start_remaining_ms?);
            // Lua numbers must retain the same exact identity in getters and event payloads.
            if active.attempt_serial > (1u64 << 53) { return None; }
            (active.scope, active.identity.spell_id, start, end, Some(active.attempt_serial), active.interruptible)
        };
        if scope.session_epoch != world.session_epoch || scope.world_generation != world.world_generation
            || !start_ms.is_finite() || !end_ms.is_finite() || end_ms < start_ms
        { return None; }
        let spell = self.spells.as_deref()?.catalog.get(spell_id)?;
        Some(UnitCastView {
            name: spell.name.clone(), rank: spell.rank.clone(), icon: spell.icon.clone(),
            start_ms, end_ms,
            trade_skill: spell.attributes & benilla_formats::SPELL_ATTR_IS_TRADESKILL != 0,
            cast_id, not_interruptible: interruptible.map(|value| !value),
        })
    }

    pub(crate) fn power(&self, guid: u64, kind: u8, maximum: bool) -> Option<u32> {
        let world = self.world.as_ref()?;
        if kind > 6 || world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) {
            return None;
        }
        let resources = world.resources(guid)?;
        if maximum { resources.max_powers[usize::from(kind)] }
        else { resources.powers[usize::from(kind)] }
    }

    pub(crate) fn power_type(&self, guid: u64) -> Option<u8> {
        let world = self.world.as_ref()?;
        if world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) {
            return None;
        }
        world.resources(guid)?.power_type.filter(|kind| *kind <= 6)
    }

    pub(crate) fn known(&self, id: u32, pet: bool) -> Option<bool> {
        if pet {
            self.pet.as_ref().map(|pet| pet.spells.spells.iter().any(|entry| entry.action() == id))
        } else {
            self.known.as_ref().map(|known| known.spells.contains(&id))
        }
    }

    pub(crate) fn info(&self, id: u32) -> Option<SpellInfoView> {
        let spells = self.spells.as_deref()?;
        let definition = spells.catalog.get(id)?;
        let player = self.player.single().ok();
        let skills = crate::spell::PlayerSkills {
            lines: self.lines.as_deref().map(|lines| &lines.catalog),
            spells: Some(&spells.catalog),
            items: Some(&self.items),
        };
        let cost = player.map(|player| {
            let raw = crate::spell::usable::power_cost(definition, player, skills,
                &self.modifiers, &mut crate::spell::Unrecorded);
            let scale = if definition.power_type == 6 { 10 } else {
                benilla_protocol::messages::power_display_scale(definition.power_type)
            };
            raw / scale as i32
        });
        let level = crate::spell::SpellLevel::Player { me: player, skills };
        let cast_time_ms = spells.cast_time_ms(definition, player.map(|player| &player.0),
            level, &self.modifiers);
        let range = spells.ranges.get(definition.range_index);
        Some(SpellInfoView {
            name: definition.name.clone(),
            rank: definition.rank.clone(),
            icon: definition.icon.clone(),
            cost,
            channeled: definition.tooltip_channeled(),
            power_type: definition.power_type as i32,
            cast_time_ms,
            min_range: range.map(|range| range.min),
            max_range: range.map(|range| range.max),
        })
    }
}
