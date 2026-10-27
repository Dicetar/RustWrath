//! UI change detection over the native aura owner. The memo is limited to the resolver's
//! currently reachable units and expires with its VM/session/world/object generation.

use std::collections::BTreeMap;

use bevy::prelude::*;
use benilla_protocol_wrath::reducer::{AuraReceipt, AuraSlot};
use benilla_ui::script::{ScriptValue, UiTarget};

#[derive(Default)]
pub(crate) struct AuraMemo {
    scope: Option<(u64, u64, u64)>,
    visible: BTreeMap<u64, AuraSnapshot>,
}

struct AuraSnapshot {
    generation: u32,
    observed: bool,
    slots: Vec<AuraSlot>,
    receipts: BTreeMap<u8, AuraReceipt>,
}

pub(crate) fn feed(
    mut vm: crate::script_calls::VmWith<Option<Res<'static, crate::wrath::WrathWorldView>>>,
    mut memo: Local<AuraMemo>,
) {
    vm.feed(|script, world, fires| {
        let Some(script) = script.filter(|script| script.target() == UiTarget::Wrath335) else {
            *memo = AuraMemo::default();
            return;
        };
        let Some(world) = world.filter(|world|
            world.phase == Some(benilla_protocol_wrath::session::Phase::InWorld)) else {
            *memo = AuraMemo::default();
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
            let previous = memo.visible.remove(&guid);
            let Some(unit) = world.entities.get(&guid) else {
                if previous.is_some() {
                    for token in script.unit_tokens_of(guid) {
                        fires.post("UNIT_AURA", vec![ScriptValue::Str(token)]);
                    }
                }
                continue;
            };
            let changed = previous.as_ref().is_none_or(|previous|
                previous.generation != unit.generation || previous.observed != unit.auras_observed
                    || previous.slots != unit.auras || previous.receipts != unit.aura_receipts);
            if changed {
                for token in script.unit_tokens_of(guid) {
                    fires.post("UNIT_AURA", vec![ScriptValue::Str(token)]);
                }
            }
            // Move unchanged snapshots rather than cloning on movement-only frames. A receipt
            // revision still notifies when a refresh leaves every wire slot value identical.
            let snapshot = if changed { AuraSnapshot {
                generation: unit.generation, observed: unit.auras_observed,
                slots: unit.auras.clone(), receipts: unit.aura_receipts.clone(),
            } }
                else { previous.expect("unchanged visible unit") };
            retained.insert(guid, snapshot);
        }
        for &guid in memo.visible.keys() {
            for token in script.unit_tokens_of(guid) {
                fires.post("UNIT_AURA", vec![ScriptValue::Str(token)]);
            }
        }
        memo.visible = retained;
    });
}
