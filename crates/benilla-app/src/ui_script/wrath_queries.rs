//! Synchronous UI queries over the existing catalog, known-spell and cast-cost owners.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use benilla_ui::script::SpellInfoView;

#[derive(SystemParam)]
pub(crate) struct SpellQueries<'w, 's> {
    spells: Option<Res<'w, crate::ui_action::Spells>>,
    known: Option<Res<'w, crate::ui_action::PlayerActions>>,
    pet: Option<Res<'w, crate::ui_pet::PetBar>>,
    world: Option<Res<'w, crate::wrath::WrathWorldView>>,
    objects: crate::net::Objects<'w, 's>,
    player: Query<'w, 's, &'static crate::net::ObjectStore, With<crate::net::SelfPlayer>>,
    modifiers: Res<'w, crate::spell::SpellModifiers>,
    lines: Option<Res<'w, crate::ui_spellbook::SkillLines>>,
    items: Res<'w, crate::items::Items>,
}

impl SpellQueries<'_, '_> {
    pub(crate) fn power(&self, guid: u64, kind: u8, maximum: bool) -> Option<u32> {
        let world = self.world.as_ref()?;
        if kind > 6 || world.phase != Some(benilla_protocol_wrath::session::Phase::InWorld) {
            return None;
        }
        let entity = world.entities.get(&guid)?;
        if let Some(fields) = entity.player_fields.as_ref() {
            let base = if maximum { benilla_protocol_wrath::fields::UNIT_FIELD_MAXPOWER1 }
                else { benilla_protocol_wrath::fields::UNIT_FIELD_POWER1 };
            return fields.get(usize::from(base) + usize::from(kind)).copied();
        }
        let fields = self.objects.object(guid)?;
        if maximum { fields.unit_max_power(kind) } else { fields.unit_power(kind) }
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
