//! Deterministic encounter proposals on existing maps; no collision or server authority.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::artifact::{ensure, ActorKind, BuildIdentity, Candidate, FloatBits, Hex64};
use crate::spawns::{NpcRole, SpawnDomain};
use crate::ValidationError;

pub const REQUEST_SCHEMA: &str = "project-b.procgen.placement-request.v1";
pub const PLAN_SCHEMA: &str = "project-b.procgen.encounter-plan.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldScope {
    pub map_id: u32,
    pub instance_id: u32,
    pub phase_mask: u32,
    pub player_guid: Hex64,
    pub world_generation: Hex64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeightRule {
    /// Fixed elevation is a proposal, not a ground intersection.
    Flat,
    /// Only valid for terrain samples. Generated terrain is not stock-map elevation.
    Terrain { vertical_scale: FloatBits },
    /// Absolute world Z for every cell in this domain, in row-major order.
    Samples {
        #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 1_048_576>")]
        elevations: Vec<FloatBits>
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainTransform {
    pub domain: SpawnDomain,
    pub origin: [FloatBits; 3],
    /// World XY displacement per local X/Y sample; no implicit axis convention.
    pub x_axis: [FloatBits; 2],
    pub y_axis: [FloatBits; 2],
    pub orientation: FloatBits,
    pub height: HeightRule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateBinding {
    pub kind: ActorKind,
    pub creature_entry: u32,
    pub database_snapshot_sha256: String,
    pub template_record_sha256: String,
    pub min_level: u32,
    pub max_level: u32,
    /// Caller-attested configured services. Role tags never configure the server.
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 3>")]
    pub services: Vec<NpcRole>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRequest {
    pub schema_version: String,
    pub candidate_manifest: Hex64,
    pub revision: String,
    pub scope: WorldScope,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 2>")]
    pub transforms: Vec<DomainTransform>,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 16>")]
    pub templates: Vec<TemplateBinding>,
    pub minimum: [FloatBits; 3],
    pub maximum: [FloatBits; 3],
    /// Horizontal world spacing. Zero explicitly permits coincident actors.
    pub minimum_separation: FloatBits,
    /// Retained external placement/collision review locator; not automatically trusted.
    pub placement_review: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedActor {
    /// Candidate-local identity, never a creature GUID or database spawn ID.
    pub local_id: String,
    pub source_index: u32,
    pub domain: SpawnDomain,
    pub encounter: u32,
    pub kind: ActorKind,
    pub level: u32,
    pub creature_entry: u32,
    pub position: [FloatBits; 3],
    pub orientation: FloatBits,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterPlan {
    pub schema_version: String,
    pub source_identity: BuildIdentity,
    pub request: PlacementRequest,
    #[serde(deserialize_with = "crate::json::bounded_vec::<_, _, 100_000>")]
    pub actors: Vec<PlannedActor>,
    /// Always requires the existing WM owner to recheck live scope and placement.
    pub requires_owner_preflight: bool,
    pub fingerprint: Hex64,
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn finite(values: &[FloatBits]) -> bool { values.iter().all(|v| v.get().is_finite()) }
fn domain_tag(domain: SpawnDomain) -> &'static str {
    match domain { SpawnDomain::Dungeon => "dungeon", SpawnDomain::Overworld => "overworld" }
}

impl PlacementRequest {
    pub fn decode(bytes: &[u8]) -> Result<Self, ValidationError> { crate::json::decode(bytes) }

    fn validate(&self, candidate: &Candidate) -> Result<(), ValidationError> {
        ensure(self.schema_version == REQUEST_SCHEMA && self.candidate_manifest == candidate.manifest_hash,
            "placement.identity", "unsupported schema or different candidate manifest")?;
        ensure(!self.revision.is_empty() && self.revision.len() <= 128,
            "placement.revision", "explicit revision, maximum 128 bytes required")?;
        ensure(self.scope.map_id <= 65535 && self.scope.phase_mask != 0 && self.scope.player_guid.0 != 0
            && self.scope.world_generation.0 != 0, "placement.scope", "map, nonzero player/generation and phase required")?;
        ensure(finite(&self.minimum) && finite(&self.maximum)
            && (0..3).all(|i| self.minimum[i].get() <= self.maximum[i].get()
                && self.minimum[i].get().abs() <= 1_000_000.0 && self.maximum[i].get().abs() <= 1_000_000.0),
            "placement.bounds", "finite ordered explicit bounds within +/-1000000 required")?;
        let separation = self.minimum_separation.get();
        ensure(separation.is_finite() && (separation == 0.0 || (0.001..=1000.0).contains(&separation)),
            "placement.spacing", "horizontal minimum separation must be zero or finite 0.001..=1000")?;
        ensure(self.placement_review.as_ref().is_none_or(|s| !s.is_empty() && s.len() <= 1024),
            "placement.review", "invalid review locator")?;
        ensure(!self.transforms.is_empty() && self.transforms.len() <= 2 && self.templates.len() <= 16,
            "placement.mapping", "one transform per selected domain and at most 16 bindings")?;
        for (i, t) in self.transforms.iter().enumerate() {
            ensure(!self.transforms[..i].iter().any(|p| p.domain == t.domain), "placement.transform", "duplicate domain transform")?;
            ensure(finite(&t.origin) && finite(&t.x_axis) && finite(&t.y_axis) && t.orientation.get().is_finite(),
                "placement.transform", "non-finite transform")?;
            let determinant = f64::from(t.x_axis[0].get()) * f64::from(t.y_axis[1].get())
                - f64::from(t.x_axis[1].get()) * f64::from(t.y_axis[0].get());
            ensure(determinant.abs() >= 0.000001 && t.x_axis.iter().chain(t.y_axis.iter()).all(|v| v.get().abs() <= 1000.0),
                "placement.transform", "degenerate or excessive coordinate axes")?;
            match &t.height {
                HeightRule::Flat => {}
                HeightRule::Terrain { vertical_scale } => ensure(t.domain == SpawnDomain::Overworld
                    && vertical_scale.get().is_finite() && vertical_scale.get().abs() <= 100_000.0,
                    "placement.height", "terrain height requires overworld and finite bounded vertical scale")?,
                HeightRule::Samples { elevations } => {
                    let count = match t.domain { SpawnDomain::Dungeon => candidate.dungeon.tiles.len(), SpawnDomain::Overworld => candidate.terrain.heights.len() };
                    ensure(elevations.len() == count && finite(elevations), "placement.height", "explicit height grid must match domain dimensions")?;
                }
            }
        }
        for (i, binding) in self.templates.iter().enumerate() {
            if let ActorKind::Enemy { archetype, .. } = &binding.kind {
                crate::artifact::supported_archetype(archetype)?;
            }
            ensure(!self.templates[..i].iter().any(|b| b.kind == binding.kind), "placement.template", "duplicate actor-kind binding")?;
            ensure(binding.creature_entry > 0 && digest(&binding.database_snapshot_sha256) && digest(&binding.template_record_sha256)
                && (1..=80).contains(&binding.min_level) && (binding.min_level..=80).contains(&binding.max_level),
                "placement.template", "entry, retained database/record hashes and supported level interval required")?;
            ensure(binding.services.len() <= 3 && binding.services.iter().enumerate().all(|(i, role)| !binding.services[..i].contains(role)),
                "placement.template", "duplicate or excessive service declarations")?;
            if let ActorKind::Npc { role } = binding.kind {
                ensure(binding.services.contains(&role), "placement.services", "NPC role lacks caller-attested configured template service")?;
            }
        }
        Ok(())
    }
}

impl EncounterPlan {
    /// Build in original actor order, preserving domain and encounter identities. No nudging,
    /// dropping, template inference, world lookup or mutation occurs on refusal.
    pub fn build(candidate: &Candidate, supported_build: &BuildIdentity, request: PlacementRequest) -> Result<Self, ValidationError> {
        candidate.restore(supported_build)?;
        request.validate(candidate)?;
        let separation = f64::from(request.minimum_separation.get());
        let mut occupied: HashMap<(i64, i64), Vec<[f64; 2]>> = HashMap::new();
        let mut actors = Vec::with_capacity(candidate.spawns.len());
        for (index, actor) in candidate.spawns.iter().enumerate() {
            let transform = request.transforms.iter().find(|t| t.domain == actor.domain)
                .ok_or_else(|| ValidationError::new("placement.transform", "missing transform for populated domain"))?;
            let binding = request.templates.iter().find(|t| t.kind == actor.kind)
                .ok_or_else(|| ValidationError::new("placement.template", "missing explicit actor-kind mapping"))?;
            ensure((binding.min_level..=binding.max_level).contains(&actor.level), "placement.level", "generated level outside pinned template capability")?;
            let width = match actor.domain { SpawnDomain::Dungeon => candidate.dungeon.width, SpawnDomain::Overworld => candidate.terrain.width };
            let cell = (actor.y * width + actor.x) as usize;
            let mut point = [0.0f64; 3];
            for (axis, value) in point[..2].iter_mut().enumerate() {
                *value = f64::from(transform.origin[axis].get())
                    + f64::from(actor.x) * f64::from(transform.x_axis[axis].get())
                    + f64::from(actor.y) * f64::from(transform.y_axis[axis].get());
            }
            point[2] = match &transform.height {
                HeightRule::Flat => f64::from(transform.origin[2].get()),
                HeightRule::Terrain { vertical_scale } => f64::from(transform.origin[2].get())
                    + f64::from(candidate.terrain.heights[cell].get()) * f64::from(vertical_scale.get()),
                HeightRule::Samples { elevations } => f64::from(elevations[cell].get()),
            };
            // Store the actual f32 position the native client/server owners consume.
            let position = point.map(|v| FloatBits::from_float(v as f32));
            ensure((0..3).all(|i| position[i].get().is_finite()
                && position[i].get() >= request.minimum[i].get() && position[i].get() <= request.maximum[i].get()),
                "placement.position", "transformed actor outside explicit world bounds")?;
            if separation > 0.0 {
                let xy = [f64::from(position[0].get()), f64::from(position[1].get())];
                let key = ((xy[0] / separation).floor() as i64, (xy[1] / separation).floor() as i64);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        if let Some(points) = occupied.get(&(key.0 + dx, key.1 + dy)) {
                            ensure(points.iter().all(|p| (p[0] - xy[0]).powi(2) + (p[1] - xy[1]).powi(2) >= separation * separation),
                                "placement.spacing", "actors violate explicit minimum spacing; revise request rather than silently move them")?;
                        }
                    }
                }
                occupied.entry(key).or_default().push(xy);
            }
            actors.push(PlannedActor { local_id: format!("{:016x}:{}:{index}", candidate.manifest_hash.0, domain_tag(actor.domain)),
                source_index: index as u32, domain: actor.domain, encounter: actor.encounter,
                kind: actor.kind.clone(), level: actor.level, creature_entry: binding.creature_entry,
                position, orientation: transform.orientation });
        }
        let mut plan = Self { schema_version: PLAN_SCHEMA.into(), source_identity: supported_build.clone(),
            request, actors, requires_owner_preflight: true, fingerprint: Hex64(0) };
        plan.fingerprint = plan.calculate_fingerprint()?;
        Ok(plan)
    }

    fn calculate_fingerprint(&self) -> Result<Hex64, ValidationError> {
        let mut unsigned = self.clone();
        unsigned.fingerprint = Hex64(0);
        let bytes = crate::json::encode(&unsigned)?;
        let mut h = crate::ManifestHash(0xcbf2_9ce4_8422_2325);
        h.string(PLAN_SCHEMA); h.bytes(&bytes);
        Ok(Hex64(h.0))
    }

    /// Rebuild and compare every derived field; imported plan bodies are never trusted.
    pub fn verify(&self, candidate: &Candidate, supported_build: &BuildIdentity) -> Result<(), ValidationError> {
        let rebuilt = Self::build(candidate, supported_build, self.request.clone())?;
        ensure(self == &rebuilt, "placement.plan", "plan differs from candidate/request derivation")
    }

    pub fn encode(&self, candidate: &Candidate, build: &BuildIdentity) -> Result<Vec<u8>, ValidationError> {
        self.verify(candidate, build)?;
        crate::json::encode(self)
    }
    pub fn decode(bytes: &[u8], candidate: &Candidate, build: &BuildIdentity) -> Result<Self, ValidationError> {
        let plan: Self = crate::json::decode(bytes)?;
        plan.verify(candidate, build)?;
        Ok(plan)
    }
}
