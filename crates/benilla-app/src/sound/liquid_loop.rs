//! The above-water liquid ambient loop, as 3.3.5 drives it (`SI2::HandleWaterSounds`, `0x4D2390`):
//! one loop only, at the nearest liquid point within 60 yd of the player, at that liquid's
//! `SoundEntries` kit. The reference keeps a single record (`s_nearestWaterSounds[1]` /
//! `s_waterSounds[1]`), queries `World::QueryLiquidSounds(entity, 60.0f)` at most every 500 ms and
//! only after the player moved (`_OkToQueryLiquidSounds`, `0x4D2230`), fades in and out over 1.0 s
//! (`FADEINTIME`/`FADEOUTTIME`), and stops hard when the listener is underwater
//! (`SI2::IsUnderwater`).
//!
//! Deviation: the 3.3.5 kit is the liquid type row's own `m_soundID`, while this client carries no
//! `LiquidType.dbc` per wet cell and resolves the cell's sound nibble through `SoundWaterType.dbc`
//! instead (the donor table, which names the same River/Ocean/Magma/Slime loops). A liquid flagged
//! `m_flags & 0x80` and inside a map object is skipped by the reference; that flag is not in our
//! liquid data either, so an indoor pool whose cell carries a loop will still be heard.
//!
//! The reference's own `MapWaterSounds` switch (registered at `0x462a40` in 1.12) is not built; the
//! loop rides the ambience bucket, so `EnableAmbience` and the ambience slider gate it.

use bevy::prelude::*;

use benilla_assets::coords::{bevy_to_wow, wow_to_bevy};
use benilla_formats::WaterSoundCatalog;

use crate::net::Embodied;
use benilla_assets::{AssetSet, LockRecover, WorldAssets};
use benilla_world::schedule::WorldStage;

use super::kit::{
    self, play_kit_ext, set_source_kit_gain, source_kit_playing, stop_source_kit, KitRef,
    PlayExtras, SoundCategory, SoundKits,
};
use super::{SoundConfig, SoundOutput};

/// The scan radius (yd) around the player: `World::QueryLiquidSounds(entity, 60.0f)`.
const TRIGGER_RADIUS: f32 = 60.0;
/// Fade in and out (s): `FADEINTIME` / `FADEOUTTIME` in `SoundInterface2Water.cpp`.
const FADE_SECS: f32 = 1.0;
/// The query throttle (s): `_OkToQueryLiquidSounds` (`0x4D2230`) scans at most every 500 ms, and
/// only when the player has moved since the last scan.
const QUERY_INTERVAL_SECS: f32 = 0.5;

/// The `SoundWaterType.dbc` class → kit map.
#[derive(Resource)]
pub(super) struct WaterSounds(WaterSoundCatalog);

/// The one loop the reference keeps (`s_waterSounds[1]`): the nearest liquid point's bed.
struct ClassLoop {
    /// The slewed emitter; the pump's tracked-follow reads its `Transform`.
    emitter: Entity,
    kit: u32,
    /// The pump-lane gain, 0 → 1 on arm and 1 → 0 on leave; the channel stops at 0.
    gain: f32,
    /// A superseded kit (the nearest cell's speed nibble changed) fading out on the same
    /// emitter: `(kit, gain)`.
    retiring: Option<(u32, f32)>,
    /// The last 500 ms scan found this kit in range: the fade target, so a stationary player keeps
    /// the bed without re-scanning (`_OkToQueryLiquidSounds`).
    wet: bool,
}

/// The loop slot, the submerge edge latch and the 500 ms query gate.
#[derive(Resource)]
struct LiquidLoopState {
    live: Option<ClassLoop>,
    was_underwater: bool,
    /// `s_lastPosition`: the player position at the last scan; an unchanged one skips the scan.
    last_query: Option<[f32; 3]>,
    /// `s_timeSinceQuery`, in seconds.
    since_query: f32,
}

impl Default for LiquidLoopState {
    fn default() -> Self {
        Self {
            live: None,
            was_underwater: false,
            last_query: None,
            // `s_timeSinceQuery` starts at 999999: the first scan is not throttled.
            since_query: QUERY_INTERVAL_SECS,
        }
    }
}

fn load_water_sounds(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = {
        let mut chain = assets.chain.lock_recover();
        benilla_formats::load_water_sound_catalog(&mut chain)
    };
    match loaded {
        Ok(cat) => {
            info!("sound: {} SoundWaterType rows", cat.len());
            commands.insert_resource(WaterSounds(cat));
        }
        Err(e) => warn!("sound: SoundWaterType failed to load: {e:#}"),
    }
}

/// `_OkToQueryLiquidSounds` (`0x4D2230`): the 500 ms throttle and the moved-player gate.
fn query_due(since_query: f32, moved: bool) -> bool {
    since_query >= QUERY_INTERVAL_SECS && moved
}

/// The per-frame gain step: `FADE_SECS` to cross the whole range.
fn fade_step(dt: f32) -> f32 {
    if FADE_SECS > 0.0 {
        dt / FADE_SECS
    } else {
        1.0
    }
}

/// The per-frame driver: the submerge edge, the 500 ms scan, arm/move/retire, and the fades.
#[allow(clippy::too_many_arguments)]
fn drive_liquid_loops(
    mut state: ResMut<LiquidLoopState>,
    water_sounds: Option<Res<WaterSounds>>,
    world: benilla_world::world_point::WorldPoint,
    player: Query<&Transform, With<Embodied>>,
    mut emitters: Query<&mut Transform, Without<Embodied>>,
    time: Res<Time>,
    kits: Option<ResMut<SoundKits>>,
    assets: Option<Res<WorldAssets>>,
    mut out: NonSendMut<SoundOutput>,
    config: Res<SoundConfig>,
    listener: Res<super::AudioListener>,
    mut commands: Commands,
) {
    let (Some(water_sounds), Some(mut kits), Some(assets)) = (water_sounds, kits, assets) else {
        return;
    };
    // No audio device: a loop that fails to start is never held, so it would retry and warn every
    // frame. The startup warning already covers it.
    if out.mixer.is_none() {
        return;
    }
    let dt = time.delta_secs();
    let step = fade_step(dt);

    // Submerged: a hard stop with no fade (`SI2::IsUnderwater` short-circuits the driver), and the
    // resurface edge restarts the bed at full.
    if world.submersion().is_water() {
        if let Some(cl) = state.live.take() {
            stop_source_kit(&mut out, cl.emitter, cl.kit);
            if let Some((old, _)) = cl.retiring {
                stop_source_kit(&mut out, cl.emitter, old);
            }
            // The 1.12 driver left this emitter alive on every dive; the loop owns it.
            commands.entity(cl.emitter).despawn();
        }
        state.was_underwater = true;
        return;
    }
    let resurfaced = std::mem::take(&mut state.was_underwater);

    let Ok(player_tf) = player.single() else {
        return; // no avatar yet
    };
    let player_pos = player_tf.translation;
    let player_wow = bevy_to_wow(player_pos);

    // The scan: throttled and gated on movement, exactly as `_OkToQueryLiquidSounds` gates
    // `World::QueryLiquidSounds`. A held frame keeps the current bed and position.
    state.since_query += dt;
    let moved = state.last_query != Some(player_wow);
    if query_due(state.since_query, moved) {
        state.since_query = 0.0;
        state.last_query = Some(player_wow);
        let candidate = world.nearest_liquid(player_wow, TRIGGER_RADIUS);
        let desired = candidate
            .as_ref()
            .and_then(|c| water_sounds.0.kit_for_nibble(c.nibble));
        match (state.live.take(), desired) {
            (None, Some(kit_id)) => {
                // Arm at the nearest wet point, at full gain on the resurface edge.
                let point = candidate.expect("a kit implies a candidate").point;
                let pos = wow_to_bevy(point);
                let emitter = commands.spawn((Transform::from_translation(pos),)).id();
                let gain = if resurfaced { 1.0 } else { 0.0 };
                start_loop(
                    &mut kits,
                    &assets,
                    &mut out,
                    &config,
                    listener.pos,
                    kit_id,
                    pos,
                    emitter,
                    gain,
                );
                state.live = Some(ClassLoop {
                    emitter,
                    kit: kit_id,
                    gain,
                    retiring: None,
                    wet: true,
                });
            }
            (Some(mut cl), Some(kit_id)) => {
                if cl.kit != kit_id {
                    // The speed nibble changed (still → fast river): the old kit fades out on
                    // the same emitter while the new one fades in.
                    if let Some((old, _)) = cl.retiring.take() {
                        // A second swap mid-fade drops the oldest outright.
                        stop_source_kit(&mut out, cl.emitter, old);
                    }
                    cl.retiring = Some((cl.kit, cl.gain));
                    cl.kit = kit_id;
                    cl.gain = 0.0;
                }
                // `SI2::Set3DPosition`: the bed follows the nearest wet point.
                let point = candidate.expect("a kit implies a candidate").point;
                if let Ok(mut tf) = emitters.get_mut(cl.emitter) {
                    tf.translation = wow_to_bevy(point);
                }
                cl.wet = true;
                state.live = Some(cl);
            }
            (Some(mut cl), None) => {
                // Nothing wet in range: fade out, then stop.
                cl.wet = false;
                state.live = Some(cl);
            }
            (None, None) => {}
        }
    }

    // The fades advance every frame, whatever the scan did: the backend ramps them, not the
    // query. A kit whose channel the device dropped is restarted in place.
    let Some(mut cl) = state.live.take() else {
        return;
    };
    if !cl.wet {
        cl.gain -= step;
    } else if resurfaced {
        cl.gain = 1.0;
    } else {
        cl.gain = (cl.gain + step).min(1.0);
    }
    if cl.gain <= 0.0 {
        stop_source_kit(&mut out, cl.emitter, cl.kit);
        if let Some((old, _)) = cl.retiring.take() {
            stop_source_kit(&mut out, cl.emitter, old);
        }
        commands.entity(cl.emitter).despawn();
    } else {
        if !source_kit_playing(&out, cl.emitter, cl.kit) {
            let pos = emitters
                .get(cl.emitter)
                .map(|t| t.translation)
                .unwrap_or(player_pos);
            start_loop(
                &mut kits,
                &assets,
                &mut out,
                &config,
                listener.pos,
                cl.kit,
                pos,
                cl.emitter,
                cl.gain,
            );
        }
        set_source_kit_gain(&mut out, cl.emitter, cl.kit, cl.gain);
        if let Some((old, mut g)) = cl.retiring.take() {
            g -= step;
            if g <= 0.0 {
                stop_source_kit(&mut out, cl.emitter, old);
            } else {
                set_source_kit_gain(&mut out, cl.emitter, old, g);
                cl.retiring = Some((old, g));
            }
        }
        state.live = Some(cl);
    }
}

/// Start one bed on the ambience bucket, tagged to its emitter, at an initial gain.
/// Force-looped: the type-22 kits are all authored loops but the lava pool's lacks the `0x200`
/// flag; how the reference loops that one is untraced.
#[allow(clippy::too_many_arguments)]
fn start_loop(
    kits: &mut SoundKits,
    assets: &WorldAssets,
    out: &mut SoundOutput,
    config: &SoundConfig,
    listener: Vec3,
    kit_id: u32,
    pos: Vec3,
    emitter: Entity,
    gain: f32,
) {
    if let Err(e) = play_kit_ext(
        kits,
        assets,
        out,
        config,
        listener,
        KitRef::Id(kit_id),
        Some(pos),
        SoundCategory::Ambience,
        PlayExtras {
            source: Some(emitter),
            force_loop: true,
            ..default()
        },
    ) {
        warn!("liquid loop kit {kit_id}: {e:#}");
        return;
    }
    set_source_kit_gain(out, emitter, kit_id, gain);
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<LiquidLoopState>()
        .add_systems(Startup, load_water_sounds.after(AssetSet::Open))
        .add_systems(
            Update,
            // Before the channel pump, which applies this frame's gains and positions.
            drive_liquid_loops
                .in_set(WorldStage::Present)
                .before(kit::pump_channels),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_OkToQueryLiquidSounds`: 500 ms between scans, and none at all while the player stands
    /// still, however long the frame gap.
    #[test]
    fn the_water_scan_waits_half_a_second_and_a_moved_player() {
        assert!(!query_due(0.0, true), "a scan on the same frame is throttled");
        assert!(!query_due(0.49, true), "still inside the 500 ms window");
        assert!(query_due(0.5, true), "half a second and a step: scan");
        assert!(query_due(9.0, true));
        assert!(!query_due(9.0, false), "a standing player is never re-scanned");
    }

    /// `FADEINTIME`/`FADEOUTTIME` are 1.0 s: a 0.25 s frame moves a quarter of the range.
    #[test]
    fn the_bed_fades_over_one_second() {
        assert!((fade_step(0.25) - 0.25).abs() < 1e-6);
        assert!((fade_step(1.0) - 1.0).abs() < 1e-6);
        assert!(fade_step(0.0).abs() < 1e-6);
    }
}
