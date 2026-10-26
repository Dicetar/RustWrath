//! Existing WM allocation evidence -> inert DraftEnvelope items. No allocator lives here.

use std::collections::{BTreeMap, HashSet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::artifact::{ensure, BuildIdentity, Candidate, Hex64};
use crate::content::{ItemContentPlan, UnallocatedItem};
use crate::ValidationError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservedSlot {
    pub entity_type: String,
    pub reserved_id: u32,
    pub slot_status: String,
    pub arc_key: Option<String>,
    pub character_guid: Option<u32>,
    pub source_quest_id: Option<u32>,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 32>")]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub source_index: u32,
    pub allocation_key: String,
    pub slot: ReservedSlot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservationEvidence {
    pub schema_version: String,
    pub plan_fingerprint: Hex64,
    pub owner_revision: String,
    pub owner_report_sha256: String,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 10_000>")]
    pub reservations: Vec<Reservation>,
}

/// Native owner request fields mirror ReservedSlotDbAllocator.allocate_next_free_slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationRequest {
    pub source_index: u32,
    pub entity_type: String,
    pub arc_key: String,
    pub character_guid: u32,
    pub source_quest_id: Option<u32>,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 32>")]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationRequests {
    pub schema_version: String,
    pub plan_fingerprint: Hex64,
    pub requires_owner_preflight: bool,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 10_000>")]
    pub requests: Vec<AllocationRequest>,
}

/// Serialization matches existing workbench DraftEnvelope. Values for the existing owner
/// are materialized only after verifying the typed proposal and the reservation evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemEnvelope {
    pub kind: String,
    pub template: String,
    pub draft: Value,
    pub reserved_slot: ReservedSlot,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 32>")]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocatedContent {
    pub schema_version: String,
    pub plan_fingerprint: Hex64,
    pub reservations: ReservationEvidence,
    pub requires_owner_preflight: bool,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 10_000>")]
    pub items: Vec<ItemEnvelope>,
}

impl AllocationRequests {
    pub fn build(plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<Self, ValidationError> {
        plan.verify(candidate, build)?;
        plan.require_all_ready()?;
        Ok(Self { schema_version: "project-b.procgen.item-allocation-requests.v1".into(),
            plan_fingerprint: plan.fingerprint, requires_owner_preflight: true,
            requests: plan.items.iter().map(|item| AllocationRequest { source_index: item.source_index,
                entity_type: "item".into(), arc_key: item.allocation_key.clone(), character_guid: plan.policy.player_guid.0 as u32,
                source_quest_id: None, notes: vec!["procgen_candidate; allocation request only".into(),
                    format!("content_plan:{:016x}", plan.fingerprint.0), format!("source_item:{:016x}", item.source_id.0)] }).collect() })
    }
}

impl ReservationEvidence {
    pub fn decode(bytes: &[u8]) -> Result<Self, ValidationError> { crate::json::decode(bytes) }
    pub fn validate(&self, plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<(), ValidationError> {
        plan.verify(candidate, build)?;
        plan.require_all_ready()?;
        ensure(self.schema_version == "project-b.procgen.item-reservations.v1" && self.plan_fingerprint == plan.fingerprint,
            "content.reservations", "reservation report belongs to another content plan")?;
        ensure(!self.owner_revision.trim().is_empty() && self.owner_revision.len() <= 128
            && self.owner_report_sha256.len() == 64 && self.owner_report_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "content.reservations", "retained owner revision/report digest required")?;
        ensure(self.reservations.len() == plan.items.len(), "content.reservations", "one reservation per item required; no implicit partial allocation")?;
        let mut ids = HashSet::new();
        let mut indexes = HashSet::new();
        for reservation in &self.reservations {
            let item = plan.items.get(reservation.source_index as usize)
                .ok_or_else(|| ValidationError::new("content.reservation", "unknown source index"))?;
            let slot = &reservation.slot;
            ensure(indexes.insert(reservation.source_index) && reservation.allocation_key == item.allocation_key
                && slot.entity_type == "item" && slot.reserved_id > 0 && ids.insert(slot.reserved_id)
                && slot.slot_status == "staged" && slot.arc_key.as_deref() == Some(item.allocation_key.as_str())
                && slot.character_guid == Some(plan.policy.player_guid.0 as u32) && slot.source_quest_id.is_none(),
                "content.reservation", "requires a unique fresh staged item slot with matching owner/allocation identity")?;
            ensure(!plan.policy.kinds.iter().any(|k| k.base_item_entry == slot.reserved_id),
                "content.reservation", "allocated destination collides with a pinned base template")?;
            ensure(slot.notes.len() <= 32 && slot.notes.iter().all(|s| s.len() <= 1024),
                "content.reservation", "reservation notes exceed bounds")?;
        }
        Ok(())
    }
}

fn materialize(item: &UnallocatedItem, slot: &ReservedSlot) -> Value {
    let defaults: BTreeMap<_, _> = item.template_defaults.iter().map(|(k, v)| (k.clone(), v.json_value())).collect();
    json!({ "item_entry": slot.reserved_id, "base_item_entry": item.base_item_entry,
        "name": item.name, "description": "Generated candidate; retained provenance is in its authoring package.",
        "display_id": item.display_id, "item_class": item.item_class, "item_subclass": item.item_subclass,
        "inventory_type": item.inventory_type, "quality": item.quality, "item_level": item.item_level,
        "required_level": item.required_level, "bonding": item.bonding, "allowable_class": item.allowable_class,
        "allowable_race": item.allowable_race, "buy_price": 0, "sell_price": 0, "max_count": 1, "stackable": 1,
        "clear_stats": true, "clear_spells": true, "stats": item.stats, "spells": [],
        "tags": ["wm_generated", "managed_item_slot", "procgen_candidate"], "template_defaults": defaults })
}

impl AllocatedContent {
    pub fn build(plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity, evidence: ReservationEvidence) -> Result<Self, ValidationError> {
        evidence.validate(plan, candidate, build)?;
        let slots: BTreeMap<_, _> = evidence.reservations.iter().map(|r| (r.source_index, &r.slot)).collect();
        let mut items = Vec::with_capacity(plan.items.len());
        for item in &plan.items {
            let slot = *slots.get(&item.source_index)
                .ok_or_else(|| ValidationError::new("content.reservation", "missing source reservation"))?;
            let source = item.draft.as_ref().ok_or_else(|| ValidationError::new("content.blocked", "unresolved item draft"))?;
            items.push(ItemEnvelope { kind: "item".into(), template: "procgen_explicit_mapping_v1".into(),
                draft: materialize(source, slot), reserved_slot: slot.clone(), notes: vec![
                    "Requires existing WM owner preflight; allocation evidence is not live publication.".into(),
                    format!("content_plan:{:016x}; source_index:{}", plan.fingerprint.0, item.source_index)] });
        }
        Ok(Self { schema_version: "project-b.procgen.allocated-item-content.v1".into(), plan_fingerprint: plan.fingerprint,
            reservations: evidence, requires_owner_preflight: true, items })
    }
    pub fn verify(&self, plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<(), ValidationError> {
        let mut expected = Self::build(plan, candidate, build, self.reservations.clone())?;
        let mut actual = self.clone();
        // The WM boundary has decimal JSON numbers, but its damage columns are f32.
        // Normalize those two values to their server representation before comparison.
        for content in [&mut actual, &mut expected] {
            for item in &mut content.items {
                for field in ["dmg_min1", "dmg_max1"] {
                    let value = item.draft["template_defaults"][field].as_f64()
                        .ok_or_else(|| ValidationError::new("content.damage", "missing numeric WM damage column"))? as f32;
                    ensure(value.is_finite(), "content.damage", "non-finite WM damage column")?;
                    item.draft["template_defaults"][field] = json!(value);
                }
            }
        }
        ensure(actual == expected,
            "content.allocated", "allocated content differs from verified plan and reservations")
    }
    pub fn encode(&self, plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<Vec<u8>, ValidationError> {
        self.verify(plan, candidate, build)?; crate::json::encode(self)
    }
    pub fn decode(bytes: &[u8], plan: &ItemContentPlan, candidate: &Candidate, build: &BuildIdentity) -> Result<Self, ValidationError> {
        let content: Self = crate::json::decode(bytes)?; content.verify(plan, candidate, build)?; Ok(content)
    }
}
