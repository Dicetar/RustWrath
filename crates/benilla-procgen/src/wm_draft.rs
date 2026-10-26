//! Inert drafts for WM's existing ordered bundle owner, with receipt-based cleanup.
//! No runner, transport, content allocator or mutation path lives here.

use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::artifact::{ensure, BuildIdentity, Candidate, Hex64};
use crate::planning::{EncounterPlan, WorldScope};
use crate::ValidationError;

pub const DRAFT_SCHEMA: &str = "project-b.procgen.wm-draft.v1";
pub const RECEIPT_SCHEMA: &str = "project-b.procgen.application-receipt.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftOptions {
    pub namespace: String,
    pub run_key: String,
    pub duration_ms: u32,
    pub max_actors: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStep { pub kind: String, pub action_kind: String, pub payload: Value }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle { pub schema_version: String, pub id: String, pub steps: Vec<NativeStep> }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupRequirement {
    pub spawn_step: u32,
    pub local_id: String,
    pub arc_key: String,
    pub result_identity_field: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WmDraft {
    pub schema_version: String,
    pub plan_fingerprint: Hex64,
    pub scope: WorldScope,
    pub options: DraftOptions,
    pub requires_owner_preflight: bool,
    pub bundle: Bundle,
    pub cleanup: Vec<CleanupRequirement>,
    pub partial_application_policy: String,
}

fn token(s: &str, limit: usize) -> bool {
    !s.is_empty() && s.len() <= limit && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl WmDraft {
    pub fn build(plan: &EncounterPlan, candidate: &Candidate, build: &BuildIdentity, options: DraftOptions) -> Result<Self, ValidationError> {
        plan.verify(candidate, build)?;
        ensure(token(&options.namespace, 24) && token(&options.run_key, 48), "wm_draft.identity", "explicit bounded namespace/run-key tokens required")?;
        ensure((1000..=600_000).contains(&options.duration_ms) && (1..=512).contains(&options.max_actors)
            && !plan.actors.is_empty() && plan.actors.len() <= options.max_actors as usize,
            "wm_draft.budget", "temporary duration 1000..=600000 ms and nonempty complete plan within explicit actor budget (maximum 512)")?;
        ensure(plan.request.scope.player_guid.0 <= u64::from(u32::MAX), "wm_draft.player", "existing WM owner requires player GUID low counter")?;
        let id = format!("procgen-{:016x}-{}", plan.fingerprint.0, options.run_key);
        let mut steps = Vec::with_capacity(plan.actors.len());
        let mut cleanup = Vec::with_capacity(plan.actors.len());
        for (i, actor) in plan.actors.iter().enumerate() {
            let arc_key = format!("{}:{:016x}:{}:{i}", options.namespace, plan.fingerprint.0, options.run_key);
            steps.push(NativeStep { kind: "native".into(), action_kind: "creature_spawn".into(),
                payload: json!({ "creature_entry": actor.creature_entry, "arc_key": arc_key,
                    "duration_ms": options.duration_ms, "map_id": plan.request.scope.map_id,
                    "x": actor.position[0].get(), "y": actor.position[1].get(), "z": actor.position[2].get(),
                    "orientation": actor.orientation.get(), "use_template_faction": true, "follow_player": false }) });
            cleanup.push(CleanupRequirement { spawn_step: i as u32, local_id: actor.local_id.clone(),
                arc_key, result_identity_field: "object_id".into() });
        }
        Ok(Self { schema_version: DRAFT_SCHEMA.into(), plan_fingerprint: plan.fingerprint,
            scope: plan.request.scope.clone(), options, requires_owner_preflight: true,
            bundle: Bundle { schema_version: "wm.world.bundle.v1".into(), id, steps }, cleanup,
            partial_application_policy: "retain receipts; reconcile unknown steps; never replay the whole bundle; cleanup only actual returned owned IDs".into() })
    }

    pub fn verify(&self, plan: &EncounterPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<(), ValidationError> {
        let expected = Self::build(plan, candidate, build, self.options.clone())?;
        ensure(self == &expected, "wm_draft.integrity", "draft does not match complete verified encounter plan")
    }
    pub fn encode(&self, plan: &EncounterPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<Vec<u8>, ValidationError> {
        self.verify(plan, candidate, build)?;
        crate::json::encode(self)
    }
    pub fn decode(bytes: &[u8], plan: &EncounterPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<Self, ValidationError> {
        let draft: Self = crate::json::decode(bytes)?;
        draft.verify(plan, candidate, build)?;
        Ok(draft)
    }

    /// Read a saved result from the existing WM bundle runner. A matching report is still
    /// caller-supplied evidence, not a cryptographic attestation or a fresh server query.
    pub fn import_report(&self, bytes: &[u8], plan: &EncounterPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<ApplicationReceipt, ValidationError> {
        self.verify(plan, candidate, build)?;
        let report: Value = crate::json::decode(bytes)?;
        ensure(report["bundle_id"].as_str() == Some(self.bundle.id.as_str())
            && report["run_key"].as_str() == Some(self.options.run_key.as_str()),
            "wm_receipt.identity", "report bundle/run does not match draft")?;
        let mode = report["mode"].as_str().unwrap_or("");
        ensure(mode == "apply" || mode == "dry-run", "wm_receipt.mode", "expected explicit WM apply/dry-run report mode")?;
        let results = report["steps"].as_array().ok_or_else(|| ValidationError::new("wm_receipt.steps", "missing ordered results"))?;
        ensure(!results.is_empty() && results.len() <= self.bundle.steps.len(), "wm_receipt.steps", "invalid result prefix length")?;
        let mut steps = Vec::with_capacity(self.bundle.steps.len());
        let mut object_ids = HashSet::new();
        for (i, result) in results.iter().enumerate() {
            ensure(result["index"].as_u64() == Some(i as u64) && result["kind"].as_str() == Some("native")
                && result["ok"].is_boolean(), "wm_receipt.step", "noncontiguous or invalid native result")?;
            let ok = result["ok"].as_bool() == Some(true);
            ensure(ok || i + 1 == results.len(), "wm_receipt.step", "runner must stop at first failed step")?;
            let outcome = if mode == "dry-run" {
                StepOutcome::Preview
            } else if ok {
                let native = &result["result"]["world_result"];
                let matched = result["result"]["status"].as_str() == Some("applied")
                    && native["status"].as_str() == Some("done") && native["ok"].as_bool() == Some(true)
                    && native["action_kind"].as_str() == Some("creature_spawn")
                    && native["message"].as_str() == Some("creature_spawned")
                    && native["player_guid"].as_u64() == Some(self.scope.player_guid.0)
                    && native["creature_entry"].as_u64() == Some(u64::from(plan.actors[i].creature_entry))
                    && native["arc_key"].as_str() == Some(self.cleanup[i].arc_key.as_str());
                let object = native["object_id"].as_u64().unwrap_or(0);
                let live = native["live_guid_low"].as_u64().unwrap_or(0);
                let guid = native["live_guid"].as_str().unwrap_or("");
                if matched && object > 0 && object <= u64::from(u32::MAX)
                    && live > 0 && live <= u64::from(u32::MAX) && !guid.is_empty() && guid.len() <= 128
                    && object_ids.insert(object) {
                    StepOutcome::Applied { object_id: Hex64(object), live_guid_low: live as u32, live_guid: guid.into() }
                } else {
                    // Preserve earlier usable receipts when a nominally successful mutation
                    // lacks an addressable identity (the native insert/query can return zero).
                    StepOutcome::Unknown
                }
            } else {
                // Failed/exception/timeout can follow a real mutation. Never infer no effect.
                StepOutcome::Unknown
            };
            steps.push(StepReceipt { index: i as u32, local_id: self.cleanup[i].local_id.clone(), outcome });
        }
        let all_ok = results.iter().all(|r| r["ok"].as_bool() == Some(true));
        let expected_status = if results.len() == self.bundle.steps.len() && all_ok { "complete" }
            else if results.iter().any(|r| r["ok"].as_bool() == Some(true)) { "partial" } else { "failed" };
        ensure(report["status"].as_str() == Some(expected_status)
            && (expected_status == "complete" || !all_ok), "wm_receipt.status", "inconsistent or truncated bundle report")?;
        for i in results.len()..self.bundle.steps.len() {
            steps.push(StepReceipt { index: i as u32, local_id: self.cleanup[i].local_id.clone(), outcome: StepOutcome::NotAttempted });
        }
        Ok(ApplicationReceipt { schema_version: RECEIPT_SCHEMA.into(), bundle_id: self.bundle.id.clone(),
            run_key: self.options.run_key.clone(), plan_fingerprint: self.plan_fingerprint,
            scope: self.scope.clone(), mode: mode.into(), steps,
            report_fingerprint: fingerprint_bytes(bytes) })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepOutcome {
    Applied { object_id: Hex64, live_guid_low: u32, live_guid: String },
    Unknown,
    NotAttempted,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepReceipt { pub index: u32, pub local_id: String, pub outcome: StepOutcome }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationReceipt {
    pub schema_version: String,
    pub bundle_id: String,
    pub run_key: String,
    pub plan_fingerprint: Hex64,
    pub scope: WorldScope,
    pub mode: String,
    pub steps: Vec<StepReceipt>,
    pub report_fingerprint: Hex64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupDraft {
    pub schema_version: String,
    pub original_bundle_id: String,
    pub original_run_key: String,
    pub scope: WorldScope,
    pub requires_owner_preflight: bool,
    pub reconciliation_required: Vec<u32>,
    /// None means no confirmed owned IDs; unknown effects can still require reconciliation.
    pub bundle: Option<Bundle>,
}

impl ApplicationReceipt {
    /// Reconcile against the retained original report; never trust an edited receipt alone.
    pub fn verify(&self, original_report: &[u8], draft: &WmDraft, plan: &EncounterPlan,
        candidate: &Candidate, build: &BuildIdentity) -> Result<(), ValidationError> {
        ensure(self == &draft.import_report(original_report, plan, candidate, build)?,
            "wm_receipt.integrity", "receipt differs from retained original report")
    }

    pub fn cleanup_draft(&self, original_report: &[u8], draft: &WmDraft, plan: &EncounterPlan,
        candidate: &Candidate, build: &BuildIdentity) -> Result<CleanupDraft, ValidationError> {
        self.verify(original_report, draft, plan, candidate, build)?;
        ensure(self.mode == "apply", "wm_cleanup.mode", "preview reports do not identify live owned objects")?;
        let mut steps = Vec::new();
        let mut unknown = Vec::new();
        for step in self.steps.iter().rev() {
            match &step.outcome {
                StepOutcome::Applied { object_id, .. } => steps.push(NativeStep { kind: "native".into(), action_kind: "creature_despawn".into(),
                    payload: json!({ "object_id": object_id.0 }) }),
                StepOutcome::Unknown => unknown.push(step.index),
                StepOutcome::NotAttempted | StepOutcome::Preview => {}
            }
        }
        let bundle = if steps.is_empty() { None } else { Some(Bundle { schema_version: "wm.world.bundle.v1".into(),
            id: format!("{}-cleanup", self.bundle_id), steps }) };
        Ok(CleanupDraft { schema_version: "project-b.procgen.cleanup-draft.v1".into(), original_bundle_id: self.bundle_id.clone(),
            original_run_key: self.run_key.clone(), scope: self.scope.clone(), requires_owner_preflight: true,
            reconciliation_required: unknown, bundle })
    }

    /// Confirm only actual native despawn outcomes from a separately retained cleanup run.
    /// A created cleanup draft, elapsed duration or missing live creature is not retirement.
    pub fn import_cleanup_report(&self, original_report: &[u8], draft: &WmDraft, plan: &EncounterPlan,
        candidate: &Candidate, build: &BuildIdentity, cleanup_report: &[u8],
        cleanup_run_key: &str) -> Result<CleanupReceipt, ValidationError> {
        let cleanup = self.cleanup_draft(original_report, draft, plan, candidate, build)?;
        ensure(token(cleanup_run_key, 48) && cleanup_run_key != self.run_key,
            "wm_cleanup.run", "distinct explicit cleanup run key required")?;
        let bundle = cleanup.bundle.as_ref().ok_or_else(|| ValidationError::new("wm_cleanup.bundle", "no confirmed owned objects to clean"))?;
        let report: Value = crate::json::decode(cleanup_report)?;
        ensure(report["bundle_id"].as_str() == Some(bundle.id.as_str())
            && report["run_key"].as_str() == Some(cleanup_run_key), "wm_cleanup.identity", "cleanup report belongs to another bundle/run")?;
        let mode = report["mode"].as_str().unwrap_or("");
        ensure(mode == "apply" || mode == "dry-run", "wm_cleanup.mode", "explicit apply/dry-run mode required")?;
        let results = report["steps"].as_array().ok_or_else(|| ValidationError::new("wm_cleanup.steps", "missing ordered results"))?;
        ensure(!results.is_empty() && results.len() <= bundle.steps.len(), "wm_cleanup.steps", "invalid cleanup result prefix")?;
        let mut objects = Vec::with_capacity(bundle.steps.len());
        for (i, step) in bundle.steps.iter().enumerate() {
            let id = step.payload["object_id"].as_u64().ok_or_else(|| ValidationError::new("wm_cleanup.object", "missing derived ObjectID"))?;
            let outcome = if let Some(result) = results.get(i) {
                ensure(result["index"].as_u64() == Some(i as u64) && result["kind"].as_str() == Some("native")
                    && result["ok"].is_boolean(), "wm_cleanup.step", "invalid cleanup result order/kind")?;
                let ok = result["ok"].as_bool() == Some(true);
                ensure(ok || i + 1 == results.len(), "wm_cleanup.step", "runner continued after failed cleanup")?;
                let native = &result["result"]["world_result"];
                if mode == "dry-run" { CleanupOutcome::Preview }
                else if ok && result["result"]["status"].as_str() == Some("applied")
                    && native["ok"].as_bool() == Some(true) && native["status"].as_str() == Some("done")
                    && native["action_kind"].as_str() == Some("creature_despawn")
                    && native["message"].as_str() == Some("creature_despawned") && native["object_id"].as_u64() == Some(id) {
                    CleanupOutcome::Removed
                } else { CleanupOutcome::Unknown }
            } else { CleanupOutcome::NotAttempted };
            objects.push(CleanupObject { object_id: Hex64(id), outcome });
        }
        let all_ok = results.iter().all(|r| r["ok"].as_bool() == Some(true));
        let status = if results.len() == bundle.steps.len() && all_ok { "complete" }
            else if results.iter().any(|r| r["ok"].as_bool() == Some(true)) { "partial" } else { "failed" };
        ensure(report["status"].as_str() == Some(status) && (status == "complete" || !all_ok),
            "wm_cleanup.status", "inconsistent or truncated cleanup report")?;
        let requires_reconciliation = !cleanup.reconciliation_required.is_empty()
            || objects.iter().any(|o| o.outcome != CleanupOutcome::Removed);
        Ok(CleanupReceipt { schema_version: "project-b.procgen.cleanup-receipt.v1".into(),
            original_bundle_id: self.bundle_id.clone(), cleanup_bundle_id: bundle.id.clone(),
            cleanup_run_key: cleanup_run_key.into(), scope: self.scope.clone(), mode: mode.into(),
            objects, unresolved_spawn_steps: cleanup.reconciliation_required, requires_reconciliation,
            report_fingerprint: fingerprint_bytes(cleanup_report) })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupOutcome { Removed, Unknown, NotAttempted, Preview }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupObject { pub object_id: Hex64, pub outcome: CleanupOutcome }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupReceipt {
    pub schema_version: String,
    pub original_bundle_id: String,
    pub cleanup_bundle_id: String,
    pub cleanup_run_key: String,
    pub scope: WorldScope,
    pub mode: String,
    pub objects: Vec<CleanupObject>,
    pub unresolved_spawn_steps: Vec<u32>,
    pub requires_reconciliation: bool,
    pub report_fingerprint: Hex64,
}

fn fingerprint_bytes(bytes: &[u8]) -> Hex64 {
    let mut h = crate::ManifestHash(0xcbf2_9ce4_8422_2325);
    h.string(RECEIPT_SCHEMA); h.len(bytes.len()); h.bytes(bytes);
    Hex64(h.0)
}
