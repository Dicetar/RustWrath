//! Application configuration and the optional player's diagnostic marker; adapter is reusable.
use benilla_hotpatch_bevy::{
    HotpatchEvent, HotpatchPlugin, HotpatchProbeTarget, HotpatchReason as BevyReason,
    PatchEvaluationReason, PatchRejectReason,
};
use benilla_world::rig_anim::RigPose;
use bevy::prelude::*;

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
    app.add_plugins(HotpatchPlugin::new(home.join("Hotpatch")).with_probe(probe))
        .add_systems(Last, capture_hotpatch_events);
    if probe {
        app.add_systems(Update, mark_player_rig);
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
    use super::{trace_coarse_reason, trace_evaluation_reason, BevyReason, HotpatchReason};
    use benilla_hotpatch_bevy::PatchEvaluationReason;

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
}
