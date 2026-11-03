//! Sample after pose edits, compose the patched frame, then restore the next frame's input.
use benilla_hotpatch::Host;
use benilla_hotpatch_abi::{BoneLocal, RigSpan};
use benilla_world::rig_anim::{AnimParked, PoseModulePass, RigPose};
use bevy::prelude::*;
use std::path::PathBuf;

/// Optional diagnostic target. It does not grant a module access to gameplay state.
#[derive(Component)]
pub struct HotpatchProbeTarget;

pub type PatchAttemptId = u64;

/// Coarse hotpatch failure reason. Free-form loader errors remain in the local log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotpatchReason {
    Publication,
    Evaluation,
    Retention,
    Frame,
    Unknown,
}

/// Structured host milestones. Free-form loader errors remain in the local log.
#[derive(Message, Clone, Copy, Debug)]
pub enum HotpatchEvent {
    CandidateStaged { attempt: PatchAttemptId },
    PublicationRejected { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
    CandidateRejected { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
    Activated { attempt: Option<PatchAttemptId>, digest: Option<[u8; 20]> },
    FrameFailed { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
    RetentionFailed { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
}

pub struct HotpatchPlugin {
    root: PathBuf,
    probe: bool,
}

impl HotpatchPlugin {
    pub fn new(root: PathBuf) -> Self {
        Self { root, probe: false }
    }
    pub fn with_probe(mut self, enabled: bool) -> Self {
        self.probe = enabled;
        self
    }
}

impl Plugin for HotpatchPlugin {
    fn build(&self, app: &mut App) {
        info!(
            "hotpatch: watching {} (pose ABI 1; trusted local modules)",
            self.root.display()
        );
        app.add_message::<HotpatchEvent>()
        .insert_resource(Runtime {
            host: Host::new(self.root.clone()),
            next_poll: 0.0,
            last_error: None,
            reported_revision: None,
            reported_digest: None,
            refresh: false,
            restore_pending: false,
            bones: Vec::new(),
            spans: Vec::new(),
            entities: Vec::new(),
            output: Vec::new(),
            probe: self.probe,
            next_probe: 0.0,
            sample: None,
            next_attempt: 1,
            current_attempt: None,
        })
        .add_systems(Update, poll)
        .add_systems(PostUpdate, apply_pose.in_set(PoseModulePass))
        .add_systems(Last, (capture_pose, restore_pose).chain());
    }
}

#[derive(Resource)]
struct Runtime {
    host: Host,
    next_poll: f64,
    last_error: Option<String>,
    reported_revision: Option<String>,
    reported_digest: Option<String>,
    refresh: bool,
    restore_pending: bool,
    bones: Vec<BoneLocal>,
    spans: Vec<RigSpan>,
    entities: Vec<Entity>,
    output: Vec<BoneLocal>,
    probe: bool,
    next_probe: f64,
    sample: Option<(Entity, usize)>,
    next_attempt: u64,
    current_attempt: Option<u64>,
}

fn report(runtime: &mut Runtime, error: String) {
    if runtime.last_error.as_ref() != Some(&error) {
        warn!("hotpatch: {error}");
    }
    runtime.last_error = Some(error);
}

fn poll(
    time: Res<Time>,
    mut runtime: ResMut<Runtime>,
    mut events: MessageWriter<HotpatchEvent>,
) {
    let now = time.elapsed_secs_f64();
    if now < runtime.next_poll {
        return;
    }
    runtime.next_poll = now + 1.0;
    match runtime.host.poll() {
        Ok(true) => {
            runtime.refresh = true;
            runtime.last_error = None;
            let attempt = runtime.next_attempt;
            runtime.next_attempt = runtime.next_attempt.saturating_add(1);
            runtime.current_attempt = Some(attempt);
            events.write(HotpatchEvent::CandidateStaged { attempt });
        }
        Ok(false) => {}
        Err(error) => {
            let message = format!("{error:#}");
            if runtime.last_error.as_ref() != Some(&message) {
                events.write(HotpatchEvent::PublicationRejected {
                    attempt: None,
                    reason: HotpatchReason::Publication,
                });
            }
            report(&mut runtime, message);
        }
    }
    if let Err(error) = runtime.host.collect_retired() {
        let message = format!("artifact retention: {error:#}");
        if runtime.last_error.as_ref() != Some(&message) {
            events.write(HotpatchEvent::RetentionFailed {
                attempt: runtime.current_attempt,
                reason: HotpatchReason::Retention,
            });
        }
        report(&mut runtime, message);
    }
}

fn apply_pose(
    mut runtime: ResMut<Runtime>,
    time: Res<Time>,
    roots_changed: Query<(), Or<(Changed<Transform>, Changed<GlobalTransform>)>>,
    mut rigs: Query<(Entity, &mut RigPose, Has<HotpatchProbeTarget>), Without<AnimParked>>,
    mut events: MessageWriter<HotpatchEvent>,
) {
    if !runtime.host.has_module() {
        return;
    }
    let runtime = &mut *runtime;
    runtime.bones.clear();
    runtime.spans.clear();
    runtime.entities.clear();
    runtime.sample = None;
    for (entity, mut rig, selected) in &mut rigs {
        if !rig.pose_dirty
            && !runtime.refresh
            && !rig.has_camera_billboards()
            && !roots_changed.contains(rig.joints_root)
        {
            continue;
        }
        // Use the same real bind-local sanitation as composition before giving a module input.
        rig.sanitize_locals();
        let first = runtime.bones.len();
        if first.saturating_add(rig.locals.len()) > u32::MAX as usize {
            report(runtime, "pose batch exceeds ABI capacity".into());
            return;
        }
        if rig.parents.len() != rig.locals.len() {
            report(
                runtime,
                format!("rig {entity:?}: parent/local lengths differ"),
            );
            return;
        }
        runtime
            .bones
            .extend(
                rig.locals
                    .iter()
                    .zip(&rig.parents)
                    .map(|(local, parent)| BoneLocal {
                        translation: local.translation.to_array(),
                        rotation: local.rotation.to_array(),
                        scale: local.scale.to_array(),
                        parent: i32::from(*parent),
                    }),
            );
        runtime.spans.push(RigSpan {
            entity: entity.to_bits(),
            first: first as u32,
            count: rig.locals.len() as u32,
        });
        runtime.entities.push(entity);
        if selected {
            runtime.sample = Some((entity, first));
        }
    }
    let result = runtime.host.evaluate(
        &runtime.bones,
        &runtime.spans,
        time.elapsed_secs_f64(),
        &mut runtime.output,
    );
    if let Some(error) = runtime.host.take_rejection() {
        events.write(HotpatchEvent::CandidateRejected {
            attempt: runtime.current_attempt,
            reason: HotpatchReason::Evaluation,
        });
        report(runtime, error);
    }
    match result {
        Ok(true) => {
            for (span, entity) in runtime.spans.iter().zip(&runtime.entities) {
                let Ok((_, mut rig, _)) = rigs.get_mut(*entity) else {
                    continue;
                };
                for (local, bone) in rig
                    .locals
                    .iter_mut()
                    .zip(&runtime.output[span.first as usize..(span.first + span.count) as usize])
                {
                    *local = transform(bone);
                }
                rig.pose_dirty = true;
            }
            runtime.restore_pending = true;
            runtime.refresh = false;
            let revision = runtime.host.revision().map(str::to_owned);
            let active_digest = runtime.host.digest().map(str::to_owned);
            if revision != runtime.reported_revision || active_digest != runtime.reported_digest {
                let digest = active_digest.as_deref().and_then(|hex| {
                    if hex.len() != 40 {
                        return None;
                    }
                    let mut bytes = [0; 20];
                    for (output, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
                        let high = (pair[0] as char).to_digit(16)?;
                        let low = (pair[1] as char).to_digit(16)?;
                        *output = ((high << 4) | low) as u8;
                    }
                    Some(bytes)
                });
                events.write(HotpatchEvent::Activated {
                    attempt: runtime.current_attempt,
                    digest,
                });
                info!(
                    "hotpatch: pose module activated {} rigs={} bones={}",
                    revision.as_deref().unwrap_or("none"),
                    runtime.spans.len(),
                    runtime.bones.len()
                );
                runtime.reported_revision = revision;
                runtime.reported_digest = active_digest;
                runtime.last_error = None;
            }
        }
        Ok(false) => {}
        Err(error) => {
            runtime.refresh = false;
            let message = format!("pose frame unchanged: {error:#}");
            if runtime.last_error.as_ref() != Some(&message) {
                events.write(HotpatchEvent::FrameFailed {
                    attempt: runtime.current_attempt,
                    reason: HotpatchReason::Frame,
                });
            }
            report(runtime, message);
        }
    }
}

fn transform(bone: &BoneLocal) -> Transform {
    Transform {
        translation: Vec3::from_array(bone.translation),
        rotation: Quat::from_array(bone.rotation),
        scale: Vec3::from_array(bone.scale),
    }
}

/// Composition, anchor propagation and palettes ran in PostUpdate. Their results stay; locals
/// return to their post-edit input so neither unkeyed channels nor saved edits retain a patch.
fn restore_pose(mut runtime: ResMut<Runtime>, mut rigs: Query<&mut RigPose>) {
    if !runtime.restore_pending {
        return;
    }
    for (span, entity) in runtime.spans.iter().zip(&runtime.entities) {
        let Ok(mut rig) = rigs.get_mut(*entity) else {
            continue;
        };
        if rig.locals.len() != span.count as usize {
            continue;
        }
        for (local, bone) in rig
            .locals
            .iter_mut()
            .zip(&runtime.bones[span.first as usize..(span.first + span.count) as usize])
        {
            *local = transform(bone);
        }
    }
    runtime.restore_pending = false;
}

fn capture_pose(time: Res<Time>, mut runtime: ResMut<Runtime>, rigs: Query<&RigPose>) {
    if !runtime.probe || !runtime.restore_pending || time.elapsed_secs_f64() < runtime.next_probe {
        return;
    }
    runtime.next_probe = time.elapsed_secs_f64() + 1.0;
    let Some((entity, first)) = runtime.sample else {
        return;
    };
    let Ok(rig) = rigs.get(entity) else {
        return;
    };
    if let (Some(input), Some(output), Some(model)) = (
        runtime.bones.get(first),
        runtime.output.get(first),
        rig.model.first(),
    ) {
        info!(
            "hotpatch-probe: target rig={entity:?} input={:?} output={:?} composed={:?}",
            input.translation, output.translation, model.translation
        );
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    #[ignore = "Requires two previously built native module publications; see ADR-0002"]
    fn native_reload_preserves_unkeyed_input_and_restores_the_original_pose() {
        let source =
            PathBuf::from(std::env::var_os("HOTPATCH_TEST_ROOT").expect("set HOTPATCH_TEST_ROOT"));
        let state = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension().is_some_and(|s| s == "dll") {
                std::fs::copy(entry.path(), state.path().join(entry.file_name())).unwrap();
            }
        }
        std::fs::copy(source.join("lift.toml"), state.path().join("ready.toml")).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            benilla_world::rig_anim::plugin,
        ));
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs(1),
        ));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(2));
        app.add_plugins(HotpatchPlugin::new(state.path().to_path_buf()));
        let root = app.world_mut().spawn(Transform::default()).id();
        let skeleton = benilla_assets::ModelSkeleton {
            joints: vec![benilla_assets::ModelJoint {
                parent: -1,
                local_translation: Vec3::new(0.0, 2.0, 0.0),
                billboard: None,
                parent_arm: None,
            }],
            ..Default::default()
        };
        app.world_mut()
            .entity_mut(root)
            .insert(RigPose::new(root, &skeleton));
        // An unkeyed channel keeps its local value while the rest of the rig advances.
        app.add_systems(
            PostUpdate,
            (|mut rigs: Query<&mut RigPose>| {
                for mut rig in &mut rigs {
                    rig.pose_dirty = true;
                }
            })
            .in_set(bevy::app::AnimationSystems),
        );
        for _ in 0..3 {
            app.update();
            let rig = app.world().get::<RigPose>(root).unwrap();
            assert!(
                (rig.model[0].translation.y - 2.5).abs() < 1e-5,
                "composed output must not accumulate"
            );
            assert!(
                (rig.locals[0].translation.y - 2.0).abs() < 1e-5,
                "next-frame input must stay unpatched"
            );
        }
        // Publish bytes before the manifest, just like the real publisher. The host correctly
        // collected the not-yet-published normal DLL during the first three frames.
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            let destination = state.path().join(entry.file_name());
            if entry.path().extension().is_some_and(|s| s == "dll") && !destination.exists() {
                std::fs::copy(entry.path(), destination).unwrap();
            }
        }
        std::fs::copy(source.join("normal.toml"), state.path().join("ready.toml")).unwrap();
        app.update();
        let rig = app.world().get::<RigPose>(root).unwrap();
        assert!(
            (rig.model[0].translation.y - 2.0).abs() < 1e-5,
            "replacement must remove the previous visual change"
        );
        assert!((rig.locals[0].translation.y - 2.0).abs() < 1e-5);
    }
}
