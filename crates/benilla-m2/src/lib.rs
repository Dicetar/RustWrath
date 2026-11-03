//! An M2 (MD20) model reader for WoW 1.12.1. Vanilla models are MD20 version 256 or 257: a header
//! of `M2Array`s (`count`, file `offset`), the arrays they name, and the embedded skin profiles.
//! Particle emitters and lights are not read here; benilla-formats reads them from the raw bytes.

mod error;
mod model;
mod skin;
mod track;

pub use error::Error;
pub use model::{
    M2ArrayString, M2Attachment, M2BlendMode, M2Bone, M2BoneFlags, M2Camera, M2EventMarker,
    M2Format, M2Header, M2Material, M2Model, M2PlayableAnim, M2RawData, M2RenderFlags, M2Texture,
    M2TextureTransform, M2TextureType, M2Vertex, C2, C3,
};
pub use skin::{parse_external_skin, Skin, SkinBatch, SkinSection};
pub use track::{
    CubicValue, M2QuatTrack, M2ScalarSplineTrack, M2ScalarTrack, M2SplineKey, M2Track,
    M2Vec3SplineTrack, M2Vec3Track,
};

use std::ffi::CString;
use std::io::Cursor;

use benilla_bytes::{capped, ByteExt};

use error::Result;
use track::{track_fix16, track_quat, track_spline_f32, track_spline_vec3, track_vec3_timed};

/// A `C3Vector` (3×f32) at `o`.
fn rd_c3(b: &[u8], o: usize) -> Option<C3> {
    Some(C3 {
        x: b.f32_at(o)?,
        y: b.f32_at(o + 4)?,
        z: b.f32_at(o + 8)?,
    })
}

/// Parse an MD20 model; the cursor's position is ignored.
pub fn parse_m2(cursor: &mut Cursor<&[u8]>) -> Result<M2Format> {
    let b: &[u8] = cursor.get_ref();
    if b.len() < 8 || &b[0..4] != b"MD20" {
        return Err(Error::NotMd20);
    }
    let version = b.u32_at(4).ok_or(Error::Truncated)?;
    if !(256..=264).contains(&version) {
        return Err(Error::UnsupportedVersion(version));
    }
    let wrath = version >= 264;

    // Walk the header's `(count, offset)` M2Arrays, 8 bytes each, pre-Wrath layout.
    let arr = |p: usize| -> Result<(u32, u32)> {
        Ok((
            b.u32_at(p).ok_or(Error::Truncated)?,
            b.u32_at(p + 4).ok_or(Error::Truncated)?,
        ))
    };
    let mut p = 8;
    let _name = arr(p)?;
    p += 8;
    p += 4; // flags u32
    let _global = arr(p)?;
    p += 8;
    let _anim = arr(p)?;
    p += 8;
    // AnimationLookup (`+0x24`), which the reference's ownership test `0x711960` reads.
    let animation_lookup_arr = arr(p)?;
    p += 8;
    // PlayableAnimationLookup (`+0x2c`): pre-Wrath only.
    let playable_animation_lookup_arr = if !wrath {
        let v = arr(p)?;
        p += 8;
        v
    } else {
        (0, 0)
    };
    let bones = arr(p)?;
    p += 8;
    let _key_bone = arr(p)?;
    p += 8;
    let vertices = arr(p)?;
    p += 8;
    let views = if !wrath {
        let v = arr(p)?;
        p += 8;
        v
    } else {
        p += 4; // num_skin_profiles, companion `.skin` files are not embedded
        (0, 0)
    };
    let colors_arr = arr(p)?; // header 0x54: M2Color[] (colour + alpha tracks)
    p += 8;
    let textures = arr(p)?;
    p += 8;
    let transparency_arr = arr(p)?; // header 0x64: M2TextureWeight[] (weight tracks)
    p += 8;
    if version <= 263 {
        p += 8; // texture_flipbooks (pre-Wrath)
    }
    let tex_anim_arr = arr(p)?; // header 0x74: M2TextureTransform[] (3 UV-animation tracks each)
    p += 8;
    let _color_repl = arr(p)?;
    p += 8;
    let render_flags = arr(p)?;
    p += 8;
    let _bone_lookup = arr(p)?;
    p += 8;
    let texture_lookup_table = arr(p)?;
    p += 8;
    // header 0x9c: texture_unit_lookup, a stage's UV channel or environment coordinate. These
    // three lookups sit where the reference's header walk `0x71cdf0` puts them, not a slot
    // earlier: StormwindMagePortal01 has [0] at 0x9c and its weight lookup [0,1,2,3] at 0xa4.
    let texture_unit_lookup_arr = arr(p)?;
    p += 8;
    // header 0xa4: transparency_lookup (u16), `weight_combo_index` to a weight track.
    let transparency_lookup_arr = arr(p)?;
    p += 8;
    // header 0xac: texture_animation_lookup (u16), `texture_transform_combo_index` to a texture
    // transform (`0x70b897`); 0xffff is none.
    let tex_anim_lookup_arr = arr(p)?;
    p += 8;
    // Authored bounds: AABB (min C3, max C3) then sphere radius.
    let read_box = |o: usize| -> Result<[f32; 3]> {
        Ok([
            b.f32_at(o).ok_or(Error::Truncated)?,
            b.f32_at(o + 4).ok_or(Error::Truncated)?,
            b.f32_at(o + 8).ok_or(Error::Truncated)?,
        ])
    };
    let bounding_box_min = read_box(p)?;
    p += 12;
    let bounding_box_max = read_box(p)?;
    p += 12;
    let bounding_sphere_radius = b.f32_at(p).ok_or(Error::Truncated)?;
    p += 4;
    // The collision box and sphere radius: the tight hull.
    let collision_box_min = read_box(p)?;
    p += 12;
    let collision_box_max = read_box(p)?;
    p += 12;
    let collision_sphere_radius = b.f32_at(p).ok_or(Error::Truncated)?;
    p += 4;
    let bounding_triangles = arr(p)?;
    p += 8;
    let bounding_vertices = arr(p)?;
    p += 8;
    let _bounding_normals = arr(p)?;
    p += 8;
    let attachments = arr(p)?;
    p += 8;
    let attach_lookup = arr(p)?;
    p += 8;
    // header 0x114: the animation events (`$CSL`, `$BWR`, `$SND`…); only their positions are read.
    let events = arr(p)?;
    // The follow camera's pivot height (`0x50cbc0`): attachment id 17's model-space Z.
    let pivot_attach_z = attachment_z(b, 17, attachments, attach_lookup);

    // --- read the arrays we keep ---
    let get = |start: usize, len: usize| -> Result<&[u8]> {
        b.bytes_at(start, len).ok_or(Error::Truncated)
    };

    // Vertices: 48 bytes each. The reservation is capped by the file; a short file fails at `get`.
    let vert_avail = b.len().saturating_sub(vertices.1 as usize);
    let mut verts = Vec::with_capacity(capped(vertices.0 as usize, 48, vert_avail));
    for i in 0..vertices.0 as usize {
        let v = get(vertices.1 as usize + i * 48, 48)?;
        verts.push(M2Vertex {
            position: rd_c3(v, 0).ok_or(Error::Truncated)?,
            bone_weights: [v[12], v[13], v[14], v[15]],
            bone_indices: [v[16], v[17], v[18], v[19]],
            normal: rd_c3(v, 20).ok_or(Error::Truncated)?,
            tex_coords: C2 {
                x: v.f32_at(32).ok_or(Error::Truncated)?,
                y: v.f32_at(36).ok_or(Error::Truncated)?,
            },
        });
    }

    // Textures: 16 bytes each (type u32, flags u32, filename M2Array).
    let tex_avail = b.len().saturating_sub(textures.1 as usize);
    let mut texs = Vec::with_capacity(capped(textures.0 as usize, 16, tex_avail));
    for i in 0..textures.0 as usize {
        let t = get(textures.1 as usize + i * 16, 16)?;
        let ttype = M2TextureType::from_u32(t.u32_at(0).ok_or(Error::Truncated)?);
        // `+0x04`, the address mode: bit 0 repeats U, bit 1 V, clear clamps to edge. Cutout cards
        // put UVs outside `0..1` to clamp to their transparent edge; repeated, they draw solid.
        let tflags = t.u32_at(4).ok_or(Error::Truncated)?;
        let (fcount, fofs) = (
            t.u32_at(8).ok_or(Error::Truncated)? as usize,
            t.u32_at(12).ok_or(Error::Truncated)? as usize,
        );
        let raw = b.bytes_at(fofs, fcount).unwrap_or(&[]);
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        let string = CString::new(&raw[..end]).unwrap_or_default();
        texs.push(M2Texture {
            texture_type: ttype,
            wrap_x: tflags & 0x1 != 0,
            wrap_y: tflags & 0x2 != 0,
            filename: M2ArrayString { string },
        });
    }

    // Materials (render flags): 4 bytes each (flags u16, blend_mode u16).
    let flags_avail = b.len().saturating_sub(render_flags.1 as usize);
    let mut mats = Vec::with_capacity(capped(render_flags.0 as usize, 4, flags_avail));
    for i in 0..render_flags.0 as usize {
        let m = get(render_flags.1 as usize + i * 4, 4)?;
        mats.push(M2Material {
            flags: M2RenderFlags(m.u16_at(0).ok_or(Error::Truncated)?),
            blend_mode: M2BlendMode(m.u16_at(2).ok_or(Error::Truncated)?),
        });
    }

    // Bones: 108 bytes in v256, three 28-byte tracks, pivot C3 @96. Wrath's v264 record is 88
    // bytes with 20-byte tracks and its pivot @76. The reference zeroes a NaN pivot component.
    let bone_size = if wrath { 88 } else { 108 };
    let bone_pivot = if wrath { 76 } else { 96 };
    let bones_avail = b.len().saturating_sub(bones.1 as usize);
    let mut bone_list = Vec::with_capacity(capped(bones.0 as usize, bone_size, bones_avail));
    for i in 0..bones.0 as usize {
        let bn = get(bones.1 as usize + i * bone_size, bone_size)?;
        let mut pivot = rd_c3(bn, bone_pivot).ok_or(Error::Truncated)?;
        if pivot.x.is_nan() {
            pivot.x = 0.0;
        }
        if pivot.y.is_nan() {
            pivot.y = 0.0;
        }
        if pivot.z.is_nan() {
            pivot.z = 0.0;
        }
        bone_list.push(M2Bone {
            key_bone: bn.u32_at(0).ok_or(Error::Truncated)? as i32 as i16,
            flags: M2BoneFlags(bn.u32_at(4).ok_or(Error::Truncated)?),
            parent: bn.u16_at(8).ok_or(Error::Truncated)? as i16,
            pivot,
        });
    }

    // Attachments: 48 bytes in vanilla, 40 in Wrath (id, bone, position C3 @8, a skipped track).
    // A record whose id or bone overflows `u16`, or whose bone is out of range, is dropped; real
    // ids stop at 36.
    let att_size = if wrath { 40 } else { 48 };
    let att_count = attachments.0 as usize;
    let att_avail = b.len().saturating_sub(attachments.1 as usize);
    // The table must fit the file first: `emitted_at` below is sized from the raw count.
    if capped(att_count, att_size, att_avail) < att_count {
        return Err(Error::Truncated);
    }
    let mut attachment_list = Vec::with_capacity(att_count);
    // AttachLookup indexes the file's records; a dropped one shifts ours, so map file to emitted.
    let mut emitted_at = vec![0xffffu16; att_count]; // proven to fit above
    for (i, emitted) in emitted_at.iter_mut().enumerate() {
        let a = get(attachments.1 as usize + i * att_size, att_size)?;
        let id = a.u32_at(0).ok_or(Error::Truncated)?;
        let bone = a.u32_at(4).ok_or(Error::Truncated)?;
        let position = rd_c3(a, 8).ok_or(Error::Truncated)?;
        let (Ok(id), Ok(bone)) = (u16::try_from(id), u16::try_from(bone)) else {
            continue;
        };
        if bone as usize >= bone_list.len() {
            continue;
        }
        *emitted = u16::try_from(attachment_list.len()).unwrap_or(0xffff);
        attachment_list.push(M2Attachment {
            id,
            bone,
            position: [position.x, position.y, position.z],
        });
    }

    // AttachLookup (`+0x10c`): attachment id to its one record, the reference's only id map
    // (`0x710310`). Shipped weapons repeat ids (`Stave_2H_Long_D_05`: 0-3 twice) and the lookup
    // names one. Stored as emitted indices, `0xffff` for none.
    let lk_avail = b.len().saturating_sub(attach_lookup.1 as usize);
    let mut attach_lookup_list = Vec::with_capacity(capped(attach_lookup.0 as usize, 2, lk_avail));
    for i in 0..attach_lookup.0 as usize {
        let raw = get(attach_lookup.1 as usize + i * 2, 2)?
            .u16_at(0)
            .ok_or(Error::Truncated)?;
        attach_lookup_list.push(emitted_at.get(raw as usize).copied().unwrap_or(0xffff));
    }

    // Event markers: 44 bytes in vanilla; Wrath's record is 36 (the timestamp track has no values
    // array). Both keep the 4CC, data, bone @8 and position C3 @12.
    let ev_size = if wrath { 36 } else { 44 };
    let ev_avail = b.len().saturating_sub(events.1 as usize);
    let mut event_markers = Vec::with_capacity(capped(events.0 as usize, ev_size, ev_avail));
    for i in 0..events.0 as usize {
        let e = get(events.1 as usize + i * ev_size, ev_size)?;
        let ident = [e[0], e[1], e[2], e[3]];
        let bone = e.u32_at(8).ok_or(Error::Truncated)?;
        let position = rd_c3(e, 12).ok_or(Error::Truncated)?;
        let Ok(bone) = u16::try_from(bone) else {
            continue;
        };
        if bone as usize >= bone_list.len() {
            continue;
        }
        event_markers.push(M2EventMarker {
            ident,
            bone,
            position: [position.x, position.y, position.z],
        });
    }

    // AnimationLookup: one u16 each.
    let al_avail = b.len().saturating_sub(animation_lookup_arr.1 as usize);
    let mut animation_lookup =
        Vec::with_capacity(capped(animation_lookup_arr.0 as usize, 2, al_avail));
    for i in 0..animation_lookup_arr.0 as usize {
        animation_lookup.push(
            get(animation_lookup_arr.1 as usize + i * 2, 2)?
                .u16_at(0)
                .ok_or(Error::Truncated)?,
        );
    }

    // PlayableAnimationLookup: one dword each, low16 resolved id / high16 dir flags.
    let pal_avail = b
        .len()
        .saturating_sub(playable_animation_lookup_arr.1 as usize);
    let mut playable_animation_lookup = Vec::with_capacity(capped(
        playable_animation_lookup_arr.0 as usize,
        4,
        pal_avail,
    ));
    for i in 0..playable_animation_lookup_arr.0 as usize {
        let dword = get(playable_animation_lookup_arr.1 as usize + i * 4, 4)?
            .u32_at(0)
            .ok_or(Error::Truncated)?;
        playable_animation_lookup.push(M2PlayableAnim {
            resolved_id: (dword & 0xffff) as u16,
            dir_flags: (dword >> 16) as u16,
        });
    }

    // Texture lookup table: u16 each.
    let tlt_avail = b.len().saturating_sub(texture_lookup_table.1 as usize);
    let mut tlt = Vec::with_capacity(capped(texture_lookup_table.0 as usize, 2, tlt_avail));
    for i in 0..texture_lookup_table.0 as usize {
        tlt.push(
            get(texture_lookup_table.1 as usize + i * 2, 2)?
                .u16_at(0)
                .ok_or(Error::Truncated)?,
        );
    }

    // M2Color (stride 2 tracks): an RGB track @ +0x00 and a fix16 alpha track after it.
    // M2TextureWeight (stride 1 track): one weight track. The track itself is `0x1c` bytes before
    // Wrath and `0x14` from v264 (per-sequence arrays, see `track::M2Track`). An out-of-range
    // track reads as no keys, so these loops run over the records the file holds, never the raw
    // count.
    let track_size = if wrath { 0x14 } else { 0x1c };
    let color_size = 2 * track_size;
    let colors_avail = b.len().saturating_sub(colors_arr.1 as usize);
    let colors_cap = capped(colors_arr.0 as usize, color_size, colors_avail);
    let mut color_alpha_tracks = Vec::with_capacity(colors_cap);
    let mut color_rgb_tracks = Vec::with_capacity(colors_cap);
    for i in 0..colors_cap {
        let base = colors_arr.1 as usize + i * color_size;
        color_alpha_tracks.push(track_fix16(b, base + track_size));
        color_rgb_tracks.push(track_vec3_timed(b, base));
    }
    let transparency_avail = b.len().saturating_sub(transparency_arr.1 as usize);
    let transparency_cap = capped(transparency_arr.0 as usize, track_size, transparency_avail);
    let mut transparency_tracks = Vec::with_capacity(transparency_cap);
    for i in 0..transparency_cap {
        transparency_tracks.push(track_fix16(b, transparency_arr.1 as usize + i * track_size));
    }
    let tulookup_avail = b.len().saturating_sub(texture_unit_lookup_arr.1 as usize);
    let mut texture_unit_lookup = Vec::with_capacity(capped(
        texture_unit_lookup_arr.0 as usize,
        2,
        tulookup_avail,
    ));
    for i in 0..texture_unit_lookup_arr.0 as usize {
        texture_unit_lookup.push(
            get(texture_unit_lookup_arr.1 as usize + i * 2, 2)?
                .u16_at(0)
                .ok_or(Error::Truncated)?,
        );
    }
    let tlookup_avail = b.len().saturating_sub(transparency_lookup_arr.1 as usize);
    let mut transparency_lookup =
        Vec::with_capacity(capped(transparency_lookup_arr.0 as usize, 2, tlookup_avail));
    for i in 0..transparency_lookup_arr.0 as usize {
        transparency_lookup.push(
            get(transparency_lookup_arr.1 as usize + i * 2, 2)?
                .u16_at(0)
                .ok_or(Error::Truncated)?,
        );
    }

    // M2TextureTransform (stride 3 tracks): translation @+0x00, rotation after it, scaling after
    // that (`0x70ebd0`), bounded by the file like the colour tracks.
    let transform_size = 3 * track_size;
    let ttf_avail = b.len().saturating_sub(tex_anim_arr.1 as usize);
    let ttf_cap = capped(tex_anim_arr.0 as usize, transform_size, ttf_avail);
    let mut texture_transforms = Vec::with_capacity(ttf_cap);
    for i in 0..ttf_cap {
        let base = tex_anim_arr.1 as usize + i * transform_size;
        texture_transforms.push(M2TextureTransform {
            translation: track_vec3_timed(b, base),
            rotation: track_quat(b, base + track_size),
            scaling: track_vec3_timed(b, base + 2 * track_size),
        });
    }
    let talookup_avail = b.len().saturating_sub(tex_anim_lookup_arr.1 as usize);
    let mut texture_transform_lookup =
        Vec::with_capacity(capped(tex_anim_lookup_arr.0 as usize, 2, talookup_avail));
    for i in 0..tex_anim_lookup_arr.0 as usize {
        texture_transform_lookup.push(
            get(tex_anim_lookup_arr.1 as usize + i * 2, 2)?
                .u16_at(0)
                .ok_or(Error::Truncated)?,
        );
    }

    // Global-sequence durations (`0x14`, u32 ms): the clocks `gseq` tracks wrap on.
    let gseq_arr = (
        b.u32_at(0x14).ok_or(Error::Truncated)?,
        b.u32_at(0x18).ok_or(Error::Truncated)?,
    );
    let gseq_avail = b.len().saturating_sub(gseq_arr.1 as usize);
    let mut global_sequences = Vec::with_capacity(capped(gseq_arr.0 as usize, 4, gseq_avail));
    for i in 0..gseq_arr.0 as usize {
        let Some(ms) = b.u32_at(gseq_arr.1 as usize + i * 4) else {
            break;
        };
        global_sequences.push(ms);
    }

    // The collision hull, raw: triangle indices (u16) and vertices (C3Vector).
    let bt_bytes = get(
        bounding_triangles.1 as usize,
        bounding_triangles.0 as usize * 2,
    )?
    .to_vec();
    let bv_bytes = get(
        bounding_vertices.1 as usize,
        bounding_vertices.0 as usize * 12,
    )?
    .to_vec();

    Ok(M2Format {
        model: M2Model {
            vertices: verts,
            textures: texs,
            materials: mats,
            color_alpha_tracks,
            color_rgb_tracks,
            transparency_tracks,
            transparency_lookup,
            texture_unit_lookup,
            texture_combiners: if version == 0x101 {
                parse_texture_combiners(b)
            } else {
                Vec::new()
            },
            texture_transforms,
            texture_transform_lookup,
            global_sequences,
            bones: bone_list,
            // Wrath moved the camera arrays and changed their stride; actors do not read them.
            cameras: if wrath { Vec::new() } else { parse_cameras(b) },
            camera_lookup: if wrath {
                Vec::new()
            } else {
                parse_camera_lookup(b)
            },
            raw_data: M2RawData {
                texture_lookup_table: tlt,
                bounding_triangles: bt_bytes,
                bounding_vertices: bv_bytes,
            },
            header: M2Header {
                bounding_box_min,
                bounding_box_max,
                bounding_sphere_radius,
                collision_box_min,
                collision_box_max,
                collision_sphere_radius,
            },
            pivot_attach_z,
            attachments: attachment_list,
            attach_lookup: attach_lookup_list,
            event_markers,
            animation_lookup,
            playable_animation_lookup,
            views,
            version,
        },
    })
}

/// Parse the MD20 camera array. 12340 M2Data.h uses header 0x110, stride 0x64;
/// versions 256–263 use header 0x124, stride 0x7c. A truncated table yields only whole records.
pub fn parse_cameras(b: &[u8]) -> Vec<M2Camera> {
    let wrath = b.u32_at(4).is_some_and(|v| v >= 264);
    let (header, stride, position, target, target_base, roll) = if wrath {
        (0x110, 0x64, 0x24, 0x30, 0x44, 0x50)
    } else {
        (0x124, 0x7c, 0x2c, 0x38, 0x54, 0x60)
    };
    let (Some(count), Some(ofs)) = (b.u32_at(header), b.u32_at(header + 4)) else {
        return Vec::new();
    };
    let avail = b.len().saturating_sub(ofs as usize);
    let count = capped(count as usize, stride, avail);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let rec = ofs as usize + i * stride;
        // A record is read whole or not at all.
        if rec.checked_add(stride).is_none_or(|end| end > b.len()) {
            break;
        }
        let (Some(camera_type), Some(fov), Some(far_clip), Some(near_clip)) = (
            b.u32_at(rec).map(|v| v as i32),
            b.f32_at(rec + 0x04),
            b.f32_at(rec + 0x08),
            b.f32_at(rec + 0x0c),
        ) else {
            break;
        };
        let (Some(position_base), Some(target_base)) = (
            rd_c3(b, rec + position).map(|c| [c.x, c.y, c.z]),
            rd_c3(b, rec + target_base).map(|c| [c.x, c.y, c.z]),
        ) else {
            break;
        };
        out.push(M2Camera {
            camera_type,
            fov,
            far_clip,
            near_clip,
            positions: track_spline_vec3(b, rec + 0x10),
            position_base,
            target: track_spline_vec3(b, rec + target),
            target_base,
            roll: track_spline_f32(b, rec + roll),
        });
    }
    out
}

/// Parse the MD20 CameraLookup (`0x118` in Wrath, `0x12c` in the donor; `u16` each).
pub fn parse_camera_lookup(b: &[u8]) -> Vec<u16> {
    let header = if b.u32_at(4).is_some_and(|v| v >= 264) { 0x118 } else { 0x12c };
    let (Some(count), Some(ofs)) = (b.u32_at(header), b.u32_at(header + 4)) else {
        return Vec::new();
    };
    let avail = b.len().saturating_sub(ofs as usize);
    let mut out = Vec::with_capacity(capped(count as usize, 2, avail));
    for i in 0..count as usize {
        let Some(v) = b.u16_at(ofs as usize + i * 2) else {
            break;
        };
        out.push(v);
    }
    out
}

/// Parse the version-0x101 texture combiner presets (header `0x144`, `u16` each), the array the
/// reference's fixup takes as the 0x101 header's tail (`0x71d284`); see
/// [`M2Model::texture_combiners`]. A short header or table yields what fits.
pub fn parse_texture_combiners(b: &[u8]) -> Vec<u16> {
    let (Some(count), Some(ofs)) = (b.u32_at(0x144), b.u32_at(0x148)) else {
        return Vec::new();
    };
    let avail = b.len().saturating_sub(ofs as usize);
    let mut out = Vec::with_capacity(capped(count as usize, 2, avail));
    for i in 0..count as usize {
        let Some(v) = b.u16_at(ofs as usize + i * 2) else {
            break;
        };
        out.push(v);
    }
    out
}

/// Attachment `id`'s model-space Z through the lookup, from the file's `(count, offset)` pairs.
fn attachment_z(b: &[u8], id: usize, attachments: (u32, u32), lookup: (u32, u32)) -> Option<f32> {
    if id >= lookup.0 as usize {
        return None;
    }
    let li = lookup.1 as usize + id * 2;
    let idx = b.u16_at(li)? as usize;
    if idx >= attachments.0 as usize {
        return None;
    }
    let rec = attachments.1 as usize + idx * 48;
    // `position` is at +8 of the 48-byte record, so its Z is at +16.
    b.f32_at(rec + 16)
}

#[cfg(test)]
mod tests;
