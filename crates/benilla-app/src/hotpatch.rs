//! Application configuration and the optional player's diagnostic marker; adapter is reusable.
use benilla_hotpatch_bevy::{HotpatchPlugin, HotpatchProbeTarget};
use benilla_world::rig_anim::RigPose;
use bevy::prelude::*;

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
    use benilla_hotpatch_bevy::HotpatchEvent;
    use crate::quest_trace::{HotpatchReason, PatchEvent, Stage};
    use benilla_hotpatch_bevy::HotpatchReason as BevyReason;
    for event in events.read() {
        if let Some(trace) = &trace {
            let event = match event {
                HotpatchEvent::CandidateStaged { attempt } => {
                    PatchEvent::CandidateStaged { attempt }
                }
                HotpatchEvent::PublicationRejected { attempt, reason } => {
                    PatchEvent::PublicationRejected {
                        attempt,
                        reason: match reason {
                            BevyReason::Publication => HotpatchReason::Publication,
                            BevyReason::Evaluation => HotpatchReason::Evaluation,
                            BevyReason::Retention => HotpatchReason::Retention,
                            BevyReason::Frame => HotpatchReason::Frame,
                            BevyReason::Unknown => HotpatchReason::Unknown,
                        },
                    }
                }
                HotpatchEvent::CandidateRejected { attempt, reason } => {
                    PatchEvent::CandidateRejected {
                        attempt,
                        reason: match reason {
                            BevyReason::Publication => HotpatchReason::Publication,
                            BevyReason::Evaluation => HotpatchReason::Evaluation,
                            BevyReason::Retention => HotpatchReason::Retention,
                            BevyReason::Frame => HotpatchReason::Frame,
                            BevyReason::Unknown => HotpatchReason::Unknown,
                        },
                    }
                }
                HotpatchEvent::Activated { attempt, digest } => {
                    PatchEvent::Activated { attempt, digest }
                }
                HotpatchEvent::FrameFailed { attempt, reason } => {
                    PatchEvent::FrameFailed {
                        attempt,
                        reason: match reason {
                            BevyReason::Publication => HotpatchReason::Publication,
                            BevyReason::Evaluation => HotpatchReason::Evaluation,
                            BevyReason::Retention => HotpatchReason::Retention,
                            BevyReason::Frame => HotpatchReason::Frame,
                            BevyReason::Unknown => HotpatchReason::Unknown,
                        },
                    }
                }
                HotpatchEvent::RetentionFailed { attempt, reason } => {
                    PatchEvent::RetentionFailed {
                        attempt,
                        reason: match reason {
                            BevyReason::Publication => HotpatchReason::Publication,
                            BevyReason::Evaluation => HotpatchReason::Evaluation,
                            BevyReason::Retention => HotpatchReason::Retention,
                            BevyReason::Frame => HotpatchReason::Frame,
                            BevyReason::Unknown => HotpatchReason::Unknown,
                        },
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
