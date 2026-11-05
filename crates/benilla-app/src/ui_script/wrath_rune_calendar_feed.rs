//! Project the scoped Wrath rune/calendar view into the Wrath VM's model. The view is the only
//! owner of the decoded wire facts; this feed copies them into the VM once per frame and drains
//! `OpenCalendar()` into the shared command tap. Nothing here predicts a rune or invents a date.

use bevy::prelude::*;
use benilla_protocol_wrath::session::Phase;
use benilla_ui::script::UiTarget;

use crate::net::{ClientCommand, NetCommands};
use crate::script_calls::VmWith;
use crate::ui_script::UiClock;
use crate::wrath::WrathWorldView;

pub(crate) fn feed(
    mut vm: VmWith<(
        Option<Res<'static, WrathWorldView>>,
        Option<Res<'static, UiClock>>,
        Option<Res<'static, NetCommands>>,
    )>,
) {
    vm.turn(|script, (world, clock, net)| {
        let Some(script) = script else {
            return;
        };
        if script.target() != UiTarget::Wrath335 {
            script.clear_wrath_scoped_ui();
            return;
        }
        let Some(world) = world.as_deref() else {
            script.clear_wrath_scoped_ui();
            return;
        };
        if world.phase != Some(Phase::InWorld) || world.player_guid == 0 {
            script.clear_wrath_scoped_ui();
            return;
        }

        let ui_now = clock.as_deref().map_or(0.0, |clock| clock.ui_now);
        script.set_wrath_scope(world.session_epoch, world.world_generation);

        if world.runes.is_empty() {
            script.clear_runes();
        } else {
            let slots: Vec<(u8, u8, f64)> = world
                .runes
                .iter()
                .map(|rune| {
                    (
                        rune.rune_type,
                        rune.cooldown,
                        // A resync decoded before the VM existed has no GetTime anchor yet;
                        // the first frame with a UI clock becomes its anchor.
                        if rune.received_at > 0.0 {
                            rune.received_at
                        } else {
                            ui_now
                        },
                    )
                })
                .collect();
            script.set_runes(&slots);
        }

        match &world.calendar {
            Some(calendar) => {
                // `CountPendingInvites` (`Calendar.cpp:293-303`) counts rows whose inviter is
                // set; the ignore-list half has no owner in this projection.
                let pending = calendar
                    .invites
                    .iter()
                    .filter(|invite| invite.inviter != 0)
                    .count();
                script.set_calendar(
                    calendar.server_time,
                    calendar.zone_time,
                    pending,
                    world.calendar_revision,
                );
            }
            None => script.clear_calendar(),
        }

        let fetches = script.take_calendar_fetches();
        if fetches.is_empty() {
            return;
        }
        let Some(net) = net.as_deref() else {
            return;
        };
        for (session_epoch, world_generation) in fetches {
            // The tap enforces the same rule; refusing here keeps a stale request from even
            // entering the shared command queue.
            if session_epoch != world.session_epoch || world_generation != world.world_generation {
                continue;
            }
            let _ = net.0.send(ClientCommand::OpenCalendar {
                session_epoch,
                world_generation,
            });
        }
    });
}
