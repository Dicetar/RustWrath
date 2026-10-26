//! Generated items -> explicit WM content proposals, never template allocation/publication.
//!
//! The source item and conversion audit remain retained. Mapping a float to an integer is
//! an explicit policy decision; unsupported meanings cannot disappear into a vanilla field.

use std::collections::{BTreeMap, HashSet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::artifact::{ensure, BuildIdentity, Candidate, FloatBits, Hex64, Stat};
use crate::item::{ItemKind, Rarity, StatId};
use crate::ValidationError;

pub const POLICY_SCHEMA: &str = "project-b.procgen.item-mapping.v1";
pub const PLAN_SCHEMA: &str = "project-b.procgen.item-content-plan.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NumericPolicy {
    Exact,
    /// Error is measured in source units after reversing the explicit scale.
    Nearest { max_source_error: FloatBits },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StatTarget {
    ItemStat { stat_type: u32 },
    Armor,
    Damage { school: u32 },
    Unsupported { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatMapping {
    pub source: StatId,
    pub target: StatTarget,
    pub multiplier: FloatBits,
    pub numeric_policy: NumericPolicy,
    /// States units and meaning, e.g. a reviewed rating conversion at the pinned level.
    pub interpretation: String,
    pub review_reference: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemKindMapping {
    pub kind: ItemKind,
    pub base_item_entry: u32,
    pub item_class: u32,
    pub item_subclass: u32,
    pub inventory_type: u32,
    pub display_id: u32,
    pub database_snapshot_sha256: String,
    pub base_record_sha256: String,
    pub asset_profile_sha256: String,
    pub display_record_sha256: String,
    pub inherited_fields_review: String,
    pub delay_ms: u32,
    pub durability: u32,
    pub material: i32,
    pub sheath: u32,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 8>")]
    pub stats: Vec<StatMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemMappingPolicy {
    pub schema_version: String,
    pub candidate_manifest: Hex64,
    pub revision: String,
    pub player_guid: Hex64,
    pub bonding: u32,
    pub required_level: u32,
    pub allowable_class: i32,
    pub allowable_race: i32,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 8>")]
    pub kinds: Vec<ItemKindMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MappedNumber { Integer { value: i32 }, Float { bits: FloatBits } }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conversion {
    pub source: Stat,
    pub mapping: StatMapping,
    pub result: MappedNumber,
    pub reconstructed_source: FloatBits,
    pub exact_source_bits: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentIssue { pub field: String, pub reason: String }

/// Typed proposal assembled into ManagedItemDraft only after owner allocation evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnallocatedItem {
    pub base_item_entry: u32,
    pub name: String,
    pub display_id: u32,
    pub item_class: u32,
    pub item_subclass: u32,
    pub inventory_type: u32,
    pub quality: u32,
    pub item_level: u32,
    pub required_level: u32,
    pub bonding: u32,
    pub allowable_class: i32,
    pub allowable_race: i32,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 10>")]
    pub stats: Vec<ItemStatLine>,
    pub template_defaults: BTreeMap<String, TemplateValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemplateValue { Integer { value: i64 }, Float { bits: FloatBits } }

impl TemplateValue {
    pub(crate) fn json_value(&self) -> Value {
        match self { Self::Integer { value } => json!(value), Self::Float { bits } => json!(bits.get()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemStatLine { pub stat_type: u32, pub stat_value: i32 }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedItem {
    pub source_index: u32,
    pub source_id: Hex64,
    pub allocation_key: String,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 8>")]
    pub conversions: Vec<Conversion>,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 16>")]
    pub issues: Vec<ContentIssue>,
    pub draft: Option<UnallocatedItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemContentPlan {
    pub schema_version: String,
    pub source_identity: BuildIdentity,
    pub policy: ItemMappingPolicy,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 10_000>")]
    pub items: Vec<PlannedItem>,
    pub requires_owner_preflight: bool,
    pub fingerprint: Hex64,
}

fn digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn note(s: &str) -> bool { !s.trim().is_empty() && s.len() <= 1024 }
fn kind_matches(mapping: &ItemKindMapping) -> bool {
    match mapping.kind {
        ItemKind::Sword => mapping.item_class == 2 && ((mapping.item_subclass == 7 && matches!(mapping.inventory_type, 13 | 21 | 22))
            || (mapping.item_subclass == 8 && mapping.inventory_type == 17)),
        ItemKind::Axe => mapping.item_class == 2 && ((mapping.item_subclass == 0 && matches!(mapping.inventory_type, 13 | 21 | 22))
            || (mapping.item_subclass == 1 && mapping.inventory_type == 17)),
        ItemKind::Staff => mapping.item_class == 2 && mapping.item_subclass == 10 && mapping.inventory_type == 17,
        ItemKind::Bow => mapping.item_class == 2 && mapping.item_subclass == 2 && matches!(mapping.inventory_type, 15 | 26),
        ItemKind::Gun => mapping.item_class == 2 && mapping.item_subclass == 3 && matches!(mapping.inventory_type, 15 | 26),
        ItemKind::Shield => mapping.item_class == 4 && mapping.item_subclass == 6 && mapping.inventory_type == 14,
        ItemKind::Helm => mapping.item_class == 4 && (1..=4).contains(&mapping.item_subclass) && mapping.inventory_type == 1,
        ItemKind::Chest => mapping.item_class == 4 && (1..=4).contains(&mapping.item_subclass) && matches!(mapping.inventory_type, 5 | 20),
    }
}

impl ItemMappingPolicy {
    pub fn decode(bytes: &[u8]) -> Result<Self, ValidationError> { crate::json::decode(bytes) }
    fn validate(&self, candidate: &Candidate) -> Result<(), ValidationError> {
        ensure(self.schema_version == POLICY_SCHEMA && self.candidate_manifest == candidate.manifest_hash,
            "content.identity", "unsupported item mapping or different candidate")?;
        ensure(note(&self.revision) && self.revision.len() <= 128
            && self.player_guid.0 > 0 && self.player_guid.0 <= u64::from(u32::MAX)
            && self.bonding <= 4 && self.required_level <= 80 && self.allowable_class != 0 && self.allowable_race != 0,
            "content.policy", "explicit revision, player, binding/level/class/race policy required")?;
        ensure(self.kinds.len() <= 8, "content.mapping", "at most eight item kinds")?;
        for (i, m) in self.kinds.iter().enumerate() {
            ensure(!self.kinds[..i].iter().any(|k| k.kind == m.kind) && kind_matches(m),
                "content.kind", "duplicate kind or class/subclass/inventory mismatch")?;
            ensure(m.base_item_entry > 0 && m.display_id > 0 && digest(&m.database_snapshot_sha256)
                && digest(&m.base_record_sha256) && digest(&m.asset_profile_sha256) && digest(&m.display_record_sha256)
                && note(&m.inherited_fields_review) && m.durability <= 65535 && (-1..=8).contains(&m.material)
                && m.sheath <= 7 && m.delay_ms <= 65535 && (m.item_class != 2 || m.delay_ms > 0),
                "content.template", "base/display pins and bounded explicit template policy required")?;
            ensure(m.stats.len() <= 8, "content.stats", "at most eight stat mappings per kind")?;
            for (j, s) in m.stats.iter().enumerate() {
                ensure(!m.stats[..j].iter().any(|p| p.source == s.source) && note(&s.interpretation) && note(&s.review_reference)
                    && s.multiplier.get().is_finite() && s.multiplier.get() > 0.0 && s.multiplier.get() <= 1_000_000.0,
                    "content.stat_mapping", "unique source stat, explicit meaning/review and positive bounded scale required")?;
                match &s.target {
                    StatTarget::ItemStat { stat_type } => ensure(matches!(*stat_type, 1 | 3..=7 | 12..=39 | 41..=48),
                        "content.stat_type", "unsupported or undefined 12340 ItemModType")?,
                    StatTarget::Armor => ensure(s.source == StatId::Armor, "content.stat_type", "armor destination requires armor source")?,
                    StatTarget::Damage { school } => ensure(s.source == StatId::Damage && *school <= 6 && m.item_class == 2,
                        "content.stat_type", "weapon damage destination requires weapon mapping, damage source and valid school")?,
                    StatTarget::Unsupported { reason } => ensure(note(reason), "content.unsupported", "unsupported mapping requires explanation")?,
                }
                if let NumericPolicy::Nearest { max_source_error } = &s.numeric_policy {
                    ensure(max_source_error.get().is_finite() && (0.0..=100.0).contains(&max_source_error.get()),
                        "content.quantization", "explicit finite source-unit error budget 0..=100 required")?;
                }
            }
        }
        Ok(())
    }
}

fn convert(source: &Stat, mapping: &StatMapping) -> Result<Conversion, ValidationError> {
    if let StatTarget::Unsupported { reason } = &mapping.target {
        return Err(ValidationError::new("content.unsupported", reason));
    }
    let scale = f64::from(mapping.multiplier.get());
    let original = f64::from(source.value.get());
    let scaled = original * scale;
    ensure(scaled.is_finite(), "content.number", "scaled stat is not finite")?;
    let (result, reconstructed) = match mapping.target {
        StatTarget::Damage { .. } => {
            let value = scaled as f32;
            ensure(value.is_finite() && value >= 0.0, "content.damage", "damage must be nonnegative finite f32")?;
            (MappedNumber::Float { bits: FloatBits::from_float(value) }, f64::from(value) / scale)
        }
        _ => {
            let value = scaled.round();
            ensure(value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX), "content.integer", "stat exceeds supported signed integer range")?;
            ensure(!matches!(mapping.target, StatTarget::Armor) || value >= 0.0, "content.armor", "armor cannot be negative")?;
            ensure(!matches!(mapping.target, StatTarget::ItemStat { .. }) || value != 0.0,
                "content.integer", "WM stat line cannot be zero; refuse rather than silently omit source stat")?;
            (MappedNumber::Integer { value: value as i32 }, value / scale)
        }
    };
    let exact = (reconstructed as f32).to_bits() == source.value.0;
    match mapping.numeric_policy {
        NumericPolicy::Exact => ensure(exact, "content.precision", "mapping cannot preserve source float bits; explicit conversion policy required")?,
        NumericPolicy::Nearest { max_source_error } => ensure((reconstructed - original).abs() <= f64::from(max_source_error.get()),
            "content.precision", "quantization exceeds explicit source-unit error budget")?,
    }
    Ok(Conversion { source: source.clone(), mapping: mapping.clone(), result,
        reconstructed_source: FloatBits::from_float(reconstructed as f32), exact_source_bits: exact })
}

fn neutral_defaults(m: &ItemKindMapping) -> BTreeMap<String, TemplateValue> {
    // Exact names inspected in AzerothCore's 12340 item_template SQL schema. WM applies
    // these through its existing template_defaults owner; missing columns need preflight.
    let mut fields = BTreeMap::new();
    for key in ["RandomProperty", "RandomSuffix", "ScalingStatDistribution", "ScalingStatValue",
        "dmg_min1", "dmg_max1", "dmg_type1", "dmg_min2", "dmg_max2", "dmg_type2", "armor",
        "holy_res", "fire_res", "nature_res", "frost_res", "shadow_res", "arcane_res", "block",
        "PageText", "LanguageID", "PageMaterial", "startquest", "lockid", "itemset", "area", "Map",
        "BagFamily", "TotemCategory", "socketColor_1", "socketContent_1", "socketColor_2", "socketContent_2",
        "socketColor_3", "socketContent_3", "socketBonus", "GemProperties", "ArmorDamageModifier", "duration",
        "ItemLimitCategory", "HolidayId", "ContainerSlots", "StatsCount", "RequiredSkill", "RequiredSkillRank",
        "requiredspell", "requiredhonorrank", "RequiredCityRank", "RequiredReputationFaction", "RequiredReputationRank"] {
        fields.insert(key.into(), TemplateValue::Integer { value: 0 });
    }
    fields.insert("delay".into(), TemplateValue::Integer { value: i64::from(m.delay_ms) });
    fields.insert("MaxDurability".into(), TemplateValue::Integer { value: i64::from(m.durability) });
    fields.insert("Material".into(), TemplateValue::Integer { value: i64::from(m.material) });
    fields.insert("sheath".into(), TemplateValue::Integer { value: i64::from(m.sheath) });
    fields
}

impl ItemContentPlan {
    /// Per-item diagnostics are retained; no allocation/publish operation occurs here.
    pub fn build(candidate: &Candidate, build: &BuildIdentity, policy: ItemMappingPolicy) -> Result<Self, ValidationError> {
        candidate.restore(build)?;
        policy.validate(candidate)?;
        let mut key_hash = crate::ManifestHash(0xcbf2_9ce4_8422_2325);
        key_hash.string(POLICY_SCHEMA); key_hash.string(&build.source_archive_sha256);
        key_hash.bytes(&crate::json::encode(&policy)?);
        let allocation_revision = key_hash.0;
        let mut items = Vec::with_capacity(candidate.items.len());
        for (index, item) in candidate.items.iter().enumerate() {
            let mut issues = Vec::new();
            let mut conversions = Vec::new();
            let mut proposed = None;
            if item.name.chars().count() > 120 || item.name.trim().is_empty() {
                issues.push(ContentIssue { field: "name".into(), reason: "WM item name must contain 1..=120 characters".into() });
            }
            if let Some(mapping) = policy.kinds.iter().find(|m| m.kind == item.kind) {
                for stat in &item.stats {
                    if let Some(rule) = mapping.stats.iter().find(|s| s.source == stat.stat) {
                        match convert(stat, rule) { Ok(value) => conversions.push(value), Err(e) => issues.push(ContentIssue { field: format!("stat.{:?}", stat.stat), reason: e.to_string() }) }
                    } else { issues.push(ContentIssue { field: format!("stat.{:?}", stat.stat), reason: "missing explicit stat meaning/conversion".into() }); }
                }
                let mut defaults = neutral_defaults(mapping);
                let mut stats = Vec::new();
                let mut targets = HashSet::new();
                for conversion in &conversions {
                    let key = match conversion.mapping.target { StatTarget::ItemStat { stat_type } => format!("stat:{stat_type}"),
                        StatTarget::Armor => "armor".into(), StatTarget::Damage { .. } => "damage".into(), StatTarget::Unsupported { .. } => "unsupported".into() };
                    if !targets.insert(key) { issues.push(ContentIssue { field: "stats".into(), reason: "multiple sources map to one target; aggregation is not implicit".into() }); continue; }
                    match (&conversion.mapping.target, &conversion.result) {
                        (StatTarget::ItemStat { stat_type }, MappedNumber::Integer { value }) => stats.push(ItemStatLine { stat_type: *stat_type, stat_value: *value }),
                        (StatTarget::Armor, MappedNumber::Integer { value }) => { defaults.insert("armor".into(), TemplateValue::Integer { value: i64::from(*value) }); }
                        (StatTarget::Damage { school }, MappedNumber::Float { bits }) => {
                            defaults.insert("dmg_min1".into(), TemplateValue::Float { bits: *bits }); defaults.insert("dmg_max1".into(), TemplateValue::Float { bits: *bits });
                            defaults.insert("dmg_type1".into(), TemplateValue::Integer { value: i64::from(*school) });
                        }
                        _ => return Err(ValidationError::new("content.conversion", "internal stat-target/result disagreement")),
                    }
                }
                if issues.is_empty() {
                    defaults.insert("StatsCount".into(), TemplateValue::Integer { value: stats.len() as i64 });
                    proposed = Some(UnallocatedItem { base_item_entry: mapping.base_item_entry, name: item.name.clone(), display_id: mapping.display_id,
                        item_class: mapping.item_class, item_subclass: mapping.item_subclass, inventory_type: mapping.inventory_type,
                        quality: match item.rarity { Rarity::Common => 1, Rarity::Uncommon => 2, Rarity::Rare => 3, Rarity::Epic => 4, Rarity::Legendary => 5 },
                        item_level: item.level, required_level: policy.required_level, bonding: policy.bonding,
                        allowable_class: policy.allowable_class, allowable_race: policy.allowable_race, stats, template_defaults: defaults });
                }
            } else { issues.push(ContentIssue { field: "kind".into(), reason: "missing explicit base/display/template mapping".into() }); }
            items.push(PlannedItem { source_index: index as u32, source_id: item.id,
                allocation_key: format!("procgen:item:{allocation_revision:016x}:{:016x}:{index}", item.id.0),
                conversions, issues, draft: proposed });
        }
        let mut result = Self { schema_version: PLAN_SCHEMA.into(), source_identity: build.clone(), policy,
            items, requires_owner_preflight: true, fingerprint: Hex64(0) };
        result.fingerprint = result.calculate_fingerprint()?;
        Ok(result)
    }
    fn calculate_fingerprint(&self) -> Result<Hex64, ValidationError> {
        let mut body = self.clone(); body.fingerprint = Hex64(0);
        let bytes = crate::json::encode(&body)?;
        let mut h = crate::ManifestHash(0xcbf2_9ce4_8422_2325); h.string(PLAN_SCHEMA); h.bytes(&bytes);
        Ok(Hex64(h.0))
    }
    pub fn require_all_ready(&self) -> Result<(), ValidationError> {
        ensure(self.items.iter().all(|i| i.issues.is_empty() && i.draft.is_some()), "content.blocked", "one or more generated items have unsupported/unresolved mappings")
    }
    pub fn verify(&self, candidate: &Candidate, build: &BuildIdentity) -> Result<(), ValidationError> {
        ensure(self == &Self::build(candidate, build, self.policy.clone())?, "content.plan", "item content plan differs from source derivation")
    }
    pub fn decode(bytes: &[u8], candidate: &Candidate, build: &BuildIdentity) -> Result<Self, ValidationError> {
        let plan: Self = crate::json::decode(bytes)?; plan.verify(candidate, build)?; Ok(plan)
    }
    pub fn encode(&self, candidate: &Candidate, build: &BuildIdentity) -> Result<Vec<u8>, ValidationError> {
        self.verify(candidate, build)?; crate::json::encode(self)
    }
}
