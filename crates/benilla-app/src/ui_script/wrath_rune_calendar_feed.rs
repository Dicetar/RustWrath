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

        let (
            joined,
            queued,
            lfg_joined,
            no_partial_clear,
            achievements,
            queued_dungeons,
            comment,
        ) = if let Some(party) = &world.lfg_update_party {
            (
                party.joined,
                party.queued.unwrap_or(false),
                party.lfg_joined.unwrap_or(party.joined),
                party.no_partial_clear.unwrap_or(false),
                party.achievements.unwrap_or(false),
                party.dungeon_ids.clone(),
                party.comment.as_deref(),
            )
        } else if let Some(player) = &world.lfg_update_player {
            (
                player.joined,
                player.queued.unwrap_or(false),
                false,
                player.no_partial_clear.unwrap_or(false),
                player.achievements.unwrap_or(false),
                player.dungeon_ids.clone(),
                player.comment.as_deref(),
            )
        } else {
            (false, false, false, false, false, Vec::new(), None)
        };
        let last_join_result = world
            .lfg_join_result
            .as_ref()
            .map(|r| (r.result, r.state));
        let queue_status = world.lfg_queue_status.map(|s| {
            benilla_ui::script::WrathLfgQueueStatusState {
                dungeon_id: s.dungeon_id,
                wait_time_avg: s.wait_time_avg,
                wait_time: s.wait_time,
                wait_time_tank: s.wait_time_tank,
                wait_time_healer: s.wait_time_healer,
                wait_time_dps: s.wait_time_dps,
                tanks_needed: s.tanks_needed,
                healers_needed: s.healers_needed,
                dps_needed: s.dps_needed,
                queued_time: s.queued_time,
            }
        });
        let proposal = world.lfg_proposal.as_ref().map(|p| {
            benilla_ui::script::WrathLfgProposalState {
                proposal_id: p.proposal_id,
                dungeon_id: p.dungeon_id,
                state: p.state,
                completed_mask: p.completed_mask,
                silent: p.silent,
                players: p
                    .players
                    .iter()
                    .map(|m| benilla_ui::script::WrathLfgProposalMemberState {
                        role: m.role,
                        is_local: m.is_local,
                        in_dungeon: m.in_dungeon,
                        in_same_group: m.in_same_group,
                        responded: m.responded,
                        accepted: m.accepted,
                    })
                    .collect(),
            }
        });
        script.set_wrath_lfg_server_state(
            joined,
            queued,
            lfg_joined,
            no_partial_clear,
            achievements,
            queued_dungeons,
            comment,
            last_join_result,
            queue_status,
            proposal,
        );

        let fetches = script.take_calendar_fetches();
        let scoped_commands = script.take_wrath_scoped_commands();
        if fetches.is_empty() && scoped_commands.is_empty() {
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
        for cmd in scoped_commands {
            use benilla_ui::script::WrathScopedCommand as C;
            match cmd {
                C::LfgJoin {
                    slots,
                    comment,
                    roles,
                    no_partial_clear,
                    achievements,
                    needs,
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::LfgJoin {
                        slots,
                        comment,
                        roles,
                        no_partial_clear,
                        achievements,
                        needs,
                        session_epoch,
                        world_generation,
                    });
                }
                C::LfgLeave {
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::LfgLeave {
                        session_epoch,
                        world_generation,
                    });
                }
                C::LfgProposalResult {
                    proposal_id,
                    accept,
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::LfgProposalResult {
                        proposal_id,
                        accept,
                        session_epoch,
                        world_generation,
                    });
                }
                C::SetLfgComment {
                    comment,
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::SetLfgComment {
                        comment,
                        session_epoch,
                        world_generation,
                    });
                }
                C::LfgSetRoles {
                    roles,
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::LfgSetRoles {
                        roles,
                        session_epoch,
                        world_generation,
                    });
                }
                C::VehicleExit {
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::VehicleExit {
                        session_epoch,
                        world_generation,
                    });
                }
                C::VehiclePrevSeat {
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::VehiclePrevSeat {
                        session_epoch,
                        world_generation,
                    });
                }
                C::VehicleNextSeat {
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::VehicleNextSeat {
                        session_epoch,
                        world_generation,
                    });
                }
                C::VehicleSwitchSeat {
                    vehicle,
                    seat,
                    session_epoch,
                    world_generation,
                } if session_epoch == world.session_epoch
                    && world_generation == world.world_generation =>
                {
                    let _ = net.0.send(ClientCommand::VehicleSwitchSeat {
                        vehicle,
                        seat,
                        session_epoch,
                        world_generation,
                    });
                }
                _ => {}
            }
        }
    });
}
