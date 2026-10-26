//! Encounter population: enemy packs and NPCs placed over a dungeon's rooms or a terrain
//! field's biomes. Constraints that make a layout *playable* rather than merely random:
//! the entrance is enemy-free, the boss room holds exactly one boss (with adds), treasure
//! rooms are guarded, and levels progress with distance from the entrance up to level 80.
//! A single-room dungeon stays a safe entrance with no boss.

use crate::dungeon::{Dungeon, RoomArchetype};
use crate::rng::ProcRng;
use crate::terrain::{Biome, TerrainField};
use crate::ValidationError;

/// The fork domain for the spawn stream.
pub const DOMAIN: u64 = 0x5350_4157_4e53_0004; // "SPAWNS"

/// What stands at a spawn point.
#[derive(Clone, Debug, PartialEq)]
pub enum SpawnKind {
    /// An enemy: pack archetype and level.
    Enemy { archetype: &'static str, boss: bool },
    /// A friendly: vendor / quest giver / flavor.
    Npc { role: NpcRole },
}

impl SpawnKind {
    pub fn discriminant_byte(&self) -> u8 {
        match self {
            SpawnKind::Enemy { boss: false, .. } => 1,
            SpawnKind::Enemy { boss: true, .. } => 2,
            SpawnKind::Npc { .. } => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcRole {
    Vendor,
    QuestGiver,
    Innkeeper,
}

/// Which coordinate space a spawn lives in. Dungeon spawns are tile coordinates inside the
/// generated map; overworld spawns are sample coordinates on the terrain field. The two
/// spaces are independent — consumers must never compare across them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnDomain {
    Dungeon,
    Overworld,
}

/// One placed actor.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub domain: SpawnDomain,
    pub position: (u32, u32),
    pub kind: SpawnKind,
    pub level: u32,
    /// Which encounter group this actor belongs to (packs share an id).
    pub encounter: u32,
}

/// Population parameters.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnSpec {
    /// Base enemy level at the entrance; deeper is higher.
    pub base_level: u32,
    /// Enemy packs per normal room (max).
    pub packs_per_room: u32,
    /// Terrain scatter density: attempts per 100 samples.
    pub terrain_density_per_100: u32,
}

impl Default for SpawnSpec {
    fn default() -> Self {
        Self { base_level: 5, packs_per_room: 2, terrain_density_per_100: 1 }
    }
}

impl SpawnSpec {
    /// Bound levels, pack sizes and total potential output before populating either domain.
    pub fn validate(&self, rooms: usize, terrain_samples: u64) -> Result<(), ValidationError> {
        if !(1..=80).contains(&self.base_level) {
            return Err(ValidationError::new("spawns.base_level", "expected 1..=80"));
        }
        if self.packs_per_room > 16 || self.terrain_density_per_100 > 100 {
            return Err(ValidationError::new("spawns.density", "at most 16 packs per room and 100 attempts per 100 samples"));
        }
        let dungeon_limit = (rooms as u64).checked_mul(u64::from(self.packs_per_room) * 4 + 3);
        let terrain_limit = terrain_samples.checked_mul(u64::from(self.terrain_density_per_100))
            .map(|n| n / 100);
        if dungeon_limit.zip(terrain_limit).and_then(|(a, b)| a.checked_add(b))
            .is_none_or(|n| n > 100_000)
        {
            return Err(ValidationError::new("spawns.count", "maximum 100000 potential actors"));
        }
        Ok(())
    }
}

/// Weighted enemy pack archetypes: flavor families a consumer maps to creature templates.
const PACKS: [(&str, u32); 6] = [
    ("patrol", 30),
    ("ambush", 22),
    ("cultists", 18),
    ("beasts", 16),
    ("undead", 10),
    ("elemental", 4),
];

/// Populate the dungeon (rooms + graph distance) and the terrain field (biome scatter).
/// Both outputs share one encounter-id space so consumers can treat them uniformly.
pub fn populate(rng: &mut ProcRng, spec: &SpawnSpec, dungeon: &Dungeon, terrain: &TerrainField) -> Result<Vec<Spawn>, ValidationError> {
    let samples = u64::from(terrain.width) * u64::from(terrain.height);
    spec.validate(dungeon.rooms.len(), samples)?;
    if samples == 0 || samples > 1_048_576
        || terrain.biomes.len() as u64 != samples
        || terrain.heights.len() as u64 != samples
        || terrain.moisture.len() as u64 != samples
    {
        return Err(ValidationError::new("spawns.terrain", "expected a nonempty generated terrain field"));
    }
    if dungeon.rooms.is_empty() || dungeon.rooms.len() > 1024
        || dungeon.entrance >= dungeon.rooms.len() || dungeon.boss_room >= dungeon.rooms.len()
        || dungeon.width == 0 || dungeon.height == 0
        || dungeon.width > 2048 || dungeon.height > 2048
        || u64::from(dungeon.width) * u64::from(dungeon.height) > 1_048_576
        || u64::from(dungeon.width) * u64::from(dungeon.height) != dungeon.tiles.len() as u64
        || dungeon.rooms.iter().enumerate().any(|(i, room)| {
            room.id != i || room.w == 0 || room.h == 0
                || u64::from(room.x) + u64::from(room.w) > u64::from(dungeon.width)
                || u64::from(room.y) + u64::from(room.h) > u64::from(dungeon.height)
                || room.connections.iter().any(|&next| next >= dungeon.rooms.len())
        })
    {
        return Err(ValidationError::new("spawns.dungeon", "expected bounded rooms and a valid generated room graph"));
    }
    let mut spawns = Vec::new();
    let mut next_encounter = 0u32;

    // --- dungeon rooms, by archetype and depth ---
    // Graph distances from the entrance (same BFS as generation).
    let distances = bfs(dungeon.rooms.len(), dungeon.entrance, &dungeon.rooms);
    if distances.contains(&u32::MAX) {
        return Err(ValidationError::new("spawns.dungeon", "room graph must be reachable from the entrance"));
    }
    for room in &dungeon.rooms {
        let tiles = dungeon.room_tiles(room);
        if tiles.is_empty() {
            continue;
        }
        let depth = distances[room.id];
        let level = (spec.base_level + depth).min(80);
        match room.archetype {
            RoomArchetype::Entrance => {
                // Safe by contract: one vendor, one quest giver, never an enemy.
                let (vx, vy) = *rng.choose(&tiles);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (vx, vy), kind: SpawnKind::Npc { role: NpcRole::Vendor }, level: 1.max(spec.base_level), encounter: next_encounter });
                next_encounter += 1;
                let (qx, qy) = *rng.choose(&tiles);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (qx, qy), kind: SpawnKind::Npc { role: NpcRole::QuestGiver }, level: 1.max(spec.base_level), encounter: next_encounter });
                next_encounter += 1;
            }
            RoomArchetype::Boss => {
                // Exactly one boss at the room's center, two adds at random tiles.
                let (bx, by) = room.center();
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (bx, by), kind: SpawnKind::Enemy { archetype: "boss", boss: true }, level: (level + 3).min(80), encounter: next_encounter });
                next_encounter += 1;
                let encounter = next_encounter;
                for _ in 0..2 {
                    let (x, y) = *rng.choose(&tiles);
                    spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: "guard", boss: false }, level: (level + 1).min(80), encounter });
                }
                next_encounter += 1;
            }
            RoomArchetype::Treasure => {
                // Guarded: one pack, one level hotter than the norm.
                let (x, y) = *rng.choose(&tiles);
                let pack = *rng.weighted(&PACKS);
                spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: pack, boss: false }, level: (level + 1).min(80), encounter: next_encounter });
                next_encounter += 1;
            }
            RoomArchetype::Normal => {
                let pack_count = if spec.packs_per_room == 0 {
                    0
                } else {
                    rng.int(1, spec.packs_per_room as i64) as u32
                };
                for _ in 0..pack_count {
                    let pack = *rng.weighted(&PACKS);
                    let encounter = next_encounter;
                    let size = rng.int(2, 4) as u32;
                    for _ in 0..size {
                        let (x, y) = *rng.choose(&tiles);
                        spawns.push(Spawn { domain: SpawnDomain::Dungeon, position: (x, y), kind: SpawnKind::Enemy { archetype: pack, boss: false }, level, encounter });
                    }
                    next_encounter += 1;
                }
            }
        }
    }

    // --- terrain scatter: biome-appropriate families, level by distance from the field's
    //     center (the assumed player start), skipping ocean tiles ---
    let center = (terrain.width as i64 / 2, terrain.height as i64 / 2);
    let attempts = samples * u64::from(spec.terrain_density_per_100) / 100;
    for _ in 0..attempts {
        let x = rng.int(0, terrain.width as i64 - 1) as u32;
        let y = rng.int(0, terrain.height as i64 - 1) as u32;
        let biome = terrain.biome_at(x, y);
        if biome == Biome::Ocean {
            continue;
        }
        let family = match biome {
            Biome::Desert => "beasts",
            Biome::Forest => "beasts",
            Biome::Plains => "patrol",
            Biome::Hills => "undead",
            Biome::Mountain | Biome::Snow => "elemental",
            _ => "patrol",
        };
        let distance = ((x as i64 - center.0).abs() + (y as i64 - center.1).abs()) / 16;
        let level = (spec.base_level + distance as u32).clamp(1, 80);
        spawns.push(Spawn { domain: SpawnDomain::Overworld, position: (x, y), kind: SpawnKind::Enemy { archetype: family, boss: false }, level, encounter: next_encounter });
        next_encounter += 1;
    }

    Ok(spawns)
}

fn bfs(count: usize, from: usize, rooms: &[crate::dungeon::Room]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; count];
    dist[from] = 0;
    // FIFO discovery gives shortest distances even when loop corridors are present.
    let mut frontier = std::collections::VecDeque::from([from]);
    while let Some(current) = frontier.pop_front() {
        for &next in &rooms[current].connections {
            if dist[next] == u32::MAX {
                dist[next] = dist[current] + 1;
                frontier.push_back(next);
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::{generate as gen_dungeon, DungeonSpec};
    use crate::terrain::{generate as gen_terrain, TerrainSpec};

    fn world(seed: u64) -> (Dungeon, TerrainField, Vec<Spawn>) {
        let rng = ProcRng::new(seed);
        let dungeon = gen_dungeon(&mut rng.fork(crate::dungeon::DOMAIN), &DungeonSpec::default()).expect("valid dungeon");
        let terrain = gen_terrain(&mut rng.fork(crate::terrain::DOMAIN), &TerrainSpec::default()).expect("valid terrain");
        let spawns = populate(&mut rng.fork(DOMAIN), &SpawnSpec::default(), &dungeon, &terrain).expect("valid population");
        (dungeon, terrain, spawns)
    }

    #[test]
    fn entrance_safe_boss_unique_levels_progress() {
        for seed in 0..10 {
            let (dungeon, _, spawns) = world(seed);
            let entrance_tiles: Vec<(u32, u32)> = dungeon.room_tiles(&dungeon.rooms[dungeon.entrance]);
            // No dungeon-domain enemy stands in the entrance room (overworld spawns live in
            // a different coordinate space and are excluded by domain).
            assert!(
                spawns.iter().all(|s| s.domain != SpawnDomain::Dungeon
                    || !matches!(s.kind, SpawnKind::Enemy { .. })
                    || !entrance_tiles.contains(&s.position)),
                "seed {seed}: enemy in the entrance room"
            );
            // Exactly one boss, and it is in the boss room.
            let bosses: Vec<&Spawn> = spawns
                .iter()
                .filter(|s| matches!(s.kind, SpawnKind::Enemy { boss: true, .. }))
                .collect();
            assert_eq!(bosses.len(), 1, "seed {seed}: boss count {}", bosses.len());
            assert!(dungeon.rooms[dungeon.boss_room].contains(bosses[0].position.0, bosses[0].position.1));
            // Level progression: boss level exceeds the base.
            assert!(bosses[0].level > 5);
        }
    }

    #[test]
    fn same_seed_same_population() {
        assert_eq!(world(42).2, world(42).2);
    }

    #[test]
    fn npcs_present_and_enemies_leveled() {
        let (_, _, spawns) = world(42);
        assert!(spawns.iter().any(|s| matches!(s.kind, SpawnKind::Npc { .. })));
        assert!(spawns.iter().filter(|s| matches!(s.kind, SpawnKind::Enemy { .. })).all(|s| s.level >= 1 && s.level <= 80));
    }
}
