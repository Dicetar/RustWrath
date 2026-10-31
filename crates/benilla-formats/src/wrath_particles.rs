//! Build-12340 M2 particle records. Separate from vanilla's fixed three-key ramps.
//! Layout facts: pinned 12340 reference M2Data.h M2Particle and M2Shared.cpp validation.

use anyhow::{ensure, Context, Result};
use benilla_bytes::ByteExt;

const RECORD_SIZE: usize = 0x1dc;

/// A Wrath track retains one timeline per file sequence, including empty slots. Global tracks
/// also carry nested arrays on disk; `global_sequence` selects their clock, not their layout.
#[derive(Debug, Clone)]
pub struct ParticleTrack {
    pub interpolation: u16,
    pub global_sequence: u16,
    pub sequences: Vec<Vec<(u32, f32)>>,
}

/// Independent normalized-age curve. Values retain every component and authored key.
#[derive(Debug, Clone)]
pub struct ParticleCurve<const N: usize> {
    pub keys: Vec<(f32, [f32; N])>,
}

impl<const N: usize> ParticleCurve<N> {
    fn sample(&self, age: f32, default: [f32; N]) -> [f32; N] {
        let Some(&(first_time, first)) = self.keys.first() else {
            return default;
        };
        let age = age.clamp(0.0, 1.0);
        if age <= first_time {
            return first;
        }
        for pair in self.keys.windows(2) {
            let [(a, va), (b, vb)] = pair else {
                unreachable!()
            };
            if age <= *b {
                let t = if b > a { (age - a) / (b - a) } else { 1.0 };
                return std::array::from_fn(|i| va[i] + (vb[i] - va[i]) * t);
            }
        }
        self.keys.last().unwrap().1
    }
}

/// Decoded 0x1dc-byte v264 record, before projection into a renderer's runtime model.
#[derive(Debug, Clone)]
pub struct WrathParticleEmitter {
    pub id: u32,
    pub flags: u32,
    pub position: [f32; 3],
    pub bone: u16,
    pub texture_index: u16,
    pub geometry_model: String,
    pub recursion_model: String,
    pub blend: u8,
    pub shape: u8,
    pub color_index: u16,
    pub priority_plane: i16,
    pub rows: u16,
    pub columns: u16,
    /// Speed, speed variation, latitude, longitude, gravity, lifespan, emission rate,
    /// width, length, z source, visibility; each track keeps its own clock/interpolation.
    pub tracks: [ParticleTrack; 11],
    pub life_variation: f32,
    pub emission_variation: f32,
    pub color: ParticleCurve<3>,
    pub alpha: ParticleCurve<1>,
    pub scale: ParticleCurve<2>,
    pub scale_variation: [f32; 2],
    pub head_cells: ParticleCurve<1>,
    pub tail_cells: ParticleCurve<1>,
    pub tail_length: f32,
    pub twinkle_fps: f32,
    pub twinkle_on_off: f32,
    pub twinkle_scale: [f32; 2],
    pub inherited_velocity_scale: f32,
    pub drag: f32,
    pub initial_spin: f32,
    pub initial_spin_variation: f32,
    pub spin: f32,
    pub spin_variation: f32,
    pub tumble_min: [f32; 3],
    pub tumble_max: [f32; 3],
    pub wind: [f32; 3],
    pub wind_time: f32,
    pub follow: [f32; 4],
    pub spline: Vec<[f32; 3]>,
}

impl WrathParticleEmitter {
    pub(crate) fn sample_age(&self, age: f32) -> crate::OverLifeSample {
        // M2PartTrack colors are float byte-domain RGB; the renderer uses unit-domain gamma.
        let rgb = self
            .color
            .sample(age, [255.0; 3])
            .map(|component| component / 255.0);
        crate::OverLifeSample {
            color: [rgb[0], rgb[1], rgb[2], self.alpha.sample(age, [1.0])[0]],
            size: self.scale.sample(age, [1.0; 2])[0],
            head_cell: self.head_cells.sample(age, [0.0])[0] as u16,
            tail_cell: self.tail_cells.sample(age, [0.0])[0] as u16,
        }
    }
}

fn scalar_track(source: &ParticleTrack) -> benilla_m2::M2ScalarTrack {
    let mut result = benilla_m2::M2ScalarTrack {
        interp: source.interpolation,
        gseq: source.global_sequence,
        ranges: Vec::new(),
        keys: Vec::new(),
    };
    for sequence in &source.sequences {
        let start = result.keys.len() as u32;
        result.keys.extend_from_slice(sequence);
        result.ranges.push(if sequence.is_empty() {
            (1, 0)
        } else {
            (start, result.keys.len() as u32 - 1)
        });
        if source.global_sequence != 0xffff {
            break;
        }
    }
    result
}

/// Activate only the subset the existing simulation can represent without discarding behavior.
/// Unsupported variations remain guarded with a concrete diagnostic instead of vanilla decoding.
pub(crate) fn render_emitters(bytes: &[u8]) -> Result<Vec<crate::ParticleEmitterDef>> {
    use crate::models::SeqSlot;
    use crate::particles::{blend_of, lit_of, shape_of};
    use crate::{CellRamp, EmitParams, EmitTiming, OverLife, ParticleEmitterDef};
    let records = parse_wrath_particle_records(bytes)?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    ensure!(
        records.len() <= 256,
        "Wrath emitter count exceeds runtime budget"
    );
    let format = benilla_m2::parse_m2(&mut std::io::Cursor::new(bytes))?;
    let model = format.model();
    let sequence_data = array(bytes, 0x1c, 64)?;
    let mut slots = Vec::new();
    for (index, sequence) in sequence_data.chunks_exact(64).enumerate() {
        slots.push(SeqSlot {
            index,
            band: (0, sequence.u32_at(4).context("sequence duration")?),
            looping: sequence.u32_at(12).context("sequence flags")? & 1 == 0,
        });
    }
    if slots.is_empty() {
        slots.push(SeqSlot {
            index: 0,
            band: (0, u32::MAX),
            looping: true,
        });
    }
    let global = array(bytes, 0x14, 4)?
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect::<Vec<_>>();
    let mut output = Vec::with_capacity(records.len());
    for record in records {
        ensure!(
            record.priority_plane == 0,
            "Wrath particle priority plane requires runtime support"
        );
        ensure!(
            matches!(record.shape, 1 | 2),
            "Wrath spline particles remain unsupported"
        );
        ensure!(
            record.geometry_model.is_empty() && record.recursion_model.is_empty(),
            "Wrath geometry/recursive particles remain guarded"
        );
        ensure!(
            record.life_variation == 0.0 && record.emission_variation == 0.0,
            "Wrath lifespan/emission variation requires runtime support"
        );
        ensure!(
            record.initial_spin == 0.0
                && record.initial_spin_variation == 0.0
                && record.spin == 0.0
                && record.spin_variation == 0.0,
            "Wrath particle spin variation requires runtime support"
        );
        ensure!(
            record.wind == [0.0; 3]
                && record.wind_time == 0.0
                && record.tumble_min == [0.0; 3]
                && record.tumble_max == [0.0; 3],
            "Wrath wind/tumble particles remain guarded"
        );
        ensure!(
            record.scale_variation == [0.0; 2]
                && record.scale.keys.iter().all(|(_, xy)| xy[0] == xy[1]),
            "Wrath anisotropic/variable scale requires runtime support"
        );
        ensure!(
            record.tracks.iter().all(|t| t.interpolation <= 1),
            "unsupported Wrath particle interpolation"
        );
        ensure!(
            record.rows.is_power_of_two() && record.columns.is_power_of_two(),
            "invalid particle atlas"
        );
        let texture = model
            .textures
            .get(usize::from(record.texture_index))
            .context("particle texture index outside model")?
            .filename
            .string
            .to_str()?
            .to_owned();
        ensure!(
            !texture.is_empty(),
            "replaceable particle texture requires owner support"
        );
        let tracks = record.tracks.each_ref().map(scalar_track);
        let head = record.flags & 0x20000 != 0;
        let tail = record.flags & 0x40000 != 0;
        ensure!(head || tail, "particle has neither head nor tail");
        let sampled = record.sample_age(0.0);
        output.push(ParticleEmitterDef {
            flags: record.flags,
            position: record.position,
            bone: record.bone,
            shape: shape_of(u16::from(record.shape)),
            blend: blend_of(record.blend),
            lit: lit_of(record.flags, record.blend),
            geometry_model: None,
            recursion_model: None,
            texture: Some(texture),
            tile_rows: record.rows,
            tile_cols: record.columns,
            head_tail: match (head, tail) {
                (true, false) => 0,
                (false, true) => 1,
                _ => 2,
            },
            timing: EmitTiming::bake(&tracks[6], &tracks[10], &slots, &global),
            params: EmitParams::bake(
                [
                    &tracks[0], &tracks[1], &tracks[2], &tracks[3], &tracks[4], &tracks[5],
                    &tracks[8], &tracks[7], &tracks[9],
                ],
                &slots,
                &global,
            ),
            drag: record.drag,
            tail_time: record.tail_length,
            angular_velocity_min: [0.0; 3],
            angular_velocity_max: [0.0; 3],
            inherit_scale: record.inherited_velocity_scale,
            follow_speed1: record.follow[0],
            follow_scale1: record.follow[1],
            follow_speed2: record.follow[2],
            follow_scale2: record.follow[3],
            twinkle_speed: record.twinkle_fps,
            twinkle_percent: record.twinkle_on_off,
            twinkle_min: record.twinkle_scale[0],
            twinkle_max: record.twinkle_scale[1],
            spline: None,
            spin: 0.0,
            over_life: OverLife {
                mid: 0.5,
                color: [sampled.color; 3],
                scale: [sampled.size; 3],
                head_cells: [CellRamp::new(sampled.head_cell, sampled.head_cell); 2],
                tail_cells: [CellRamp::new(sampled.tail_cell, sampled.tail_cell); 2],
                repeat: [1.0; 2],
            },
            wrath_curves: Some(std::sync::Arc::new(record)),
        });
    }
    Ok(output)
}

fn array(bytes: &[u8], at: usize, stride: usize) -> Result<&[u8]> {
    let count = bytes.u32_at(at).context("array count")? as usize;
    let offset = bytes.u32_at(at + 4).context("array offset")? as usize;
    if count == 0 {
        return Ok(&[]);
    }
    let size = count.checked_mul(stride).context("array size overflow")?;
    bytes
        .bytes_at(offset, size)
        .context("particle array outside file")
}

fn vector<const N: usize>(bytes: &[u8], at: usize) -> Result<[f32; N]> {
    let mut values = [0.0; N];
    for (i, value) in values.iter_mut().enumerate() {
        *value = bytes.f32_at(at + i * 4).context("particle vector")?;
        ensure!(value.is_finite(), "non-finite particle value");
    }
    Ok(values)
}

fn track(bytes: &[u8], at: usize, boolean: bool, remaining: &mut usize) -> Result<ParticleTrack> {
    let times = array(bytes, at + 4, 8)?;
    let values = array(bytes, at + 12, 8)?;
    ensure!(
        times.len() == values.len(),
        "particle track sequence count mismatch"
    );
    *remaining = remaining
        .checked_sub(times.len() / 8)
        .context("particle sequence expansion budget")?;
    let mut sequences = Vec::with_capacity(times.len() / 8);
    // Bound aggregate expansion as well as individual arrays: aliased forged descriptors must
    // not expand the same file bytes quadratically.
    for (ts, vs) in times.chunks_exact(8).zip(values.chunks_exact(8)) {
        let read_inner = |descriptor: &[u8], stride: usize| -> Result<&[u8]> {
            let count = descriptor.u32_at(0).context("inner count")? as usize;
            let offset = descriptor.u32_at(4).context("inner offset")? as usize;
            if count == 0 {
                return Ok(&[]);
            }
            bytes
                .bytes_at(offset, count.checked_mul(stride).context("inner size")?)
                .context("particle key array outside file")
        };
        let ts = read_inner(ts, 4)?;
        let stride = if boolean { 1 } else { 4 };
        let vs = read_inner(vs, stride)?;
        ensure!(
            ts.len() / 4 == vs.len() / stride,
            "particle track key count mismatch"
        );
        let count = ts.len() / 4;
        *remaining = remaining
            .checked_sub(count)
            .context("particle key expansion budget")?;
        let mut keys = Vec::with_capacity(count);
        for i in 0..count {
            let time = ts.u32_at(i * 4).context("particle key time")?;
            let value = if boolean {
                f32::from(vs[i] != 0)
            } else {
                vs.f32_at(i * 4).context("particle key")?
            };
            ensure!(value.is_finite(), "non-finite particle track key");
            ensure!(
                keys.last().is_none_or(|&(prev, _)| prev <= time),
                "unordered particle track"
            );
            keys.push((time, value));
        }
        sequences.push(keys);
    }
    Ok(ParticleTrack {
        interpolation: bytes.u16_at(at).context("particle interpolation")?,
        global_sequence: bytes.u16_at(at + 2).context("particle global sequence")?,
        sequences,
    })
}

fn curve<const N: usize>(
    bytes: &[u8],
    at: usize,
    encoding: u8,
    remaining: &mut usize,
) -> Result<ParticleCurve<N>> {
    let times = array(bytes, at, 2)?;
    let stride = if encoding == 0 { N * 4 } else { N * 2 };
    let values = array(bytes, at + 8, stride)?;
    ensure!(
        times.len() / 2 == values.len() / stride,
        "particle age curve count mismatch"
    );
    *remaining = remaining
        .checked_sub(times.len() / 2)
        .context("particle curve expansion budget")?;
    let mut keys = Vec::with_capacity(times.len() / 2);
    for i in 0..times.len() / 2 {
        let time = times.u16_at(i * 2).context("age time")? as i16 as f32 / 32767.0;
        let mut value = [0.0; N];
        for (component, slot) in value.iter_mut().enumerate() {
            let offset = i * stride + component * if encoding == 0 { 4 } else { 2 };
            *slot = match encoding {
                0 => values.f32_at(offset).context("age float")?,
                1 => values.u16_at(offset).context("age alpha")? as i16 as f32 / 32767.0,
                _ => f32::from(values.u16_at(offset).context("age cell")?),
            };
            ensure!(slot.is_finite(), "non-finite particle curve");
        }
        ensure!(
            keys.last().is_none_or(|&(previous, _)| previous <= time),
            "unordered particle age curve"
        );
        keys.push((time, value));
    }
    Ok(ParticleCurve { keys })
}

/// Validate every consumed array before allocating from its count. Unknown/malformed formats
/// return an error; this decoder does not relax the existing production-rendering guard.
pub fn parse_wrath_particle_records(bytes: &[u8]) -> Result<Vec<WrathParticleEmitter>> {
    ensure!(
        bytes.get(..4) == Some(b"MD20") && bytes.u32_at(4) == Some(264),
        "expected MD20 v264"
    );
    let records = array(bytes, 0x128, RECORD_SIZE)?;
    let mut remaining = bytes.len();
    let mut output = Vec::with_capacity(records.len() / RECORD_SIZE);
    for (index, rec) in records.chunks_exact(RECORD_SIZE).enumerate() {
        let base = bytes.u32_at(0x12c).context("particle base")? as usize + index * RECORD_SIZE;
        let mut text = |at| -> Result<String> {
            let raw = array(bytes, base + at, 1)?;
            remaining = remaining
                .checked_sub(raw.len())
                .context("particle string expansion budget")?;
            Ok(std::str::from_utf8(raw.strip_suffix(&[0]).unwrap_or(raw))?.to_owned())
        };
        let geometry_model = text(0x18)?;
        let recursion_model = text(0x20)?;
        let offsets = [
            0x34, 0x48, 0x5c, 0x70, 0x84, 0x98, 0xb0, 0xc8, 0xdc, 0xf0, 0x1c8,
        ];
        let mut tracks = Vec::with_capacity(11);
        for (i, offset) in offsets.iter().enumerate() {
            tracks.push(track(bytes, base + offset, i == 10, &mut remaining)?);
        }
        let scalar = |at| -> Result<f32> { Ok(vector::<1>(rec, at)?[0]) };
        let raw_spline = array(bytes, base + 0x1c0, 12)?;
        remaining = remaining
            .checked_sub(raw_spline.len() / 12)
            .context("particle spline expansion budget")?;
        let spline = (0..raw_spline.len() / 12)
            .map(|i| vector(raw_spline, i * 12))
            .collect::<Result<_>>()?;
        output.push(WrathParticleEmitter {
            id: rec.u32_at(0).context("particle id")?,
            flags: rec.u32_at(4).context("particle flags")?,
            position: vector(rec, 8)?,
            bone: rec.u16_at(0x14).context("particle bone")?,
            texture_index: rec.u16_at(0x16).context("particle texture")?,
            geometry_model,
            recursion_model,
            blend: rec[0x28],
            shape: rec[0x29],
            color_index: rec.u16_at(0x2a).context("particle color")?,
            priority_plane: rec.u16_at(0x2e).context("particle priority")? as i16,
            rows: rec.u16_at(0x30).context("particle rows")?,
            columns: rec.u16_at(0x32).context("particle columns")?,
            tracks: tracks
                .try_into()
                .map_err(|_| anyhow::anyhow!("particle track count"))?,
            life_variation: scalar(0xac)?,
            emission_variation: scalar(0xc4)?,
            color: curve(bytes, base + 0x104, 0, &mut remaining)?,
            alpha: curve(bytes, base + 0x114, 1, &mut remaining)?,
            scale: curve(bytes, base + 0x124, 0, &mut remaining)?,
            scale_variation: vector(rec, 0x134)?,
            head_cells: curve(bytes, base + 0x13c, 2, &mut remaining)?,
            tail_cells: curve(bytes, base + 0x14c, 2, &mut remaining)?,
            tail_length: scalar(0x15c)?,
            twinkle_fps: scalar(0x160)?,
            twinkle_on_off: scalar(0x164)?,
            twinkle_scale: vector(rec, 0x168)?,
            inherited_velocity_scale: scalar(0x170)?,
            drag: scalar(0x174)?,
            initial_spin: scalar(0x178)?,
            initial_spin_variation: scalar(0x17c)?,
            spin: scalar(0x180)?,
            spin_variation: scalar(0x184)?,
            tumble_min: vector(rec, 0x188)?,
            tumble_max: vector(rec, 0x194)?,
            wind: vector(rec, 0x1a0)?,
            wind_time: scalar(0x1ac)?,
            follow: vector(rec, 0x1b0)?,
            spline,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 0x140 + RECORD_SIZE];
        b[..4].copy_from_slice(b"MD20");
        b[4..8].copy_from_slice(&264u32.to_le_bytes());
        b[0x128..0x12c].copy_from_slice(&1u32.to_le_bytes());
        b[0x12c..0x130].copy_from_slice(&0x140u32.to_le_bytes());
        b
    }

    #[test]
    fn uses_wrath_record_not_legacy_offsets_and_rejects_forged_counts() {
        let mut b = fixture();
        b[0x140 + 0x29] = 2;
        b[0x140 + 0x174..0x140 + 0x178].copy_from_slice(&0.75f32.to_le_bytes());
        let emitters = parse_wrath_particle_records(&b).unwrap();
        assert_eq!(emitters[0].shape, 2);
        assert_eq!(emitters[0].drag, 0.75);
        b[0x128..0x12c].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_wrath_particle_records(&b).is_err());
        b[0x128..0x12c].copy_from_slice(&1u32.to_le_bytes());
        b.truncate(b.len() - 1);
        assert!(parse_wrath_particle_records(&b).is_err());
    }

    #[test]
    fn normalized_curves_keep_independent_keys_and_two_dimensional_scale() {
        let mut b = fixture();
        let start = b.len() as u32;
        b.extend_from_slice(&[0, 0, 255, 127]);
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            b.extend_from_slice(&value.to_le_bytes());
        }
        let at = 0x140 + 0x124;
        for (offset, value) in [(at, 2), (at + 4, start), (at + 8, 2), (at + 12, start + 4)] {
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let e = parse_wrath_particle_records(&b).unwrap();
        assert_eq!(e[0].scale.keys, [(0.0, [1.0, 2.0]), (1.0, [3.0, 4.0])]);
        assert!(e[0].alpha.keys.is_empty());
    }

    #[test]
    fn age_sampler_uses_independent_keys_and_byte_domain_color() {
        let mut e = parse_wrath_particle_records(&fixture()).unwrap().remove(0);
        e.color.keys = vec![
            (0.0, [255.0, 0.0, 0.0]),
            (0.2, [0.0, 255.0, 0.0]),
            (0.5, [0.0, 0.0, 255.0]),
            (1.0, [255.0; 3]),
        ];
        e.alpha.keys = vec![(0.0, [0.0]), (1.0, [1.0])];
        e.scale.keys = vec![(0.0, [2.0; 2]), (1.0, [4.0; 2])];
        let sample = e.sample_age(0.5);
        assert_eq!(sample.color, [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(sample.size, 3.0);
    }

    #[test]
    fn sequence_slots_and_global_tracks_keep_nested_arrays() {
        for global in [0xffffu16, 0] {
            let mut b = fixture();
            let at = 0x140 + 0xb0;
            let descriptors = b.len();
            b.resize(descriptors + 32, 0);
            let times = b.len();
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&500u32.to_le_bytes());
            let values = b.len();
            b.extend_from_slice(&12.0f32.to_le_bytes());
            b.extend_from_slice(&24.0f32.to_le_bytes());
            b[at + 2..at + 4].copy_from_slice(&global.to_le_bytes());
            // Empty first sequence, then two keys in the second. Global uses the same layout.
            for (offset, value) in [
                (at + 4, 2),
                (at + 8, descriptors as u32),
                (at + 12, 2),
                (at + 16, (descriptors + 16) as u32),
                (descriptors + 8, 2),
                (descriptors + 12, times as u32),
                (descriptors + 24, 2),
                (descriptors + 28, values as u32),
            ] {
                b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            let e = parse_wrath_particle_records(&b).unwrap();
            assert_eq!(e[0].tracks[6].global_sequence, global);
            assert!(e[0].tracks[6].sequences[0].is_empty());
            assert_eq!(e[0].tracks[6].sequences[1], [(0, 12.0), (500, 24.0)]);
            b[descriptors + 28..descriptors + 32].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(parse_wrath_particle_records(&b).is_err());
        }
    }

    #[test]
    fn aliased_descriptors_cannot_expand_quadratically() {
        let mut b = fixture();
        let at = 0x140 + 0x34;
        let start = b.len();
        let count = 100usize;
        b.resize(start + count * 16 + 128 * 8, 0);
        let times = start + count * 16;
        let values = times + 128 * 4;
        for (offset, value) in [
            (at + 4, count as u32),
            (at + 8, start as u32),
            (at + 12, count as u32),
            (at + 16, (start + count * 8) as u32),
        ] {
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for i in 0..count {
            for (offset, value) in [
                (start + i * 8, 128),
                (start + i * 8 + 4, times as u32),
                (start + count * 8 + i * 8, 128),
                (start + count * 8 + i * 8 + 4, values as u32),
            ] {
                b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        let error = parse_wrath_particle_records(&b).unwrap_err();
        assert!(error.to_string().contains("budget"), "{error:#}");
    }
}
