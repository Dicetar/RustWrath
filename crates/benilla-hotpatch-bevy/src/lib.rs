//! Sample after pose edits, compose the patched frame, then restore the next frame's input.
use benilla_hotpatch::{Host, PatchRejectReason, RestoreOutcome};
use benilla_hotpatch_abi::{BoneLocal, RigSpan};
use benilla_world::rig_anim::{AnimParked, PoseModulePass, RigPose};
use bevy::prelude::*;
use std::{collections::VecDeque, path::PathBuf};

/// Optional diagnostic target. It does not grant a module access to gameplay state.
#[derive(Component)]
pub struct HotpatchProbeTarget;

pub type PatchAttemptId = u64;

/// Activation history bound. The host retains the active module plus three predecessors, so a
/// small ring is enough to map a restored digest back to its original attempt id.
const MAX_ACTIVATION_HISTORY: usize = 8;

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
    CandidateWithdrawn { attempt: PatchAttemptId },
    PublicationRejected {
        attempt: Option<PatchAttemptId>,
        reason: HotpatchReason,
        detail: PatchRejectReason,
    },
    CandidateRejected { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
    Activated { attempt: Option<PatchAttemptId>, digest: Option<[u8; 20]> },
    FrameFailed { attempt: Option<PatchAttemptId>, reason: HotpatchReason },
    /// A failed active module was replaced by a retained predecessor that is now active again.
    ///
    /// `restored_attempt` is the predecessor's original activation attempt when it is still
    /// retained by the adapter; `None` means unknown, never a fabricated id. `digest` is the
    /// restored module's verified content identity, or `None` when unavailable.
    RestoredPrevious {
        failed_attempt: Option<PatchAttemptId>,
        restored_attempt: Option<PatchAttemptId>,
        digest: Option<[u8; 20]>,
    },
    /// The failed active module had no retained predecessor; the baseline pose is active again.
    RestoreUnavailable { failed_attempt: Option<PatchAttemptId> },
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
        let mut host = Host::new(self.root.clone());
        // Restore the digest quarantines from the previous run so a `B -> C -> B` manifest rewrite
        // cannot re-stage a digest that already failed deterministically. A malformed or unreadable
        // store is logged once and must not fail plugin startup: the in-memory stores stay empty
        // and a later quarantine rewrites the file from memory. The message seeds the quarantine
        // dedupe slot, not the publication slot.
        let mut last_quarantine_error = None;
        if let Err(error) = host.load_persistent_quarantine() {
            let message = format!("quarantine load: {error:#}");
            warn!("hotpatch: {message}");
            last_quarantine_error = Some(message);
        }
        // Pose/movement crash sentinel: a marker written after the artifact bytes were verified and
        // before the irreversible `Library::new` (loading windows) or immediately before the staged
        // pose candidate's canary `process()` call may have survived a native fault. Recover it
        // before the first poll: quarantine the crashed pose or movement digest (and persist the
        // store so it survives another crash) or fail safe on a malformed marker. Startup never
        // fails on either outcome.
        let mut last_sentinel_error = None;
        match host.load_active_sentinel() {
            Ok(Some(digest)) => {
                warn!(
                    "hotpatch: recovered crashed module {digest}; quarantining before next launch"
                );
                // Commit the recovered digest before removing the marker. The marker is the only
                // durable record between the crash and the quarantine rename; a crash or persist
                // failure here must leave it for the next launch to retry.
                match host.persist_quarantine() {
                    Ok(()) => {
                        // `persist_quarantine` clears the marker only after committing the
                        // recovered digest; a clear failure is reported through the sentinel slot.
                        if let Some(error) = host.take_sentinel_error() {
                            warn!("hotpatch: {error}");
                            last_sentinel_error = Some(error);
                        }
                    }
                    Err(error) => {
                        let message = format!(
                            "quarantine save after sentinel recovery: {error:#}; marker retained"
                        );
                        warn!("hotpatch: {message}");
                        last_quarantine_error = Some(message);
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                let message = format!("active sentinel load: {error:#}");
                warn!("hotpatch: {message}");
                last_sentinel_error = Some(message);
            }
        }
        app.add_message::<HotpatchEvent>()
        .insert_resource(Runtime {
            host,
            next_poll: 0.0,
            last_error: None,
            last_publication_detail: None,
            last_quarantine_error,
            last_sentinel_error,
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
            candidate_attempt: None,
            active_attempt: None,
            restored_attempt: None,
            activations: VecDeque::new(),
        })
        .add_systems(Update, poll)
        .add_systems(PostUpdate, apply_pose.in_set(PoseModulePass))
        .add_systems(Last, (capture_pose, restore_pose).chain())
        // Declared ahead of the pose chain: all three systems take `ResMut<Runtime>` and this
        // edge's clear must not be skipped because a pose system panicked on the exit frame. An
        // explicit `AppExit` is the clean-exit signal; `Runtime::drop` stays the fallback for a
        // bare `App` drop.
        .add_systems(Last, clear_active_sentinel_on_app_exit.before(capture_pose));
    }
}

#[derive(Resource)]
struct Runtime {
    host: Host,
    next_poll: f64,
    last_error: Option<String>,
    /// Last typed publication reason emitted or observed. The coarse `last_error` message is the
    /// dedupe key for free-form text; this separate slot lets a transient -> terminal transition
    /// with identical error text still emit the new typed detail.
    last_publication_detail: Option<PatchRejectReason>,
    /// Last reported quarantine load/save error. Deliberately separate from `last_error`: the
    /// publication/retention paths share and clear that slot, so reusing it here would let a
    /// persistent save failure re-emit `PublicationRejected`/`RetentionFailed` (and the save
    /// warning) every 1 Hz tick. Only a successful save clears this slot.
    last_quarantine_error: Option<String>,
    /// Last reported pose/movement crash-sentinel load/write/clear error. Deliberately separate
    /// from `last_error` and `last_quarantine_error`: a best-effort sentinel write/clear failure
    /// must not re-emit publication/retention events or mask a successful save, and a sentinel load
    /// failure at startup must not fail the plugin.
    last_sentinel_error: Option<String>,
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
    candidate_attempt: Option<u64>,
    active_attempt: Option<u64>,
    /// Attempt that is becoming active because it was restored, consumed by the next activation.
    restored_attempt: Option<u64>,
    /// Bounded digest→attempt map of observed activations, used only to label restored modules.
    activations: VecDeque<(Option<[u8; 20]>, PatchAttemptId)>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Rust runs `Drop` during panic unwind as well as on an orderly `App` drop. A panic inside
        // a marker-covered call (`Loaded::open`, the canary `evaluate`, movement `process`) must
        // not delete the marker for the operation that just panicked: that marker is what makes
        // the next launch quarantine the crashing digest. Bail out while unwinding; only an
        // ordinary drop is treated as the clean-exit safety net.
        if std::thread::panicking() {
            return;
        }
        // Clean-exit safety net: remove a stale `active-sentinel.toml` so an orderly client exit
        // cannot leave it for the next launch (which would wrongly quarantine a healthy digest).
        // A pending recovery marker is the only durable record of an uncommitted recovered digest
        // and must survive to the next launch, so it is retained here. Best effort: do not
        // intentionally panic or block.
        match self.host.clear_active_sentinel_on_clean_shutdown() {
            Ok(true) => {}
            Ok(false) => {
                warn!(
                    "hotpatch: clean shutdown retained active-sentinel.toml for the next launch"
                );
            }
            Err(error) => {
                warn!("hotpatch: clean shutdown active sentinel clear failed: {error:#}");
            }
        }
    }
}

fn report(runtime: &mut Runtime, error: String) {
    if runtime.last_error.as_ref() != Some(&error) {
        warn!("hotpatch: {error}");
    }
    runtime.last_error = Some(error);
}

/// Report a quarantine load/save error through its own dedupe slot.
///
/// The publication and retention paths share `Runtime.last_error`, which a successful staging poll
/// intentionally clears. Reusing that slot for the quarantine would make a persistent save failure
/// re-emit both events and both warnings on every tick, so this slot is independent and is cleared
/// only when a save succeeds.
fn report_quarantine(runtime: &mut Runtime, error: String) {
    if runtime.last_quarantine_error.as_ref() != Some(&error) {
        warn!("hotpatch: {error}");
    }
    runtime.last_quarantine_error = Some(error);
}

/// Report a pose/movement crash-sentinel error through its own dedupe slot.
///
/// The host sets [`Host::sentinel_error`] only for best-effort sentinel write/clear failures during
/// `poll`, `evaluate` or movement staging (the module load/evaluation is never failed or skipped
/// because the sentinel could not be written). A startup sentinel load failure goes directly to
/// `last_sentinel_error`; it never populates the host slot. This slot is independent of both
/// `last_error` and `last_quarantine_error`, so a sentinel issue neither re-emits
/// publication/retention events nor is masked by a successful quarantine save.
fn report_sentinel(runtime: &mut Runtime, error: String) {
    if runtime.last_sentinel_error.as_ref() != Some(&error) {
        warn!("hotpatch: {error}");
    }
    runtime.last_sentinel_error = Some(error);
}

fn digest_from_hex(hex: &str) -> Option<[u8; 20]> {
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
}

/// Recover the original activation attempt for a restored module. Unknown stays `None` rather
/// than borrowing the failed attempt's id.
fn attempt_for_digest(runtime: &Runtime, digest: Option<[u8; 20]>) -> Option<PatchAttemptId> {
    let digest = digest?;
    runtime
        .activations
        .iter()
        .rev()
        .find_map(|(known, attempt)| (*known == Some(digest)).then_some(*attempt))
}

fn remember_activation(
    runtime: &mut Runtime,
    digest: Option<[u8; 20]>,
    attempt: PatchAttemptId,
) {
    if runtime.activations.len() == MAX_ACTIVATION_HISTORY {
        runtime.activations.pop_front();
    }
    runtime.activations.push_back((digest, attempt));
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
    let result = runtime.host.poll();
    if matches!(&result, Ok(true)) || !runtime.host.has_pending() {
        if let Some(attempt) = runtime.candidate_attempt.take() {
            events.write(HotpatchEvent::CandidateWithdrawn { attempt });
        }
    }
    match result {
        Ok(true) => {
            runtime.refresh = true;
            runtime.last_error = None;
            runtime.last_publication_detail = None;
            let attempt = runtime.next_attempt;
            runtime.next_attempt = runtime.next_attempt.saturating_add(1);
            runtime.candidate_attempt = Some(attempt);
            events.write(HotpatchEvent::CandidateStaged { attempt });
        }
        Ok(false) => {}
        Err(error) => {
            let message = format!("{error:#}");
            let detail = runtime
                .host
                .take_publication_reason()
                .unwrap_or(PatchRejectReason::Unknown);
            let message_changed = runtime.last_error.as_ref() != Some(&message);
            let detail_changed = runtime.last_publication_detail != Some(detail);
            if message_changed || detail_changed {
                events.write(HotpatchEvent::PublicationRejected {
                    attempt: None,
                    reason: HotpatchReason::Publication,
                    detail,
                });
            }
            runtime.last_publication_detail = Some(detail);
            report(&mut runtime, message);
        }
    }
    if let Err(error) = runtime.host.collect_retired() {
        let message = format!("artifact retention: {error:#}");
        if runtime.last_error.as_ref() != Some(&message) {
            events.write(HotpatchEvent::RetentionFailed {
                attempt: None,
                reason: HotpatchReason::Retention,
            });
        }
        report(&mut runtime, message);
    }
    // Save the digest quarantines once per poll tick, after staging and retention have updated
    // them. Writes are digest-only; a failure is deduplicated through the quarantine slot and never
    // stops the poll or the seams.
    match runtime.host.persist_quarantine() {
        Ok(()) => runtime.last_quarantine_error = None,
        Err(error) => report_quarantine(&mut runtime, format!("quarantine save: {error:#}")),
    }
    // Drain the best-effort pose/movement crash-sentinel error slot. A failed write/clear during
    // the host poll, canary evaluation or movement staging lands here; the module load/evaluation
    // itself is never failed or skipped because the sentinel could not be written. Reported through
    // the dedicated slot, not `last_error` or the quarantine slot.
    let sentinel_error = runtime.host.take_sentinel_error();
    if let Some(error) = sentinel_error {
        report_sentinel(&mut runtime, error);
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
        runtime.refresh = false;
        events.write(HotpatchEvent::CandidateRejected {
            attempt: runtime.candidate_attempt,
            reason: HotpatchReason::Evaluation,
        });
        runtime.candidate_attempt = None;
        report(runtime, error);
    }
    // Restore identity is per evaluation; never carry a stale restore into a later activation.
    runtime.restored_attempt = None;
    let restore_outcomes = runtime.host.take_restore_outcomes();
    let active_failed = !restore_outcomes.is_empty();
    // Free-form host errors stay local; only the bounded outcomes below reach the recorder.
    if let Some(error) = runtime.host.take_active_failure() {
        report(runtime, error);
        if !active_failed {
            // A below-threshold failure kept the active module. Force the next frame to
            // repack the rigs so the retained module is evaluated again instead of
            // waiting for an unrelated animation/root change.
            runtime.refresh = true;
        }
    }
    for outcome in restore_outcomes {
        match outcome {
            RestoreOutcome::Restored {
                failed_digest,
                restored_digest,
            } => {
                let failed_attempt = match runtime.active_attempt {
                    Some(attempt) => Some(attempt),
                    None => attempt_for_digest(runtime, failed_digest),
                };
                let restored_attempt = attempt_for_digest(runtime, restored_digest);
                events.write(HotpatchEvent::FrameFailed {
                    attempt: failed_attempt,
                    reason: HotpatchReason::Frame,
                });
                events.write(HotpatchEvent::RestoredPrevious {
                    failed_attempt,
                    restored_attempt,
                    digest: restored_digest,
                });
                runtime.active_attempt = restored_attempt;
                runtime.restored_attempt = restored_attempt;
            }
            RestoreOutcome::Unavailable { failed_digest } => {
                let failed_attempt = match runtime.active_attempt {
                    Some(attempt) => Some(attempt),
                    None => attempt_for_digest(runtime, failed_digest),
                };
                events.write(HotpatchEvent::FrameFailed {
                    attempt: failed_attempt,
                    reason: HotpatchReason::Frame,
                });
                events.write(HotpatchEvent::RestoreUnavailable { failed_attempt });
                runtime.active_attempt = None;
                runtime.restored_attempt = None;
            }
        }
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
            runtime.refresh = active_failed;
            let revision = runtime.host.revision().map(str::to_owned);
            let active_digest = runtime.host.digest().map(str::to_owned);
            let restored_attempt = runtime.restored_attempt.take();
            if revision != runtime.reported_revision || active_digest != runtime.reported_digest {
                let digest = active_digest.as_deref().and_then(digest_from_hex);
                // A restored predecessor activates under its original attempt id; a staged
                // candidate activates under the attempt created when it was published.
                let attempt = match runtime.candidate_attempt {
                    Some(attempt) => Some(attempt),
                    None => restored_attempt,
                };
                events.write(HotpatchEvent::Activated { attempt, digest });
                runtime.active_attempt = attempt;
                runtime.candidate_attempt = None;
                if let Some(attempt) = attempt {
                    remember_activation(runtime, digest, attempt);
                }
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
        Ok(false) => {
            if active_failed {
                runtime.reported_revision = None;
                runtime.reported_digest = None;
                runtime.refresh = true;
                for (_, mut rig, _) in &mut rigs {
                    rig.pose_dirty = true;
                }
            }
        }
        Err(error) => {
            runtime.refresh = false;
            let message = format!("pose frame unchanged: {error:#}");
            if runtime.last_error.as_ref() != Some(&message) {
                events.write(HotpatchEvent::FrameFailed {
                    attempt: runtime.active_attempt,
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

/// Clean-exit safety net on Bevy's explicit exit edge.
///
/// A window close and console `/quit` raise `WindowCloseRequested`, which
/// `bevy_window::exit_on_all_closed` turns into `AppExit` in `PostUpdate`; the Exit Game/logout path
/// writes `AppExit` directly in `Update`. Both precede `Last`, so a `Last` system observes the exit
/// a player causes. This calls the same host method as [`Runtime`]'s destructor, which remains the
/// fallback for a bare `App` drop.
fn clear_active_sentinel_on_app_exit(
    mut exits: MessageReader<AppExit>,
    mut runtime: ResMut<Runtime>,
) {
    if exits.read().next().is_none() {
        return;
    }
    // A pending recovery or an unowned/unvalidated marker is retained by the host method;
    // the Drop fallback warns about those. This edge only reports real clear errors.
    if let Err(error) = runtime.host.clear_active_sentinel_on_clean_shutdown() {
        warn!("hotpatch: AppExit active sentinel clear failed: {error:#}");
    }
    // `impl Drop for Runtime` keeps its `std::thread::panicking()` guard: a system never runs
    // during an unwind, so the destructor still owns the panic case.
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
