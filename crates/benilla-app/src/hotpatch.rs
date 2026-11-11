//! Application configuration, the movement-controller hotpatch bridge, and the optional player
//! diagnostic marker; adapter is reusable.
use benilla_hotpatch_bevy::{
    HotpatchEvent, HotpatchPlugin, HotpatchProbeTarget, HotpatchReason as BevyReason,
    MovementBounds, MovementFrame, MovementProposalSource, MovementRefusal, PatchEvaluationReason,
    PatchRejectReason, Runtime as HotpatchRuntime,
};
use benilla_world::rig_anim::RigPose;
use benilla_world::schedule::WorldStage;
use bevy::prelude::*;

use crate::player::{
    CameraControl, MoveSpeed, MovementProcessor, MovementProcessorOutcome,
    MovementProcessorRefusal, MovementProcessorState, MovementProposal as AppMovementProposal,
    Player, PlayerControlSet, MODULE_OWNED_FLAGS, MODULE_SPEED_SLACK, MODULE_STEP_FLOOR,
    MODULE_STEP_SLACK, TERMINAL_VELOCITY,
};
use crate::quest_trace::{HotpatchReason, PatchEvent, Stage};

/// Map the adapter's coarse reason into the quest-trace reason space.
fn trace_coarse_reason(reason: BevyReason) -> HotpatchReason {
    match reason {
        BevyReason::Publication => HotpatchReason::Publication,
        BevyReason::Evaluation => HotpatchReason::Evaluation,
        BevyReason::Retention => HotpatchReason::Retention,
        BevyReason::Frame => HotpatchReason::Frame,
        BevyReason::Unknown => HotpatchReason::Unknown,
    }
}

/// Map a fine evaluation detail into the quest-trace reason space.
///
/// `Unknown` and any future `#[non_exhaustive]` variant fall back to the event's coarse reason, so
/// the recorder still shows `Evaluation`/`Frame` rather than inventing a finer code.
fn trace_evaluation_reason(detail: PatchEvaluationReason, coarse: BevyReason) -> HotpatchReason {
    match detail {
        PatchEvaluationReason::Status(status) => HotpatchReason::EvaluationStatus(status),
        PatchEvaluationReason::InvalidOutput => HotpatchReason::EvaluationInvalidOutput,
        PatchEvaluationReason::TopologyChanged => HotpatchReason::EvaluationTopologyChanged,
        PatchEvaluationReason::FrameTooLarge => HotpatchReason::EvaluationFrameTooLarge,
        PatchEvaluationReason::HostFrameRejected => HotpatchReason::EvaluationHostFrame,
        PatchEvaluationReason::Unknown => trace_coarse_reason(coarse),
        _ => trace_coarse_reason(coarse),
    }
}

pub(crate) fn plugin(app: &mut App) {
    let Some(home) = crate::local_state::home() else {
        return;
    };
    let probe = std::env::var("WOW_HOTPATCH_PROBE").as_deref() == Ok("1");
    let movement = std::env::var("WOW_HOTPATCH_MOVEMENT")
        .map(|v| v != "0")
        .unwrap_or(true);
    app.init_resource::<MovementProcessor>()
        .add_plugins(
            HotpatchPlugin::new(home.join("Hotpatch"))
                .with_probe(probe)
                .with_movement(movement),
        )
        .add_systems(
            Update,
            prepare_movement_hotpatch
                .in_set(WorldStage::Input)
                .before(PlayerControlSet)
                .run_if(not(resource_exists::<crate::run_mode::CaptureMode>))
                .in_set(crate::client_state::InWorldGated),
        )
        .add_systems(
            Update,
            settle_movement_hotpatch
                .in_set(WorldStage::Input)
                .after(PlayerControlSet)
                .run_if(not(resource_exists::<crate::run_mode::CaptureMode>))
                .in_set(crate::client_state::InWorldGated),
        )
        .add_systems(Last, capture_hotpatch_events);
    if probe {
        app.add_systems(Update, mark_player_rig);
    }
}

/// Build the ABI frame and host bounds from the current player/camera state and evaluate any
/// staged or active movement processor ahead of [`crate::player::PlayerControlSet`].
fn prepare_movement_hotpatch(
    time: Res<Time>,
    mut runtime: ResMut<HotpatchRuntime>,
    mut processor: ResMut<MovementProcessor>,
    player: Option<Res<Player>>,
    move_speed: Option<Res<MoveSpeed>>,
    rig: Option<Res<CameraControl>>,
) {
    let Some(player) = player else {
        return;
    };
    let dt = time.delta_secs();
    let run_speed = move_speed
        .as_ref()
        .map(|s| s.0)
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(7.0);
    let walk_speed = (run_speed * (2.5 / 7.0)).max(0.1);
    let swim_speed = (run_speed * (4.722222 / 7.0)).max(0.1);
    let velocity = [player.horiz_vel.x, player.vel_y, player.horiz_vel.z];
    let horiz_speed = player.horiz_vel.length();
    let body_speed = horiz_speed.max(run_speed);
    let liquid_height = player
        .liquid_surface
        .filter(|y| y.is_finite())
        .unwrap_or(f32::NEG_INFINITY);
    let camera_yaw = rig
        .as_ref()
        .map(|r| r.yaw)
        .filter(|y| y.is_finite())
        .unwrap_or(player.face_yaw);
    let frame = MovementFrame {
        dt_seconds: dt,
        position: player.pos.to_array(),
        velocity,
        facing: player.face_yaw,
        pitch: player.mover_pitch,
        camera_yaw,
        input_buttons: 0,
        move_flags: player.move_flags,
        run_speed,
        walk_speed,
        swim_speed,
        ground_height: player.pos.y,
        ground_normal: [0.0, 1.0, 0.0],
        liquid_height,
        stamina_norm: 1.0,
    };
    let bounds = MovementBounds {
        max_step: (body_speed * dt.max(0.0) * MODULE_STEP_SLACK + MODULE_STEP_FLOOR).max(0.05),
        max_horizontal_speed: (body_speed * MODULE_SPEED_SLACK + MODULE_STEP_FLOOR).max(0.05),
        max_vertical_speed: (TERMINAL_VELOCITY.max(player.vel_y.abs()) * MODULE_SPEED_SLACK)
            .max(1.0),
        max_abs_facing: std::f32::consts::TAU
            .max(player.face_yaw.abs() + std::f32::consts::TAU),
        permitted_flags: MODULE_OWNED_FLAGS,
    };
    drive_movement_proposal(&mut runtime, &mut processor, &frame, &bounds);
}

/// Settle the controller's movement verdict on the hotpatch host immediately after
/// [`crate::player::PlayerControlSet`] has consumed the frame's proposal.
fn settle_movement_hotpatch(
    mut runtime: ResMut<HotpatchRuntime>,
    mut processor: ResMut<MovementProcessor>,
) {
    settle_movement_outcome(&mut runtime, &mut processor);
}

/// Bridge one frame's movement staging state and proposal from [`HotpatchRuntime`] onto the
/// player controller's [`MovementProcessor`].
pub(crate) fn drive_movement_proposal(
    runtime: &mut HotpatchRuntime,
    processor: &mut MovementProcessor,
    frame: &MovementFrame,
    bounds: &MovementBounds,
) {
    if !runtime.is_movement_seam_enabled() {
        let _ = runtime.take_movement_stage_outcome();
        if processor.state() != MovementProcessorState::Disabled {
            processor.disable();
        }
        return;
    }
    processor.enable();
    match runtime.take_movement_stage_outcome() {
        Some(Ok(true)) => {
            if runtime.movement_processor_active() {
                processor.stage_over_active();
            } else {
                processor.stage();
            }
        }
        Some(Ok(false)) => {}
        Some(Err(refusal)) => {
            if refusal == MovementRefusal::SeamDisabled {
                processor.disable();
                return;
            } else if runtime.movement_processor_active() {
                processor.retain_active_on_refusal(MovementProcessorRefusal::Host);
            } else {
                processor.unload();
                processor.refuse(MovementProcessorRefusal::Host);
                return;
            }
        }
        None => {}
    }
    if runtime.movement_processor_staged()
        && !matches!(processor.state(), MovementProcessorState::Staged)
    {
        if runtime.movement_processor_active() {
            processor.stage_over_active();
        } else {
            processor.stage();
        }
    } else if !runtime.has_movement_processor() {
        if matches!(
            processor.state(),
            MovementProcessorState::Staged | MovementProcessorState::Active
        ) {
            processor.unload();
        }
        return;
    }
    match runtime.propose_movement(frame, bounds) {
        Ok((proposal, source)) => {
            match source {
                MovementProposalSource::Staged => {
                    if !matches!(processor.state(), MovementProcessorState::Staged) {
                        if runtime.movement_processor_active() {
                            processor.stage_over_active();
                        } else {
                            processor.stage();
                        }
                    }
                }
                MovementProposalSource::Active => {
                    processor.mark_active();
                }
            }
            processor.offer(AppMovementProposal {
                position: Vec3::from_array(proposal.position),
                velocity: Vec3::from_array(proposal.velocity),
                facing: proposal.facing,
                pitch: proposal.pitch,
                move_flags: proposal.move_flags,
            });
        }
        Err(MovementRefusal::NoModule) => {
            processor.unload();
        }
        Err(MovementRefusal::SeamDisabled) => {
            processor.disable();
        }
        Err(MovementRefusal::FrameInput) => {
            processor.handback_frame();
        }
        Err(MovementRefusal::Tolerated) => {
            processor.tolerate(MovementProcessorRefusal::Host);
        }
        Err(_refusal) => {
            if runtime.movement_processor_active() {
                processor.retain_active_on_refusal(MovementProcessorRefusal::Host);
            } else {
                processor.unload();
                processor.refuse(MovementProcessorRefusal::Host);
            }
        }
    }
}

/// Settle the controller's post-frame [`MovementProcessorOutcome`] onto [`HotpatchRuntime`]:
/// promote on `Promoted`, withdraw commit readiness on `StagedRejected` or `Drained`, and unload
/// on `ActiveRejected`.
pub(crate) fn settle_movement_outcome(
    runtime: &mut HotpatchRuntime,
    processor: &mut MovementProcessor,
) {
    match processor.take_outcome() {
        MovementProcessorOutcome::None | MovementProcessorOutcome::ActiveAccepted => {}
        MovementProcessorOutcome::Drained | MovementProcessorOutcome::StagedRejected(_) => {
            let _ = runtime.withdraw_movement_proposal();
        }
        MovementProcessorOutcome::Promoted => {
            let _ = runtime.commit_movement_processor();
        }
        MovementProcessorOutcome::ActiveRejected(_) => {
            let _ = runtime.unload_movement_processor();
        }
    }
}

fn capture_hotpatch_events(
    mut events: MessageReader<benilla_hotpatch_bevy::HotpatchEvent>,
    trace: Option<Res<crate::quest_trace::Source>>,
    view: Option<Res<crate::wrath::WrathWorldView>>,
) {
    for event in events.read() {
        if let Some(trace) = &trace {
            let event = match *event {
                HotpatchEvent::CandidateStaged { attempt } => {
                    PatchEvent::CandidateStaged { attempt }
                }
                HotpatchEvent::CandidateWithdrawn { attempt } => {
                    PatchEvent::CandidateWithdrawn { attempt }
                }
                HotpatchEvent::PublicationRejected {
                    attempt,
                    reason,
                    detail,
                } => {
                    PatchEvent::PublicationRejected {
                        attempt,
                        reason: match detail {
                            PatchRejectReason::TransientIoPending => {
                                HotpatchReason::TransientIoPending
                            }
                            PatchRejectReason::ManifestUnreadable => {
                                HotpatchReason::ManifestUnreadable
                            }
                            PatchRejectReason::ManifestMalformed => {
                                HotpatchReason::ManifestMalformed
                            }
                            PatchRejectReason::InvalidBasename => {
                                HotpatchReason::InvalidBasename
                            }
                            PatchRejectReason::EmptyRevision => HotpatchReason::EmptyRevision,
                            PatchRejectReason::ArtifactTooLarge => {
                                HotpatchReason::ArtifactTooLarge
                            }
                            PatchRejectReason::DigestMismatch => HotpatchReason::DigestMismatch,
                            PatchRejectReason::ImmutableFilename => {
                                HotpatchReason::ImmutableFilename
                            }
                            PatchRejectReason::ArtifactRejected => {
                                HotpatchReason::ArtifactRejected
                            }
                            PatchRejectReason::Quarantined => HotpatchReason::Quarantined,
                            PatchRejectReason::ModuleLoadFailed => {
                                HotpatchReason::ModuleLoadFailed
                            }
                            PatchRejectReason::MissingEntryPoint => {
                                HotpatchReason::MissingEntryPoint
                            }
                            PatchRejectReason::AbiMismatch => HotpatchReason::AbiMismatch,
                            PatchRejectReason::MissingProcessor => {
                                HotpatchReason::MissingProcessor
                            }
                            PatchRejectReason::LoadFailed => HotpatchReason::LoadFailed,
                            _ => trace_coarse_reason(reason),
                        },
                    }
                }
                HotpatchEvent::CandidateRejected {
                    attempt,
                    reason,
                    detail,
                } => PatchEvent::CandidateRejected {
                    attempt,
                    reason: trace_evaluation_reason(detail, reason),
                },
                HotpatchEvent::Activated { attempt, digest } => {
                    PatchEvent::Activated { attempt, digest }
                }
                HotpatchEvent::FrameFailed {
                    attempt,
                    reason,
                    detail,
                } => PatchEvent::FrameFailed {
                    attempt,
                    reason: trace_evaluation_reason(detail, reason),
                },
                HotpatchEvent::RestoredPrevious {
                    failed_attempt,
                    restored_attempt,
                    digest,
                } => PatchEvent::RestoredPrevious {
                    failed_attempt,
                    restored_attempt,
                    digest,
                },
                HotpatchEvent::RestoreUnavailable {
                    failed_attempt,
                    detail,
                } => PatchEvent::RestoreUnavailable {
                    failed_attempt,
                    reason: trace_evaluation_reason(detail, BevyReason::Frame),
                },
                HotpatchEvent::RetentionFailed { attempt, reason } => {
                    PatchEvent::RetentionFailed {
                        attempt,
                        reason: trace_coarse_reason(reason),
                    }
                }
            };
            trace.observe(view.as_ref().map(|v| (v.session_epoch, v.world_generation)), None,
                Stage::Hotpatch { event });
        }
    }
}

fn mark_player_rig(
    mut commands: Commands,
    rigs: Query<Entity, (With<RigPose>, Without<HotpatchProbeTarget>)>,
    parents: Query<&ChildOf>,
    players: Query<(), With<crate::net::SelfPlayer>>,
) {
    for entity in &rigs {
        let mut ancestor = entity;
        for _ in 0..32 {
            if players.contains(ancestor) {
                commands.entity(entity).insert(HotpatchProbeTarget);
                break;
            }
            let Ok(parent) = parents.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        drive_movement_proposal, settle_movement_outcome, trace_coarse_reason,
        trace_evaluation_reason, AppMovementProposal, BevyReason, HotpatchReason, HotpatchRuntime,
        MovementBounds, MovementFrame, MovementProcessor, MovementProcessorOutcome,
        MovementProcessorRefusal, MovementProcessorState, MODULE_OWNED_FLAGS,
    };
    use crate::player::{MovementBaseline, MovementProcessorTick};
    use benilla_hotpatch_bevy::PatchEvaluationReason;
    use bevy::math::Vec3;

    fn sample_frame_and_bounds() -> (MovementFrame, MovementBounds) {
        let frame = MovementFrame {
            dt_seconds: 0.02,
            position: [1.0, 2.0, 3.0],
            velocity: [0.0, 0.0, 7.0],
            facing: 0.0,
            pitch: 0.0,
            camera_yaw: 0.0,
            input_buttons: 0,
            move_flags: 0,
            run_speed: 7.0,
            walk_speed: 2.5,
            swim_speed: 4.722222,
            ground_height: 2.0,
            ground_normal: [0.0, 1.0, 0.0],
            liquid_height: f32::NEG_INFINITY,
            stamina_norm: 1.0,
        };
        let bounds = MovementBounds {
            max_step: 0.35,
            max_horizontal_speed: 12.0,
            max_vertical_speed: 90.0,
            max_abs_facing: std::f32::consts::TAU,
            permitted_flags: MODULE_OWNED_FLAGS,
        };
        (frame, bounds)
    }

    #[test]
    fn every_evaluation_detail_maps_to_a_fine_trace_reason() {
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::Status(7), BevyReason::Evaluation),
            HotpatchReason::EvaluationStatus(7)
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::Status(-3), BevyReason::Frame),
            HotpatchReason::EvaluationStatus(-3)
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::InvalidOutput, BevyReason::Evaluation),
            HotpatchReason::EvaluationInvalidOutput
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::TopologyChanged, BevyReason::Frame),
            HotpatchReason::EvaluationTopologyChanged
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::FrameTooLarge, BevyReason::Evaluation),
            HotpatchReason::EvaluationFrameTooLarge
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::HostFrameRejected, BevyReason::Frame),
            HotpatchReason::EvaluationHostFrame
        );
    }

    #[test]
    fn unknown_evaluation_detail_falls_back_to_the_coarse_reason() {
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::Unknown, BevyReason::Evaluation),
            HotpatchReason::Evaluation
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::Unknown, BevyReason::Frame),
            HotpatchReason::Frame
        );
        assert_eq!(
            trace_evaluation_reason(PatchEvaluationReason::Unknown, BevyReason::Publication),
            HotpatchReason::Publication
        );
    }

    #[test]
    fn coarse_reasons_keep_their_trace_labels() {
        assert_eq!(
            trace_coarse_reason(BevyReason::Retention),
            HotpatchReason::Retention
        );
        assert_eq!(trace_coarse_reason(BevyReason::Unknown), HotpatchReason::Unknown);
    }

    #[test]
    fn movement_adapter_tracks_seam_toggle_and_stage_failures_cleanly() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut runtime = HotpatchRuntime::new(dir.path().to_path_buf());
        let mut processor = MovementProcessor::default();
        let (frame, bounds) = sample_frame_and_bounds();

        // Seam starts disabled: processor stays Disabled.
        drive_movement_proposal(&mut runtime, &mut processor, &frame, &bounds);
        assert_eq!(processor.state(), MovementProcessorState::Disabled);

        // Enabling the seam with no manifest moves the processor to Baseline.
        runtime.set_movement_seam_enabled(true);
        drive_movement_proposal(&mut runtime, &mut processor, &frame, &bounds);
        assert_eq!(processor.state(), MovementProcessorState::Baseline);

        // A malformed `movement.toml` refuses staging on the host and marks the processor
        // Refused(Host).
        std::fs::write(dir.path().join("movement.toml"), "not = [valid toml").unwrap();
        let _ = runtime.stage_movement_processor();
        drive_movement_proposal(&mut runtime, &mut processor, &frame, &bounds);
        assert_eq!(
            processor.state(),
            MovementProcessorState::Refused(MovementProcessorRefusal::Host)
        );

        // Disabling the seam returns the processor to Disabled immediately.
        runtime.set_movement_seam_enabled(false);
        drive_movement_proposal(&mut runtime, &mut processor, &frame, &bounds);
        assert_eq!(processor.state(), MovementProcessorState::Disabled);
    }

    #[test]
    fn movement_adapter_settles_promotion_and_withdrawal_outcomes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut runtime = HotpatchRuntime::new(dir.path().to_path_buf());
        runtime.set_movement_seam_enabled(true);
        let mut processor = MovementProcessor::default();
        processor.enable();
        assert!(processor.stage());

        let baseline = MovementBaseline {
            position: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.0, 0.0, 7.0),
            facing: 0.0,
            speed: 7.0,
            dt_seconds: 0.02,
            turn_rate: std::f32::consts::PI,
            may_translate: true,
            may_turn: true,
        };
        processor.offer(AppMovementProposal {
            position: Vec3::new(1.0, 2.0, 3.1),
            velocity: Vec3::new(0.0, 0.0, 7.0),
            facing: 0.0,
            pitch: 0.0,
            move_flags: 0,
        });
        assert!(matches!(
            processor.take_tick(&baseline),
            MovementProcessorTick::Corrected(_)
        ));
        assert_eq!(processor.state(), MovementProcessorState::Active);
        settle_movement_outcome(&mut runtime, &mut processor);
        assert_eq!(processor.take_outcome(), MovementProcessorOutcome::None);
    }
}
