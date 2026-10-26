//! Owned, versioned candidates. Import is validation, never publication authority.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use crate::{ProcgenOutput, ProcgenSpec, ValidationError, GENERATOR_VERSION};
use crate::dungeon::{Dungeon, DungeonSpec};
use crate::item::{self, GeneratedItem, ItemKind, Part, PartSlot, Rarity, StatId};
use crate::spawns::{NpcRole, Spawn, SpawnDomain, SpawnKind, SpawnSpec};
use crate::terrain::{Biome, TerrainField, TerrainSpec};

pub const SCHEMA: &str = "project-b.procgen.candidate.v1";
pub const CATALOG_VERSION: &str = "project-b.builtin-parts.v1";

macro_rules! hex_type {
    ($name:ident, $ty:ty, $digits:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub $ty);
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&format!(concat!("{:0", $digits, "x}"), self.0))
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                let value = String::deserialize(de)?;
                if value.len() != $digits || !value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                    return Err(serde::de::Error::custom("expected fixed-width lowercase hexadecimal"));
                }
                <$ty>::from_str_radix(&value, 16).map(Self).map_err(serde::de::Error::custom)
            }
        }
    };
}
hex_type!(Hex64, u64, 16);
hex_type!(FloatBits, u32, 8);

impl FloatBits {
    pub fn get(self) -> f32 { f32::from_bits(self.0) }
    pub fn from_float(value: f32) -> Self { Self(value.to_bits()) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    pub repository: String,
    pub commit: String,
    /// SHA-256 of a retained source archive, required even for a clean checkout.
    pub source_archive_sha256: String,
    pub dirty: bool,
}

impl BuildIdentity {
    pub fn validate(&self) -> Result<(), ValidationError> {
        ensure(!self.repository.is_empty() && self.repository.len() <= 1024, "identity.repository", "required repository, maximum 1024 bytes")?;
        ensure((self.commit.len() == 40 || self.commit.len() == 64) && hex(&self.commit), "identity.commit", "full lowercase Git object identity required")?;
        ensure(self.source_archive_sha256.len() == 64 && hex(&self.source_archive_sha256), "identity.archive", "retained source archive SHA-256 required")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogIdentity { pub version: String, pub fingerprint: Hex64 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainRequest {
    pub width: u32, pub height: u32, pub frequency: FloatBits,
    pub octaves: u32, pub sea_level: FloatBits,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub seed: Hex64, pub items: u32, pub dungeon: DungeonSpec,
    pub terrain: TerrainRequest, pub spawns: SpawnSpec,
}

impl Request {
    pub fn spec(&self) -> ProcgenSpec {
        ProcgenSpec { seed: self.seed.0, items: self.items, dungeon: self.dungeon.clone(),
            terrain: TerrainSpec { width: self.terrain.width, height: self.terrain.height,
                frequency: self.terrain.frequency.get(), octaves: self.terrain.octaves,
                sea_level: self.terrain.sea_level.get() }, spawns: self.spawns.clone() }
    }
    pub fn capture(spec: &ProcgenSpec) -> Self {
        Self { seed: Hex64(spec.seed), items: spec.items, dungeon: spec.dungeon.clone(),
            terrain: TerrainRequest { width: spec.terrain.width, height: spec.terrain.height,
                frequency: FloatBits::from_float(spec.terrain.frequency), octaves: spec.terrain.octaves,
                sea_level: FloatBits::from_float(spec.terrain.sea_level) }, spawns: spec.spawns.clone() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stat { pub stat: StatId, pub value: FloatBits }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartDefinition {
    pub id: String, pub slot: PartSlot, pub name: String, pub weight: u32,
    pub min_rarity: Rarity, pub bonuses: Vec<Stat>,
}

impl PartDefinition {
    fn capture(part: &Part) -> Self {
        Self { id: part.id.into(), slot: part.slot, name: part.name.into(), weight: part.weight,
            min_rarity: part.min_rarity, bonuses: stats(part.bonuses) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: Hex64, pub name: String, pub kind: ItemKind, pub rarity: Rarity,
    pub level: u32, pub stats: Vec<Stat>, pub part_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terrain {
    pub width: u32, pub height: u32, pub heights: Vec<FloatBits>,
    pub moisture: Vec<FloatBits>, pub biomes: Vec<Biome>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActorKind {
    Enemy { archetype: String, boss: bool },
    Npc { role: NpcRole },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub domain: SpawnDomain, pub x: u32, pub y: u32, pub kind: ActorKind,
    pub level: u32, pub encounter: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub schema_version: String,
    pub generator_version: String,
    pub source_identity: BuildIdentity,
    pub catalog_identity: CatalogIdentity,
    pub request: Request,
    /// Generator v2 has a fixed effective item center level of ten.
    pub effective_item_level: u32,
    pub part_definitions: Vec<PartDefinition>,
    pub items: Vec<Item>,
    pub dungeon: Dungeon,
    pub terrain: Terrain,
    pub spawns: Vec<Actor>,
    pub manifest_hash: Hex64,
}

fn catalog() -> Vec<PartDefinition> {
    item::default_catalog().iter().flat_map(|(_, pool)| pool.iter())
        .map(PartDefinition::capture).collect()
}

fn catalog_identity(parts: &[PartDefinition]) -> CatalogIdentity {
    let mut h = crate::ManifestHash(0xcbf2_9ce4_8422_2325);
    h.string(CATALOG_VERSION);
    h.len(parts.len());
    for p in parts {
        h.string(&p.id); h.byte(slot_tag(p.slot)); h.string(&p.name); h.u32(p.weight);
        h.rarity(p.min_rarity); h.len(p.bonuses.len());
        for s in &p.bonuses { h.byte(s.stat.discriminant_byte()); h.u32(s.value.0); }
    }
    CatalogIdentity { version: CATALOG_VERSION.into(), fingerprint: Hex64(h.0) }
}

fn slot_tag(slot: PartSlot) -> u8 {
    match slot { PartSlot::Body => 1, PartSlot::Grip => 2, PartSlot::Core => 3,
        PartSlot::Adornment => 4, PartSlot::Material => 5 }
}
fn stats(values: &[(StatId, f32)]) -> Vec<Stat> {
    values.iter().map(|&(stat, value)| Stat { stat, value: FloatBits::from_float(value) }).collect()
}
fn hex(s: &str) -> bool { s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
pub(crate) fn ensure(ok: bool, field: &'static str, why: &str) -> Result<(), ValidationError> {
    if ok { Ok(()) } else { Err(ValidationError::new(field, why)) }
}

impl Candidate {
    /// Capture a generated output without borrowing its static catalog or strings.
    pub fn capture(spec: &ProcgenSpec, output: &ProcgenOutput, source: BuildIdentity) -> Result<Self, ValidationError> {
        spec.validate()?;
        source.validate()?;
        ensure(output.generator_version == GENERATOR_VERSION && output.seed == spec.seed,
            "artifact.identity", "output does not match generator/request")?;
        ensure(output.items.len() == spec.items as usize && output.spawns.len() <= 100_000
            && output.dungeon.width == spec.dungeon.width && output.dungeon.height == spec.dungeon.height
            && output.dungeon.tiles.len() as u64 == u64::from(spec.dungeon.width) * u64::from(spec.dungeon.height)
            && output.dungeon.rooms.len() <= spec.dungeon.target_rooms as usize
            && output.dungeon.rooms.iter().all(|r| r.connections.len() <= 1023)
            && output.terrain.width == spec.terrain.width && output.terrain.height == spec.terrain.height
            && output.terrain.heights.len() as u64 == u64::from(spec.terrain.width) * u64::from(spec.terrain.height)
            && output.terrain.moisture.len() == output.terrain.heights.len()
            && output.terrain.biomes.len() == output.terrain.heights.len(),
            "artifact.capture", "output exceeds request/shape budgets; refusing before copying")?;
        for i in &output.items {
            ensure(i.name.len() <= 512 && i.stats.len() <= 8 && i.parts.len() == 5,
                "artifact.capture", "item exceeds capture budget")?;
            for part in &i.parts {
                ensure(part.id.len() <= 128 && part.name.len() <= 512 && part.bonuses.len() <= 8,
                    "artifact.capture", "part exceeds capture budget")?;
                let supported = item::default_catalog().iter().flat_map(|(_, p)| p.iter()).find(|p| p.id == part.id)
                    .ok_or_else(|| ValidationError::new("artifact.catalog", "output contains unsupported part"))?;
                ensure(PartDefinition::capture(part) == PartDefinition::capture(supported),
                    "artifact.catalog", "output part definition differs from supported catalog")?;
            }
        }
        ensure(output.spawns.iter().all(|s| match s.kind {
            SpawnKind::Enemy { archetype, .. } => archetype.len() <= 128,
            SpawnKind::Npc { .. } => true,
        }), "artifact.capture", "actor archetype exceeds capture budget")?;
        let parts = catalog();
        let candidate = Self {
            schema_version: SCHEMA.into(), generator_version: GENERATOR_VERSION.into(),
            source_identity: source.clone(), catalog_identity: catalog_identity(&parts),
            request: Request::capture(spec), effective_item_level: 10, part_definitions: parts,
            items: output.items.iter().map(|i| Item { id: Hex64(i.id), name: i.name.clone(),
                kind: i.kind, rarity: i.rarity, level: i.level, stats: stats(&i.stats),
                part_ids: i.parts.iter().map(|p| p.id.into()).collect() }).collect(),
            dungeon: output.dungeon.clone(),
            terrain: Terrain { width: output.terrain.width, height: output.terrain.height,
                heights: output.terrain.heights.iter().map(|&v| FloatBits::from_float(v)).collect(),
                moisture: output.terrain.moisture.iter().map(|&v| FloatBits::from_float(v)).collect(),
                biomes: output.terrain.biomes.clone() },
            spawns: output.spawns.iter().map(|s| Actor { domain: s.domain, x: s.position.0, y: s.position.1,
                kind: match s.kind { SpawnKind::Enemy { archetype, boss } => ActorKind::Enemy { archetype: archetype.into(), boss },
                    SpawnKind::Npc { role } => ActorKind::Npc { role } }, level: s.level, encounter: s.encounter }).collect(),
            manifest_hash: Hex64(output.manifest_hash),
        };
        candidate.restore(&source)?;
        Ok(candidate)
    }

    pub fn decode(bytes: &[u8], supported_build: &BuildIdentity) -> Result<Self, ValidationError> {
        let candidate: Self = crate::json::decode(bytes)?;
        candidate.restore(supported_build)?;
        Ok(candidate)
    }

    pub fn encode(&self, supported_build: &BuildIdentity) -> Result<Vec<u8>, ValidationError> {
        self.restore(supported_build)?;
        crate::json::encode(self)
    }

    /// The caller must pin a retained, supported build; metadata is not self-authenticating.
    /// FNV detects inconsistency, not malicious forgery. No generator or runtime is executed.
    pub fn restore(&self, supported_build: &BuildIdentity) -> Result<ProcgenOutput, ValidationError> {
        ensure(self.schema_version == SCHEMA, "artifact.schema", "unsupported candidate schema")?;
        ensure(self.generator_version == GENERATOR_VERSION, "artifact.generator", "unsupported generator version")?;
        self.source_identity.validate()?;
        supported_build.validate()?;
        ensure(&self.source_identity == supported_build, "artifact.build", "candidate does not match pinned supported source archive")?;
        let spec = self.request.spec();
        spec.validate()?;
        ensure(self.effective_item_level == 10, "artifact.item_level", "generator v2 item center is ten")?;
        let parts = catalog();
        ensure(self.part_definitions == parts && self.catalog_identity == catalog_identity(&parts),
            "artifact.catalog", "unsupported or altered full part catalog")?;
        ensure(self.items.len() == spec.items as usize && self.items.len() <= 10_000,
            "artifact.items", "item count must match bounded request")?;
        crate::validation::validate_world(self, &spec)?;
        let mut items = Vec::with_capacity(self.items.len());
        let mut item_ids = std::collections::HashSet::new();
        for i in &self.items {
            ensure(item_ids.insert(i.id), "artifact.item_id", "duplicate item identity")?;
            ensure(!i.name.is_empty() && i.name.len() <= 512 && (1..=80).contains(&i.level), "artifact.item", "invalid item name/level")?;
            ensure(i.part_ids.len() == 5 && i.stats.len() <= 8, "artifact.item", "five ordered parts and at most eight distinct stats required")?;
            let mut seen = std::collections::HashSet::new();
            ensure(i.stats.iter().all(|s| s.value.get().is_finite() && seen.insert(s.stat)), "artifact.stats", "non-finite or repeated item stat")?;
            let mut references = Vec::with_capacity(5);
            for (index, id) in i.part_ids.iter().enumerate() {
                let part = item::default_catalog().iter().flat_map(|(_, p)| p.iter()).find(|p| p.id == id)
                    .ok_or_else(|| ValidationError::new("artifact.part", "unknown part identifier"))?;
                ensure(slot_tag(part.slot) as usize == index + 1 && part.min_rarity <= i.rarity,
                    "artifact.part", "part slot order or rarity floor mismatch")?;
                references.push(part);
            }
            items.push(GeneratedItem { id: i.id.0, name: i.name.clone(), kind: i.kind,
                rarity: i.rarity, level: i.level, stats: i.stats.iter().map(|s| (s.stat, s.value.get())).collect(), parts: references });
        }
        let terrain = TerrainField { width: self.terrain.width, height: self.terrain.height,
            heights: self.terrain.heights.iter().map(|b| b.get()).collect(),
            moisture: self.terrain.moisture.iter().map(|b| b.get()).collect(), biomes: self.terrain.biomes.clone() };
        let spawns = self.spawns.iter().map(|s| Ok(Spawn { domain: s.domain, position: (s.x, s.y),
            kind: match &s.kind { ActorKind::Enemy { archetype, boss } => SpawnKind::Enemy { archetype: supported_archetype(archetype)?, boss: *boss },
                ActorKind::Npc { role } => SpawnKind::Npc { role: *role } }, level: s.level, encounter: s.encounter }))
            .collect::<Result<Vec<_>, ValidationError>>()?;
        let fingerprint = crate::manifest_hash(&spec, &items, &self.dungeon, &terrain, &spawns);
        ensure(fingerprint == self.manifest_hash.0, "artifact.fingerprint", "complete generator-v2 manifest mismatch")?;
        Ok(ProcgenOutput { generator_version: GENERATOR_VERSION, seed: spec.seed, items,
            dungeon: self.dungeon.clone(), terrain, spawns, manifest_hash: fingerprint })
    }
}

pub(crate) fn supported_archetype(name: &str) -> Result<&'static str, ValidationError> {
    match name {
        "patrol" => Ok("patrol"), "ambush" => Ok("ambush"), "cultists" => Ok("cultists"),
        "beasts" => Ok("beasts"), "undead" => Ok("undead"), "elemental" => Ok("elemental"),
        "boss" => Ok("boss"), "guard" => Ok("guard"),
        _ => Err(ValidationError::new("artifact.archetype", "unsupported actor archetype")),
    }
}
