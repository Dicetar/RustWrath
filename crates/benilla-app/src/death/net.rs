//! The death packet handlers filling [`DeathNet`], the movement-mode forward for our own mover,
//! and the corpse-guid latch's hooks on the object lifecycle, called from the drain's object arms.

use benilla_protocol::{EntityKind, MoveMode, ObjectFields, SessionEvent, SessionEventKind};
use bevy::prelude::*;

use super::{CorpsePoint, DeathNet, ReleaseLocation, ResurrectOffer};
use crate::net::{MoveModeMessage, NetHandlerApp, ObjectStore, SelfGuid, SelfPlayer};

/// Register the death handlers and the session-end listener.
pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.net_handler(K::CorpseQuery, on_corpse_query)
        .net_handler(K::CorpseReclaimDelay, on_corpse_reclaim_delay)
        .net_handler(K::ResurrectRequest, on_resurrect_request)
        .net_handler(K::SpiritHealerConfirm, on_spirit_healer_confirm)
        .net_handler(K::DurabilityDamageDeath, on_durability_damage_death)
        .net_handler(K::DeathReleaseLoc, on_death_release_loc)
        .net_handler(K::PreResurrect, on_pre_resurrect)
        .net_handler(K::CorpseNotInInstance, on_corpse_not_in_instance)
        .net_handler(K::MoveMode, on_move_mode)
        .net_handler(K::Disconnected, on_session_end);
}

fn on_corpse_query(In(ev): In<SessionEvent>, mut death_net: ResMut<DeathNet>) {
    if let SessionEvent::CorpseQuery {
        found,
        display_map,
        position,
        corpse_map,
    } = ev
    {
        corpse_query(found, display_map, position, corpse_map, &mut death_net);
    }
}

fn on_corpse_reclaim_delay(
    In(ev): In<SessionEvent>,
    mut death_net: ResMut<DeathNet>,
    real_clock: Res<Time<Real>>,
) {
    if let SessionEvent::CorpseReclaimDelay { delay_ms } = ev {
        corpse_reclaim_delay(delay_ms, real_clock.elapsed_secs_f64(), &mut death_net);
    }
}

fn on_resurrect_request(In(ev): In<SessionEvent>, mut death_net: ResMut<DeathNet>) {
    if let SessionEvent::ResurrectRequest {
        caster,
        name,
        sickness,
        has_timer,
    } = ev
    {
        resurrect_request(caster, name, sickness, has_timer, &mut death_net);
    }
}

fn on_spirit_healer_confirm(In(ev): In<SessionEvent>, mut death_net: ResMut<DeathNet>) {
    if let SessionEvent::SpiritHealerConfirm { npc } = ev {
        spirit_healer_confirm(npc, &mut death_net);
    }
}

fn on_durability_damage_death(In(ev): In<SessionEvent>, log: Res<crate::ui_chat::ChatLog>) {
    if let SessionEvent::DurabilityDamageDeath = ev {
        durability_damage_death(&log);
    }
}

fn on_death_release_loc(
    In(ev): In<SessionEvent>,
    mut death_net: ResMut<DeathNet>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
) {
    if let SessionEvent::DeathReleaseLoc { map_id, position } = ev {
        let ghost = me
            .iter()
            .next()
            .is_some_and(|store| store.0.player_is_ghost());
        death_release_loc(map_id, position, ghost, &mut death_net);
    }
}

/// `SMSG_PRE_RESURRECT`: the body turning ghost drops every projectile still homing on it
/// (`Player_C.cpp:22246-22252` → `CMissile::ClearInFlightMissilesWithTarget`).
fn on_pre_resurrect(
    In(ev): In<SessionEvent>,
    mut clear: MessageWriter<crate::entities::ClearMissilesAt>,
) {
    if let SessionEvent::PreResurrect { guid } = ev {
        clear.write(crate::entities::ClearMissilesAt { target: guid });
    }
}

/// `SMSG_AREA_TRIGGER_NO_CORPSE`, the instance entrance's refusal of a ghost whose corpse lies
/// elsewhere: `CGGameUI::DisplayError(ERR_CORPSE_IS_NOT_IN_INSTANCE)` (`GameUI.cpp:6821-6824`).
fn on_corpse_not_in_instance(In(ev): In<SessionEvent>, errors: Res<crate::ui_action::UiErrorKeys>) {
    if let SessionEvent::CorpseNotInInstance = ev {
        errors.push(crate::ui_action::UiError::key(
            "ERR_CORPSE_IS_NOT_IN_INSTANCE",
        ));
    }
}

fn on_move_mode(
    In(ev): In<SessionEvent>,
    self_guid: Res<SelfGuid>,
    mut death_net: ResMut<DeathNet>,
    mut out: MessageWriter<MoveModeMessage>,
) {
    if let SessionEvent::MoveMode {
        guid,
        counter,
        mode,
        apply,
    } = ev
    {
        move_mode(
            guid,
            counter,
            mode,
            apply,
            &self_guid,
            &mut death_net,
            &mut out,
        );
    }
}

/// The death stores are session-scoped; a reconnect while dead re-sends the reclaim delay.
fn on_session_end(In(_): In<SessionEvent>, mut death_net: ResMut<DeathNet>) {
    *death_net = DeathNet::default();
}

/// Our corpse streaming in: latch its guid for the reclaim send. The reference's latch
/// `[0xb4e328]` has one writer, `0x4920d0`, gated on `CORPSE_FIELD_OWNER == me` and
/// `CORPSE_FIELD_FLAGS` bit 0 clear, so bones never arm it; `RetrieveCorpse` sends whatever is in
/// the latch.
pub(crate) fn note_corpse(
    guid: u64,
    kind: EntityKind,
    fields: &ObjectFields,
    self_guid: &SelfGuid,
    death_net: &mut DeathNet,
) {
    if kind == EntityKind::Corpse
        && fields.corpse_owner() == self_guid.0
        && !fields.corpse_is_bones()
    {
        death_net.corpse_guid = Some(guid);
    }
}

/// The same latch, re-asked when `CORPSE_FIELD_FLAGS` changes under a live guid, as the
/// reference's `FLAGS` mirror handler `0x5d6d60` does, so an in-place flip to bones drops it.
pub(crate) fn recheck_corpse(
    guid: u64,
    fields: &ObjectFields,
    self_guid: &SelfGuid,
    death_net: &mut DeathNet,
) {
    // A delta carries only what changed: an absent FLAGS field is not a clear bit.
    let Some(flags) = fields.corpse_flags_present() else {
        return;
    };
    let mine = fields.corpse_owner() == self_guid.0 || death_net.corpse_guid == Some(guid);
    if !mine {
        return;
    }
    if flags & 0x01 != 0 {
        if death_net.corpse_guid == Some(guid) {
            death_net.corpse_guid = None;
        }
    } else if fields.corpse_owner() == self_guid.0 {
        death_net.corpse_guid = Some(guid);
    }
}

/// The corpse-to-bones swap destroys the corpse object; a stale guid must not ride a reclaim.
pub(crate) fn forget_corpse(guid: u64, death_net: &mut DeathNet) {
    if death_net.corpse_guid == Some(guid) {
        death_net.corpse_guid = None;
    }
}

/// `MSG_CORPSE_QUERY`'s answer; a not-found, asked or pushed at bones conversion, drops the marker.
fn corpse_query(
    found: bool,
    display_map: i32,
    position: [f32; 3],
    corpse_map: u32,
    death_net: &mut DeathNet,
) {
    death_net.corpse = found.then_some(CorpsePoint {
        display_map,
        position,
        corpse_map,
    });
}

/// `SMSG_CORPSE_RECLAIM_DELAY`, anchored at arrival on the clock the feed reads. The 1.12 client
/// re-fires the corpse-range events on each (`0x4962d0`), hence the generation bump.
fn corpse_reclaim_delay(delay_ms: u32, now_secs: f64, death_net: &mut DeathNet) {
    death_net.reclaim_at = Some(now_secs + f64::from(delay_ms) / 1000.0);
    death_net.reclaim_generation = death_net.reclaim_generation.wrapping_add(1);
}

/// `SMSG_DEATH_RELEASE_LOC` (`Player.cpp:4860-4872`): where the release teleported the ghost,
/// the minimap spirit healer's position and `GetDeathReleasePosition`'s source. 12340's handler
/// (`Player_C.cpp:9720-9736`) keeps it only while the active player is **not** a ghost: the
/// ghost leg, and a missing player, replace it with `SetDeathReleaseLocation(-1, zero)`
/// (`GameUI.cpp:3451-3459`). `map_id` itself is signed because a stored leg may carry `-1`
/// (`ResurrectPlayer`, `Player.cpp:4380-4385`), which projects off every map.
fn death_release_loc(map_id: i32, position: [f32; 3], ghost: bool, death_net: &mut DeathNet) {
    death_net.release_location = (!ghost).then_some(ReleaseLocation { map_id, position });
}

/// `SMSG_RESURRECT_REQUEST`, the RESURRECT popup's data.
fn resurrect_request(
    caster: u64,
    name: String,
    sickness: bool,
    has_timer: bool,
    death_net: &mut DeathNet,
) {
    death_net.resurrect = Some(ResurrectOffer {
        caster,
        name,
        sickness,
        has_timer,
    });
    death_net.resurrect_generation = death_net.resurrect_generation.wrapping_add(1);
}

/// A player caster's offer, its name empty on the wire, as [`resurrect_request`] records it.
#[cfg(test)]
pub(super) fn resurrect_request_for_tests(caster: u64, death_net: &mut DeathNet) {
    resurrect_request(caster, String::new(), false, true, death_net);
}

/// `SMSG_SPIRIT_HEALER_CONFIRM`: the healer's gossip re-sends it on every ask, and the reference
/// fires `CONFIRM_XP_LOSS` per arrival.
fn spirit_healer_confirm(npc: u64, death_net: &mut DeathNet) {
    death_net.ask_spirit_healer(npc);
}

/// `SMSG_DURABILITY_DAMAGE_DEATH`, the 10% death durability loss: a combat-log line at chat type
/// `0x19` `COMBAT_MISC_INFO` with no arguments (`0x628e60`), not an error; the body is ignored.
fn durability_damage_death(log: &crate::ui_chat::ChatLog) {
    log.push_combat(crate::ui_chat::combat::PendingCombat {
        kind: crate::ui_chat::ChatEventKind::CombatMiscInfo,
        family: crate::ui_chat::combat::DURABILITYDAMAGE_DEATH,
        variant: crate::ui_chat::combat::Variant::OtherOther,
        subject: 0,
        object: 0,
        fills: crate::ui_chat::combat::Fills::default(),
        named: crate::ui_chat::combat::Named::Ready,
        tries: 0,
        word: None,
    });
}

/// The acked movement modes, root, water-walk, feather-fall and hover, addressed only to our own
/// mover and applied by the controller ([`crate::player::wire_in`]); water-walk is also mirrored
/// into [`DeathNet`] as a ghost cue.
fn move_mode(
    guid: u64,
    counter: u32,
    mode: MoveMode,
    apply: bool,
    self_guid: &SelfGuid,
    death_net: &mut DeathNet,
    out: &mut MessageWriter<MoveModeMessage>,
) {
    if self_guid.0 != Some(guid) {
        return;
    }
    if mode == MoveMode::WaterWalk {
        death_net.water_walk = apply;
    }
    out.write(MoveModeMessage {
        guid,
        counter,
        mode,
        apply,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SMSG_DEATH_RELEASE_LOC` stores the graveyard the release named and `ResurrectPlayer`'s
    /// `-1` clears it, so a resurrection leaves no world-map or minimap release marker behind
    /// (`Player_C.cpp:9720-9736`, `Player.cpp:4380-4385`).
    /// `SMSG_CORPSE_RECLAIM_DELAY` anchors the wait on the clock the feed reads and bumps the
    /// generation that re-fires the corpse-range events (`0x4962d0`), so RECOVER_CORPSE re-shows
    /// with the new delay.
    #[test]
    fn the_reclaim_delay_anchors_and_re_announces() {
        let mut death = DeathNet::default();
        corpse_reclaim_delay(30_000, 100.0, &mut death);
        assert_eq!(death.reclaim_at, Some(130.0));
        assert_eq!(death.reclaim_generation, 1);
        corpse_reclaim_delay(60_000, 130.0, &mut death);
        assert_eq!(death.reclaim_at, Some(190.0));
        assert_eq!(death.reclaim_generation, 2);
    }

    /// `SMSG_RESURRECT_REQUEST` holds the offer the popup answers and announces it once per
    /// message; the answer takes it, and the next packet starts a fresh announcement.
    #[test]
    fn a_resurrect_offer_is_held_and_announced_per_message() {
        let mut death = DeathNet::default();
        resurrect_request_for_tests(0x9, &mut death);
        assert_eq!(death.resurrect.as_ref().map(|o| o.caster), Some(0x9));
        assert_eq!(death.resurrect_generation, 1);
        assert!(death.resurrect.take().is_some(), "the answer takes the offer");
        resurrect_request_for_tests(0xA, &mut death);
        assert_eq!(death.resurrect_generation, 2);
        assert_eq!(death.resurrect.as_ref().map(|o| o.caster), Some(0xA));
    }

    /// The reclaim guid is the reference's one-writer latch: our streamed body arms it, another
    /// player's never replaces it, and the bone-pile flag (bit 0) never arms it (`0x4920d0`).
    /// A destroyed corpse drops the guid so a later reclaim cannot send a dead one.
    #[test]
    fn the_corpse_latch_arms_for_our_body_and_never_its_bones() {
        use benilla_protocol::messages::ObjectType;

        const ME: u64 = 0x11;
        const BODY: u64 = 0xC0_0000_0000_0001;
        const BONES: u64 = 0xC0_0000_0000_0002;
        let me = SelfGuid(Some(ME));
        let corpse_fields = |owner: u64, flags: Option<u32>| {
            let mut pairs = vec![(6u16, owner as u32), (7, 0)];
            if let Some(flags) = flags {
                pairs.push((35, flags));
            }
            ObjectFields::from_pairs(&pairs).into_created(ObjectType::Corpse)
        };

        let mut death = DeathNet::default();
        note_corpse(BODY, EntityKind::Corpse, &corpse_fields(ME, None), &me, &mut death);
        assert_eq!(death.corpse_guid, Some(BODY), "our body arms the reclaim");

        // Another player's body must not steal the latch.
        note_corpse(
            0xC0_0000_0000_0003,
            EntityKind::Corpse,
            &corpse_fields(0x22, None),
            &me,
            &mut death,
        );
        assert_eq!(death.corpse_guid, Some(BODY));

        // A bone pile keeps the owner but flag bit 0 says bones: it never arms.
        note_corpse(
            BONES,
            EntityKind::Corpse,
            &corpse_fields(ME, Some(1)),
            &me,
            &mut death,
        );
        assert_eq!(death.corpse_guid, Some(BODY), "bones never arm");

        // An in-place flip to bones under the live guid drops the latch.
        recheck_corpse(BODY, &corpse_fields(ME, Some(1)), &me, &mut death);
        assert_eq!(death.corpse_guid, None, "the bone flip drops it");

        note_corpse(
            BODY,
            EntityKind::Corpse,
            &corpse_fields(ME, None),
            &me,
            &mut death,
        );
        forget_corpse(BODY, &mut death);
        assert_eq!(death.corpse_guid, None, "the destroyed corpse drops it");
    }

    /// 12340's handler (`Player_C.cpp:9720-9736`) stores the graveyard only while the active
    /// player is not a ghost: the ghost leg (and a missing player) replaces it with
    /// `SetDeathReleaseLocation(-1, zero)` (`GameUI.cpp:3451-3459`). AzerothCore sends the packet
    /// right after the ghost aura (`Player.cpp:4860-4872`) and sends `-1` while the flag is still
    /// set (`Player.cpp:4380-4392`), so both arrive on the clear leg — the same answer, and no
    /// stale marker after a resurrection.
    #[test]
    fn a_release_location_is_kept_only_while_not_a_ghost() {
        let mut death = DeathNet::default();
        death_release_loc(0, [-8943.5, 132.25, 61.5], false, &mut death);
        assert_eq!(
            death.release_location,
            Some(ReleaseLocation {
                map_id: 0,
                position: [-8943.5, 132.25, 61.5],
            })
        );
        death_release_loc(0, [-8943.5, 132.25, 61.5], true, &mut death);
        assert_eq!(death.release_location, None, "a ghost keeps no location");
        death_release_loc(-1, [0.0, 0.0, 0.0], true, &mut death);
        assert_eq!(death.release_location, None, "the resurrection clears too");
    }
}
