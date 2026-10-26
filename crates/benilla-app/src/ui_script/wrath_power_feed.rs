//! Wrath power notifications over the existing native resource owner. No power prediction or
//! rune cooldown simulation lives here; this is a scoped memo of what the UI has been notified of.

use std::collections::BTreeMap;
use bevy::prelude::*;
use benilla_ui::script::{ScriptValue, UiTarget};
use crate::wrath_native::WrathUnitResources;

#[derive(Default)]
pub(crate) struct PowerMemo {
    scope: Option<(u64, u64, u64)>,
    visible: BTreeMap<u64, (u32, WrathUnitResources)>,
}

const EVENTS: [Option<(&str, &str)>; 7] = [
    Some(("UNIT_MANA", "UNIT_MAXMANA")),
    Some(("UNIT_RAGE", "UNIT_MAXRAGE")),
    Some(("UNIT_FOCUS", "UNIT_MAXFOCUS")),
    Some(("UNIT_ENERGY", "UNIT_MAXENERGY")),
    Some(("UNIT_HAPPINESS", "UNIT_MAXHAPPINESS")),
    // RUNE_POWER_UPDATE takes an individual rune index/readiness, not an aggregate unit field.
    None,
    Some(("UNIT_RUNIC_POWER", "UNIT_MAXRUNIC_POWER")),
];

pub(crate) fn feed(
    mut vm: crate::script_calls::VmWith<Option<Res<'static, crate::wrath::WrathWorldView>>>,
    mut memo: Local<PowerMemo>,
) {
    vm.feed(|script, world, fires| {
        let Some(script) = script.filter(|script| script.target() == UiTarget::Wrath335) else {
            *memo = PowerMemo::default();
            return;
        };
        let Some(world) = world.filter(|world|
            world.phase == Some(benilla_protocol_wrath::session::Phase::InWorld)) else {
            *memo = PowerMemo::default();
            return;
        };
        let scope = (script.session(), world.session_epoch, world.world_generation);
        if memo.scope != Some(scope) {
            memo.visible.clear();
            memo.scope = Some(scope);
        }
        let mut retained = BTreeMap::new();
        let mut visible = script.held_unit_guids();
        visible.sort_unstable();
        for guid in visible {
            let Some(unit) = world.entities.get(&guid) else { continue; };
            let Some(resources) = world.resources(guid) else { continue; };
            let previous = memo.visible.get(&guid).filter(|(generation, _)| *generation == unit.generation)
                .map(|(_, resources)| resources);
            let mut changed = Vec::new();
            if resources.power_type.is_some()
                && previous.is_none_or(|previous| previous.power_type != resources.power_type)
            { changed.push("UNIT_DISPLAYPOWER"); }
            for (kind, events) in EVENTS.iter().enumerate() {
                let Some((current, maximum)) = events else { continue; };
                if resources.powers[kind].is_some()
                    && previous.is_none_or(|previous| previous.powers[kind] != resources.powers[kind])
                { changed.push(*current); }
                if resources.max_powers[kind].is_some()
                    && previous.is_none_or(|previous| previous.max_powers[kind] != resources.max_powers[kind])
                { changed.push(*maximum); }
            }
            for token in script.unit_tokens_of(guid) {
                for event in &changed { fires.post(event, vec![ScriptValue::Str(token.clone())]); }
            }
            retained.insert(guid, (unit.generation, resources));
        }
        memo.visible = retained;
    });
}
