//! The focus `[m_focusTarget]` (`CGGameUI::SetFocusTarget`, `0x51FF20`), beside the selection. The
//! VM owns the guid, as the reference's game UI does: `FocusUnit` and `ClearFocus` write it there
//! and signal `PLAYER_FOCUS_CHANGED` inside the verb, and a `/reload` starts a VM with none
//! (`CGGameUI::Shutdown`, `0x528F00`, clears it). [`FocusTarget`] mirrors it for the app's own
//! token resolver and adds what only the engine sees: the clears of `BreakTarget` (`0x524350`) when
//! the unit's object deactivates (`CGUnit_C::Disable`, `0x734FD0`) or turns unselectable
//! (`CGUnit_C::OnFlagChanged`, `0x73C330`), and of `CGGameUI::LeaveWorld` (`0x528C30`).
//!
//! Not modelled: `SMSG_BREAK_TARGET` (`CGGameUI::MessageHandler`, `0x526530`), which the app does
//! not decode for the selection either; the duel exception in `Disable`; a pet's `PetChangeHandler`
//! (`0x6D1970`) clear, which the despawn clear covers once the old pet's object is gone.

use bevy::prelude::*;

use crate::net::{GuidIndex, ObjectStore, SelfGuid, SelfPlayer};

/// `UNIT_FLAG_NOT_SELECTABLE`, `UNIT_FIELD_FLAGS` bit 25: the change that sets it runs
/// `BreakTarget` (`0x73C330`, `changed & 0x2000000`).
const UNIT_FLAG_NOT_SELECTABLE: u32 = 1 << 25;

/// The app's copy of the VM's focus, and the object it last found under it.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub(crate) struct FocusTarget {
    /// The focused guid, `None` for no focus.
    pub(crate) guid: Option<u64>,
    /// The object [`Self::guid`] named when last looked at: a different object, or none, is that
    /// one's deactivation.
    held: Option<Entity>,
    /// Whether that object was unselectable when last looked at, so only the edge breaks.
    unselectable: bool,
}

impl FocusTarget {
    /// A focus on `guid` with no object seen yet, for a resolver harness.
    #[cfg(test)]
    pub(crate) fn seated(guid: u64) -> Self {
        Self {
            guid: Some(guid),
            ..Self::default()
        }
    }
}

/// What happened to the focus's object since it was last looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusObject {
    /// The same object, selectable or already unselectable before.
    Kept,
    /// The same object just turned unselectable (`0x73C330`): every unit, raid members too.
    TurnedUnselectable,
    /// The object deactivated (`0x734FD0`): destroyed, out of range, or replaced under the guid.
    Deactivated,
    /// No object was held and none is, or one streamed in under the guid.
    Unheld,
}

/// Classify what the focus's object did: `was` the object last held and `now` the one the guid
/// names now, each with whether it is unselectable.
pub(crate) fn focus_object(
    was: Option<Entity>,
    was_unselectable: bool,
    now: Option<Entity>,
    now_unselectable: bool,
) -> FocusObject {
    match was {
        Some(held) if now == Some(held) => {
            if now_unselectable && !was_unselectable {
                FocusObject::TurnedUnselectable
            } else {
                FocusObject::Kept
            }
        }
        Some(_) => FocusObject::Deactivated,
        None => FocusObject::Unheld,
    }
}

/// Whether the focus ends: an unselectable edge always, a deactivation unless `IsRaidMemberOrPet`
/// holds the guid (`0x734FD0`, the same test the selection's teardown makes).
pub(crate) fn focus_breaks(object: FocusObject, member_or_pet: bool) -> bool {
    match object {
        FocusObject::TurnedUnselectable => true,
        FocusObject::Deactivated => !member_or_pet,
        FocusObject::Kept | FocusObject::Unheld => false,
    }
}

/// What following the focus reads.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FocusWorld<'w, 's> {
    focus: ResMut<'w, FocusTarget>,
    index: Res<'w, GuidIndex>,
    group: Res<'w, crate::ui_party::GroupState>,
    stores: Query<'w, 's, &'static ObjectStore>,
    self_guid: Res<'w, SelfGuid>,
    self_store: Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>,
}

impl FocusWorld<'_, '_> {
    /// The object `guid` names now, and whether it is an unselectable unit.
    fn object(&self, guid: u64) -> (Option<Entity>, bool) {
        let now = self.index.0.get(&guid).copied();
        let unselectable = now
            .and_then(|e| self.stores.get(e).ok())
            .filter(|s| s.is_unit())
            .is_some_and(|s| s.0.unit_flags() & UNIT_FLAG_NOT_SELECTABLE != 0);
        (now, unselectable)
    }

    /// `IsRaidMemberOrPet` (`0x491900`) on `guid`, as [`super::track`] asks it.
    fn member_or_pet(&self, guid: u64) -> bool {
        let held = |g: u64| {
            self.index
                .0
                .get(&g)
                .and_then(|e| self.stores.get(*e).ok())
                .map(|s| &s.0)
        };
        let me = self
            .self_guid
            .0
            .map(|g| (g, self.self_store.iter().next().map(|s| &s.0)));
        self.group.member_or_pet(guid, me, held)
    }

    /// Take the VM's focus as [`FocusTarget::guid`], seating its object when it moved.
    fn adopt(&mut self, vm_focus: u64) {
        let guid = (vm_focus != 0).then_some(vm_focus);
        if self.focus.guid != guid {
            let (held, unselectable) = guid.map_or((None, false), |g| self.object(g));
            *self.focus = FocusTarget {
                guid,
                held,
                unselectable,
            };
        }
    }
}

/// Follow the focus's object once the net stage has applied the objects that streamed in and out:
/// adopt the VM's focus, then end it where `BreakTarget` would, signalling `PLAYER_FOCUS_CHANGED`
/// through the VM's `SetFocusTarget`. No in-game VM is no focus.
pub(crate) fn follow_focus(mut hosted_vm: crate::script_calls::HostedVm, mut world: FocusWorld) {
    hosted_vm.run(|script| {
        let Some(script) = script else {
            *world.focus = FocusTarget::default();
            return;
        };
        world.adopt(script.focus_guid());
        let Some(guid) = world.focus.guid else {
            return;
        };
        let (now, unselectable) = world.object(guid);
        let object = focus_object(
            world.focus.held,
            world.focus.unselectable,
            now,
            unselectable,
        );
        let member = object == FocusObject::Deactivated && world.member_or_pet(guid);
        if focus_breaks(object, member) {
            script.set_focus_target(0);
            *world.focus = FocusTarget::default();
        } else {
            world.focus.held = now;
            world.focus.unselectable = unselectable;
        }
    })
}

/// `CGGameUI::LeaveWorld` (`0x528C30`): a focus that is not a raid member or pet is cleared
/// (`GameUI.cpp` `0x528C30`, after the selection, before `PLAYER_LEAVING_WORLD`), signalled. The
/// logout's `Shutdown` (`0x528F00`) clears it whatever it is, which the VM's end does here.
pub(crate) fn clear_focus_on_leave_world(
    mut hosted_vm: crate::script_calls::HostedVm,
    mut world: FocusWorld,
) {
    hosted_vm.run(|script| {
        let Some(script) = script else {
            return;
        };
        let guid = script.focus_guid();
        if guid != 0 && !world.member_or_pet(guid) {
            script.set_focus_target(0);
            *world.focus = FocusTarget::default();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A despawn ends the focus unless a raid member or pet holds the guid; the unselectable edge
    /// ends it for anyone, and only on the edge.
    #[test]
    fn the_focus_breaks_where_break_target_runs() {
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let gone = focus_object(Some(a), false, None, false);
        assert_eq!(gone, FocusObject::Deactivated);
        assert!(
            focus_breaks(gone, false),
            "a despawned stranger ends the focus"
        );
        assert!(
            !focus_breaks(gone, true),
            "a raid member out of range keeps it"
        );
        let replaced = focus_object(Some(a), false, Some(b), false);
        assert!(
            focus_breaks(replaced, false),
            "a new object under the guid is a deactivation"
        );
        let edge = focus_object(Some(a), false, Some(a), true);
        assert!(
            focus_breaks(edge, true),
            "the unselectable edge ends it for a member too"
        );
        assert!(!focus_breaks(
            focus_object(Some(a), true, Some(a), true),
            false
        ));
        assert!(!focus_breaks(
            focus_object(None, false, Some(a), false),
            false
        ));
    }
}
