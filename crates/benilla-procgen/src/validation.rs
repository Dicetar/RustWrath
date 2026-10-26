//! Validate imported shapes before using helpers that assume generated, bounded data.

use std::collections::{HashSet, VecDeque};
use crate::artifact::{ensure, ActorKind, Candidate};
use crate::dungeon::RoomArchetype;
use crate::spawns::SpawnDomain;
use crate::terrain::Biome;
use crate::{ProcgenSpec, ValidationError};

pub(crate) fn validate_world(c: &Candidate, spec: &ProcgenSpec) -> Result<(), ValidationError> {
    let d = &c.dungeon;
    ensure(d.width == spec.dungeon.width && d.height == spec.dungeon.height
        && d.tiles.len() as u64 == u64::from(d.width) * u64::from(d.height),
        "artifact.dungeon", "tile dimensions/length must match request")?;
    ensure(!d.rooms.is_empty() && d.rooms.len() <= spec.dungeon.target_rooms as usize
        && d.entrance < d.rooms.len() && d.boss_room < d.rooms.len(),
        "artifact.rooms", "invalid room count or entrance/boss reference")?;
    let mut edges = 0usize;
    let mut room_area = 0u64;
    let mut room_cells = vec![false; d.tiles.len()];
    for (index, room) in d.rooms.iter().enumerate() {
        ensure(room.id == index && room.w > 0 && room.h > 0
            && u64::from(room.x) + u64::from(room.w) <= u64::from(d.width)
            && u64::from(room.y) + u64::from(room.h) <= u64::from(d.height),
            "artifact.room", "invalid room index or rectangle")?;
        ensure(room.connections.len() < d.rooms.len(), "artifact.graph", "too many room connections")?;
        edges += room.connections.len();
        ensure(edges <= 10_238, "artifact.graph", "adjacency budget exceeded")?;
        let mut seen = HashSet::new();
        for &next in &room.connections {
            ensure(next < d.rooms.len() && next != index && seen.insert(next),
                "artifact.graph", "out-of-range, self or duplicate connection")?;
        }
        room_area += u64::from(room.w) * u64::from(room.h);
        ensure(room_area <= d.tiles.len() as u64, "artifact.room", "room area exceeds tile budget")?;
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                let cell = (y * d.width + x) as usize;
                ensure(!room_cells[cell] && d.tiles[cell].walkable(),
                    "artifact.room", "rooms must be disjoint and fully walkable")?;
                room_cells[cell] = true;
            }
        }
    }
    for room in &d.rooms {
        ensure(room.connections.iter().all(|&next| d.rooms[next].connections.contains(&room.id)),
            "artifact.graph", "room connections must be reciprocal")?;
    }
    let mut reached = vec![false; d.rooms.len()];
    let mut queue = VecDeque::from([d.entrance]);
    reached[d.entrance] = true;
    while let Some(index) = queue.pop_front() {
        for &next in &d.rooms[index].connections {
            if !reached[next] { reached[next] = true; queue.push_back(next); }
        }
    }
    ensure(reached.iter().all(|v| *v) && d.all_reachable(), "artifact.connectivity", "disconnected room graph or walkable tiles")?;
    ensure(d.rooms[d.entrance].archetype == RoomArchetype::Entrance
        && d.rooms.iter().filter(|r| r.archetype == RoomArchetype::Entrance).count() == 1,
        "artifact.entrance", "exactly one safe entrance required")?;
    let boss_rooms = d.rooms.iter().filter(|r| r.archetype == RoomArchetype::Boss).count();
    ensure(if d.rooms.len() == 1 { d.boss_room == d.entrance && boss_rooms == 0 }
        else { d.boss_room != d.entrance && boss_rooms == 1 && d.rooms[d.boss_room].archetype == RoomArchetype::Boss },
        "artifact.boss", "boss room identity inconsistent with room count")?;

    let t = &c.terrain;
    let samples = u64::from(t.width) * u64::from(t.height);
    ensure(t.width == spec.terrain.width && t.height == spec.terrain.height
        && t.heights.len() as u64 == samples && t.moisture.len() as u64 == samples
        && t.biomes.len() as u64 == samples, "artifact.terrain", "terrain dimensions/arrays must match request")?;
    ensure(t.heights.iter().chain(t.moisture.iter()).all(|b| b.get().is_finite() && (0.0..=1.0).contains(&b.get())),
        "artifact.terrain", "terrain samples must be finite and normalized")?;
    ensure(c.spawns.len() <= 100_000, "artifact.spawns", "maximum 100000 actors")?;
    let mut bosses = 0;
    for s in &c.spawns {
        ensure((1..=80).contains(&s.level) && s.encounter < 100_000,
            "artifact.spawn", "invalid level or encounter identity")?;
        match s.domain {
            SpawnDomain::Dungeon => {
                ensure(s.x < d.width && s.y < d.height, "artifact.spawn", "dungeon coordinate out of bounds")?;
                ensure(d.tiles[(s.y * d.width + s.x) as usize].walkable(), "artifact.spawn", "actor on solid tile")?;
                let entrance = &d.rooms[d.entrance];
                if let ActorKind::Enemy { archetype, boss } = &s.kind {
                    ensure(*boss == (archetype == "boss"), "artifact.spawn", "boss flag/archetype mismatch")?;
                    ensure(!entrance.contains(s.x, s.y), "artifact.spawn", "enemy in safe entrance")?;
                    if *boss {
                        bosses += 1;
                        ensure(d.rooms[d.boss_room].contains(s.x, s.y), "artifact.spawn", "boss outside boss room")?;
                    }
                }
            }
            SpawnDomain::Overworld => {
                ensure(s.x < t.width && s.y < t.height, "artifact.spawn", "terrain coordinate out of bounds")?;
                ensure(t.biomes[(s.y * t.width + s.x) as usize] != Biome::Ocean
                    && matches!(&s.kind, ActorKind::Enemy { boss: false, .. }),
                    "artifact.spawn", "overworld actor must be a non-boss enemy on land")?;
            }
        }
    }
    ensure(bosses == if d.rooms.len() > 1 { 1 } else { 0 }, "artifact.spawn", "expected one boss in multi-room dungeon, none in single room")
}
