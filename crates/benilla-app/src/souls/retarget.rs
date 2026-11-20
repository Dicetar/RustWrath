//! Elden Ring motion on our rig and relayed Souls players: kit actions play the ER clip each
//! row was read from (`source`), and, on their own switch, idle, guard, walk, run, sprint, crouch
//! and airborne play the ER locomotion clips.
//! Touchdown changes only the visual source of a jump attack. The air and landing clips
//! share the action clock, with hit opening, hit closing and recovery end aligned. The same
//! mapping drives the upper-body carry after movement releases the attack.
//!
//! **The clips** are the reference's bake, `player_anims.bin` (`ERAN`: model-space joint positions
//! per frame at 30 fps, plus the head's axes and optional weapon axes, with root motion removed).
//! It is read from `benilla-config/combat/player_anims.bin`, next to `souls.toml` and `player.kit`,
//! on a worker thread the first time `F10` turns the mode on with a retarget switch set, and again when the
//! file is newer. The folder is local and ignored: the bake is never committed.
//!
//! **The retarget** maps the ER skeleton onto the M2 one by role, from the bind pivots (an M2
//! bind pose has identity rotations, so a pivot is the sum of the bind translations up its
//! chain), the Head key bone, and the hand and elbow attachments:
//!
//! - the spine, pelvis to Head, takes ER `Pelvis…Head` by arc length; each spine bone gets a
//!   whole frame, up along the chain and its side blended from the hips' line to the shoulders',
//!   so the torso twists with the swing;
//! - the Head takes the bake's head axes;
//! - each distinct hand attachment aligns the held item's shaft and bit with the baked weapon
//!   axes when available. After blending, the equipment attachment point is seated at its
//!   authored position in the wrist's coordinate system;
//! - arm segments aim along the ER arm; the wrist takes the knuckle/thumb frame;
//! - the pelvis uses anatomical up. Each leg reaches for the scaled ER ankle with the ER knee
//!   as its pole. An ankle-to-toe frame transfers pitch relative to a planted foot. The bind
//!   height of the sole accounts for different boots and the digitigrade Tauren hoof on the
//!   ground. In the air the ankle follows the baked offset from the hip.
//!
//! Every other bone keeps the stock animation. The pass writes the bone locals in
//! [`benilla_world::rig_anim::PosePost`], after the M2 evaluator and before the strafe twist and
//! compose, so held items follow the hands.
//!
//! **Hot-togglable.** A clip change blends from the last written pose over the clip's own blend
//! frames. With the mode, both switches or the clips off, the pass fades back to the stock pose
//! in [`FADE_OUT_SECS`] and then writes nothing; a bone the evaluator does not re-key gets its
//! stock value back.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use benilla_combat_core::{Kit, KitAction, Milli};
use benilla_world::rig_anim::RigPose;
use bevy::prelude::*;

use super::{current_kit, running_action, souls_on, SoulsConfig, SoulsState, YARDS_PER_METRE};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::creature_anim::{move_flags, AnimDriver, BodyTwist};
use crate::entities::{BoneAttach, HandKeyBones};
use crate::net::SelfPlayer;

/// The bake's rate.
const CLIP_FPS: f32 = 30.0;
/// Seconds back to the stock pose once nothing plays.
const FADE_OUT_SECS: f32 = 0.2;
/// The shortest blend into a clip, seconds.
const MIN_BLEND_SECS: f32 = 0.05;
/// ESTIMATE: locomotion changes (idle to walk, walk to run, a stance swap) blend at least this
/// long; an ER gait change reads soft, never as a pop.
const MIN_LOCO_BLEND_SECS: f32 = 0.18;
/// ESTIMATE: the guard's arms rise over a moving body, and the stock draw/stow ceremony takes the
/// arms, in this long; they let go as fast.
const UPPER_BLEND_SECS: f32 = 0.15;
/// `Item.dbc`: armour class, its shield subclass.
const ITEM_CLASS_ARMOR: u8 = 4;
const ARMOR_SUBCLASS_SHIELD: u8 = 6;
/// The file's name, next to `souls.toml`.
const CLIP_FILE: &str = "player_anims.bin";
/// Fade a released swing into the gait over the last part of its authored recovery.
const CARRY_FADE_SECS: f32 = 0.15;

/// The ER joints the retarget reads, in [`JOINT_NAMES`] order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum J {
    Pelvis,
    Spine,
    Spine1,
    Spine2,
    Neck,
    Head,
    LClavicle,
    LUpperArm,
    LForearm,
    LHand,
    RClavicle,
    RUpperArm,
    RForearm,
    RHand,
    LThigh,
    LCalf,
    LFoot,
    LToe0,
    RThigh,
    RCalf,
    RFoot,
    RToe0,
    LFinger2,
    RFinger2,
    LFinger0,
    RFinger0,
}

const JOINTS: usize = 26;

/// The bake's joint names, by [`J`].
const JOINT_NAMES: [&str; JOINTS] = [
    "Pelvis",
    "Spine",
    "Spine1",
    "Spine2",
    "Neck",
    "Head",
    "L_Clavicle",
    "L_UpperArm",
    "L_Forearm",
    "L_Hand",
    "R_Clavicle",
    "R_UpperArm",
    "R_Forearm",
    "R_Hand",
    "L_Thigh",
    "L_Calf",
    "L_Foot",
    "L_Toe0",
    "R_Thigh",
    "R_Calf",
    "R_Foot",
    "R_Toe0",
    "L_Finger2",
    "R_Finger2",
    "L_Finger0",
    "R_Finger0",
];

const SPINE: [J; 6] = [J::Pelvis, J::Spine, J::Spine1, J::Spine2, J::Neck, J::Head];
/// Clavicle to the middle knuckle, `[right, left]`.
const ARMS: [[J; 5]; 2] = [
    [
        J::RClavicle,
        J::RUpperArm,
        J::RForearm,
        J::RHand,
        J::RFinger2,
    ],
    [
        J::LClavicle,
        J::LUpperArm,
        J::LForearm,
        J::LHand,
        J::LFinger2,
    ],
];
/// Thigh to toe, `[right, left]`.
const LEGS: [[J; 4]; 2] = [
    [J::RThigh, J::RCalf, J::RFoot, J::RToe0],
    [J::LThigh, J::LCalf, J::LFoot, J::LToe0],
];
/// The thumb's base, `[right, left]`: which way the palm faces.
const THUMBS: [J; 2] = [J::RFinger0, J::LFinger0];
/// The hand attachment ids, `[right, left]`.
const HAND_ATTACH: [u16; 2] = [1, 2];
/// The elbow attachment ids, `[right, left]`.
const ELBOW_ATTACH: [u16; 2] = [3, 4];

/// The stand-still idle, the rest pose the mapping measures the ER skeleton in.
const IDLE: u32 = 0;
/// The one-handed guard, held still.
const GUARD: u32 = 100;
/// Walk, run and their crouched forms, each `+0` forward, `+1` back, `+2` left, `+3` right.
const WALK: u32 = 20000;
const RUN: u32 = 20100;
const SPRINT: u32 = 20200;
const CROUCH_IDLE: u32 = 300_000;
const CROUCH_WALK: u32 = 320_000;
const CROUCH_RUN: u32 = 320_100;
/// The airborne loop, for jumps and falls alike.
const AIRBORNE: u32 = 202_040;

/// Locomotion speeds, metres per second: below `STILL` the body stands, below `WALK_TOP` it walks.
const STILL: f32 = 0.3;
const WALK_TOP: f32 = 2.8;
/// The speed each locomotion clip shows at rate 1, metres per second. ESTIMATE: the bake drops
/// root motion, so these are judged from the ER gaits, and the clip rate follows the real speed.
const WALK_CLIP_SPEED: f32 = 1.6;
const RUN_CLIP_SPEED: f32 = 4.5;
const SPRINT_CLIP_SPEED: f32 = 6.5;
const CROUCH_WALK_CLIP_SPEED: f32 = 1.2;
const CROUCH_RUN_CLIP_SPEED: f32 = 3.0;

/// The stock base clips ER locomotion may stand in for: Stand 0, Walk 4, Run 5, ShuffleLeft 11,
/// ShuffleRight 12, Walkbackwards 13, the ready stances 25-29, JumpStart/Jump/JumpEnd/Fall
/// 37-40, StealthWalk 119, StealthStand 120, Sprint 143 and JumpLandRun 187. Anything else (a cast, an emote, a
/// swim, a mount) keeps the stock animation.
fn stands_in_for(base: u16) -> bool {
    matches!(base, 0 | 4 | 5 | 11..=13 | 25..=29 | 37..=40 | 119 | 120 | 143 | 187)
}

/// One baked clip.
struct ErClip {
    /// Blend-in length, frames.
    blend: f32,
    frames: usize,
    data: Vec<f32>,
}

/// The bake, the joints the retarget reads resolved to their offsets in a pose row.
struct ErClips {
    stride: usize,
    at: [usize; JOINTS],
    /// The head's X axis; Y and Z follow.
    head_axes: usize,
    /// Optional X-axis offsets for the right and left weapons. Y follows X in each row.
    weapon_axes: [Option<usize>; 2],
    clips: std::collections::HashMap<String, ErClip>,
}

/// One sampled ER pose in the M2 model frame (`+X` right, `+Y` up, `−Z` forward).
#[derive(Clone, Copy, Debug)]
struct Pose {
    p: [Vec3; JOINTS],
    head_up: Vec3,
    head_fwd: Vec3,
    /// Baked (X, Y) directions in model space, indexed like `HAND_ATTACH`.
    weapon_axes: [Option<(Vec3, Vec3)>; 2],
}

impl Pose {
    fn at(&self, j: J) -> Vec3 {
        self.p[j as usize]
    }
}

/// The bake's space (`+X` left, `+Y` up, `+Z` forward) to the model's: a half turn about up.
fn er_to_model(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(-x, y, -z)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| format!("truncated at byte {}", self.at))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn name(&mut self) -> Result<String, String> {
        let n = usize::from(self.take(1)?[0]);
        let raw = self.take(n)?;
        String::from_utf8(raw.to_vec()).map_err(|_| "a name is not UTF-8".to_string())
    }
}

impl ErClips {
    /// Parse the bake.
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != b"ERAN" {
            return Err("not an ERAN file".into());
        }
        let joints = r.u32()? as usize;
        let oriented = r.u32()? as usize;
        let count = r.u32()? as usize;
        let joint_names = (0..joints)
            .map(|_| r.name())
            .collect::<Result<Vec<_>, _>>()?;
        let oriented_names = (0..oriented)
            .map(|_| r.name())
            .collect::<Result<Vec<_>, _>>()?;
        let stride = joints * 3 + oriented * 9;
        let mut at = [0; JOINTS];
        for (slot, name) in at.iter_mut().zip(JOINT_NAMES) {
            *slot = joint_names
                .iter()
                .position(|j| j == name)
                .ok_or_else(|| format!("no joint {name}"))?
                * 3;
        }
        let axes = |name: &str| {
            oriented_names
                .iter()
                .position(|j| j == name)
                .map(|i| joints * 3 + i * 9)
        };
        let head_axes = axes("Head").ok_or("no Head axes")?;
        let weapon_axes = ["R_Weapon", "L_Weapon"].map(axes);
        let mut clips = std::collections::HashMap::with_capacity(count);
        for _ in 0..count {
            let name = r.name()?;
            let blend = r.f32()?;
            let frames = r.u32()? as usize;
            let floats = frames.checked_mul(stride).ok_or("a clip is too long")?;
            let raw = r.take(floats.checked_mul(4).ok_or("a clip is too long")?)?;
            let data: Vec<f32> = raw
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            if frames > 0 && data.iter().all(|v| v.is_finite()) {
                clips.insert(
                    name,
                    ErClip {
                        blend,
                        frames,
                        data,
                    },
                );
            }
        }
        Ok(Self {
            stride,
            at,
            head_axes,
            weapon_axes,
            clips,
        })
    }

    /// The pose at `frame`, between samples; a looped clip wraps, another holds its ends.
    fn sample(&self, clip: &ErClip, frame: f32, looped: bool) -> Pose {
        let last = (clip.frames - 1) as f32;
        let frame = if looped && last > 0.0 {
            frame.rem_euclid(last)
        } else {
            frame.clamp(0.0, last)
        };
        let i = (frame.floor() as usize).min(clip.frames - 1);
        let j = (i + 1).min(clip.frames - 1);
        let t = frame - i as f32;
        let a = &clip.data[i * self.stride..][..self.stride];
        let b = &clip.data[j * self.stride..][..self.stride];
        let v = |at: usize| {
            let lerp = |k: usize| a[at + k] + (b[at + k] - a[at + k]) * t;
            er_to_model(lerp(0), lerp(1), lerp(2))
        };
        Pose {
            p: std::array::from_fn(|k| v(self.at[k])),
            head_up: v(self.head_axes),
            head_fwd: v(self.head_axes + 6),
            weapon_axes: self.weapon_axes.map(|at| at.map(|x| (v(x), v(x + 3)))),
        }
    }

    fn named(&self, id: u32) -> Option<&ErClip> {
        self.clips.get(&format!("a000_{id:06}"))
    }

    /// The clip in a weapon stance's bank (`a{bank:03}`), else the common one: a stance carries
    /// its own idle and guard, and a two-handed one its own gait, but nothing else.
    fn named_in(&self, bank: u8, id: u32) -> Option<&ErClip> {
        (bank != 0)
            .then(|| self.clips.get(&format!("a{bank:03}_{id:06}")))
            .flatten()
            .or_else(|| self.named(id))
    }

    /// The rest pose: the idle's first frame.
    fn rest(&self) -> Option<Pose> {
        let idle = self.named(IDLE)?;
        Some(self.sample(idle, 0.0, false))
    }
}

/// A point on the ER skeleton: `t` of the way from joint `a` to `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ErPoint {
    a: J,
    b: J,
    t: f32,
}

impl ErPoint {
    fn at(self, pose: &Pose) -> Vec3 {
        pose.at(self.a).lerp(pose.at(self.b), self.t)
    }
}

/// How one bone is posed.
#[derive(Clone, Copy, Debug)]
enum Drive {
    /// Turned from its parent's frame along the shortest arc that points `bind_dir` (pivot to the
    /// next pivot down the chain) from `from` to `to`.
    Aim {
        from: ErPoint,
        to: ErPoint,
        bind_dir: Vec3,
    },
    /// A whole model frame: up from `from` to `to`, the side `side_mix` of the way from the
    /// hips' line to the shoulders'.
    Torso {
        from: ErPoint,
        to: ErPoint,
        bind_up: Vec3,
        side_mix: f32,
    },
    /// The bake's head axes.
    Head,
    /// A whole hand frame: up from the ER hand to its middle knuckle, the side toward the thumb's
    /// base; `bind_up` and `bind_side` are the same in the M2 bind pose.
    Hand {
        hand: J,
        finger: J,
        thumb: J,
        bind_up: Vec3,
        bind_side: Vec3,
    },
    /// The attachment takes the weapon axes. Its origin is seated after all pose layers blend.
    Grip { side: usize },
}

/// A held item's actual attachment point in the wrist's coordinate system.
#[derive(Debug)]
struct PalmSocket {
    /// Wrist child first, ending at the socket bone.
    chain: Vec<usize>,
    offset: Vec3,
    held: Vec3,
}

impl PalmSocket {
    fn point(&self, locals: &[Transform]) -> Vec3 {
        self.chain
            .iter()
            .rev()
            .fold(self.offset, |p, &b| locals[b].transform_point(p))
    }

    fn place(&self, locals: &mut [Transform], wrist_point: Vec3) -> bool {
        let (&bone, helpers) = self.chain.split_last().expect("nonempty socket chain");
        let parent = helpers
            .iter()
            .fold(bevy::math::Affine3A::IDENTITY, |m, &b| {
                m * locals[b].compute_affine()
            });
        if parent.matrix3.determinant().abs() <= f32::EPSILON {
            return false;
        }
        let socket = &mut locals[bone];
        let translation = parent.inverse().transform_point3(wrist_point)
            - socket.rotation * (socket.scale * self.offset);
        if !translation.is_finite() || socket.translation == translation {
            return false;
        }
        socket.translation = translation;
        true
    }
}

/// The roles of one rig's bones.
#[derive(Debug)]
struct RigMap {
    /// Driven bones, parents first.
    driven: Vec<(usize, Drive)>,
    grips: [Option<PalmSocket>; 2],
    pelvis: usize,
    /// Model units per ER metre.
    scale: f32,
    /// The ER pelvis at rest.
    rest_pelvis: Vec3,
    legs: Vec<Leg>,
    upper_arms: [usize; 2],
    clavicles: [Option<usize>; 2],
    stand_offset: f32,
}

/// The three joints that position the leg. The stock clip still poses the deform helpers
/// under them, including twists, hocks, and toe flexion.
#[derive(Debug)]
struct Leg {
    joints: [J; 4],
    /// Hip, knee hinge, and foot hinge on the path to the toe, in parent-first order.
    bones: [usize; 3],
    upper: Vec3,
    /// Stock locals on the knee-to-foot path, excluding the two hinges.
    shin_helpers: Vec<usize>,
    contact: Vec3,
    neutral_toe: Vec3,
    scale: f32,
}

fn model_parent(parents: &[i16], work: &[Transform], bone: usize) -> GlobalTransform {
    let mut model = GlobalTransform::IDENTITY;
    let mut child = bone;
    while let Some(parent) = parent_of(parents, child).filter(|&p| p < child) {
        model = GlobalTransform::from(work[parent]) * model;
        child = parent;
    }
    model
}

impl Leg {
    fn build(chain: &[usize], piv: &[Vec3], joints: [J; 4], rest: &Pose, eps: f32) -> Option<Self> {
        let height = piv[*chain.first()?].y;
        // A short HD hip spacer precedes the first long descending link. The next distinct
        // pivot is the knee. Coincident ankle/foot bones are one landmark.
        let hi = (0..chain.len().checked_sub(1)?)
            .find(|&i| piv[chain[i]].y - piv[chain[i + 1]].y > height * 0.20)?;
        let hip = chain[hi];
        let knee = chain[hi + 1];
        let after = |start: usize| {
            (start + 1..chain.len()).find(|&i| piv[chain[i]].distance(piv[chain[start]]) > eps)
        };
        let shin_end = after(hi + 1)?;
        let shin = piv[chain[shin_end]] - piv[knee];
        // A backward-sloping hock ends above the hoof. Preserve its geometry as part of the
        // lower leg. The classic hoof has no toe pivot at all.
        let ankle_i = if shin.z > -shin.y && shin.z > height * 0.10 {
            after(shin_end).unwrap_or(shin_end)
        } else {
            shin_end
        };
        // HD rigs put a shin helper at the ankle, followed by the actual rotating foot at
        // the same pivot. The last bone at this pivot carries the toe. A knee sibling at
        // the same position as the hinge is outside this path and keeps its thigh twist.
        let toe_i = after(ankle_i);
        let foot_i = toe_i.unwrap_or(chain.len()) - 1;
        let ankle = piv[chain[foot_i]];
        let toe = toe_i.map_or(ankle + Vec3::NEG_Z * ankle.y, |t| piv[chain[t]]);
        let upper = piv[knee] - piv[hip];
        let lower = ankle - piv[knee];
        let [h, k, a, t] = joints.map(|j| rest.at(j));
        let er_length = h.distance(k) + k.distance(a);
        let er_toe = t - a;
        if er_length < eps || upper.length() < eps || lower.length() < eps {
            return None;
        }
        Some(Self {
            joints,
            bones: [hip, knee, chain[foot_i]],
            upper,
            shin_helpers: chain[hi + 2..foot_i].to_vec(),
            contact: Vec3::new(toe.x - ankle.x, -ankle.y, toe.z - ankle.z),
            neutral_toe: Vec3::new(0., er_toe.y, -er_toe.x.hypot(er_toe.z)),
            scale: (upper.length() + lower.length()) / er_length,
        })
    }

    fn pose(
        &self,
        pose: &Pose,
        parents: &[i16],
        binds: &[Vec3],
        work: &mut [Transform],
        grounded: bool,
    ) {
        let [h, k, a, t] = self.joints.map(|j| pose.at(j));
        let hips = (pose.at(J::RThigh) - pose.at(J::LThigh)).normalize_or(Vec3::X);
        let thigh = k - h;
        let shin = a - k;
        let plane = shin.cross(thigh);
        let side = if plane.length_squared() > 1e-6 {
            let n = plane.normalize();
            if n.dot(hips) < 0. {
                -n
            } else {
                n
            }
        } else {
            hips
        };
        let tip = (t - a).normalize_or(Vec3::NEG_Z);
        // Extract yaw from the toe when it has a forward projection. As a swinging foot
        // passes through vertical, the knee plane keeps the sagittal frame continuous.
        let lateral = tip.cross(Vec3::Y);
        let weight = (lateral.dot(side) * 3.).clamp(0., 1.);
        let foot_side = side.lerp(lateral.normalize_or(side), weight);
        let Some(foot) = basis(tip, foot_side)
            .zip(basis(self.neutral_toe, Vec3::X))
            .map(|(now, rest)| now * rest.inverse())
        else {
            return;
        };
        let [hip_bone, _, foot_bone] = self.bones;
        let mut reach = (a - h) * self.scale;
        if grounded {
            let hip = model_parent(parents, work, hip_bone).transform_point(binds[hip_bone]);
            reach.y = t.y * self.scale - (foot * self.contact).y - hip.y;
        }
        let axis = reach.normalize_or(Vec3::NEG_Y);
        let u = self.upper.length();
        // Fold the stock helper locals into the effective shin before solving. This is just
        // the foot's bind translation on a classic human, but includes a moving Tauren hock.
        let shin_offset = self
            .shin_helpers
            .iter()
            .rev()
            .fold(binds[foot_bone], |p, &b| work[b].transform_point(p));
        let l = shin_offset.length();
        if !l.is_finite() || l <= f32::EPSILON {
            return;
        }
        let margin = u.min(l) * 0.0001;
        let d = reach.length().clamp((u - l).abs() + margin, u + l - margin);
        let along = (u * u - l * l + d * d) / (2. * d);
        let bend = (u * u - along * along).max(0.).sqrt();
        let pole = (thigh - axis * thigh.dot(axis))
            .normalize_or(side.cross(axis).normalize_or(Vec3::NEG_Z));
        let upper = axis * along + pole * bend;
        let lower = axis * d - upper;
        let solved_side =
            lower
                .cross(upper)
                .try_normalize()
                .map_or(side, |s| if s.dot(side) < 0. { -s } else { s });
        let orient = |dir, bind| {
            basis(dir, solved_side)
                .zip(basis(bind, Vec3::X))
                .map(|(now, rest)| now * rest.inverse())
        };
        let (Some(thigh), Some(shin)) = (orient(upper, self.upper), orient(lower, shin_offset))
        else {
            return;
        };
        for (b, desired) in self.bones.into_iter().zip([thigh, shin, foot]) {
            let parent = model_parent(parents, work, b).rotation();
            work[b] = Transform::from_translation(binds[b])
                .with_rotation((parent.inverse() * desired).normalize());
        }
    }
}

fn parent_of(parents: &[i16], i: usize) -> Option<usize> {
    usize::try_from(*parents.get(i)?)
        .ok()
        .filter(|&p| p < parents.len() && p != i)
}

/// `i` and its ancestors, nearest first; capped against a cycle.
fn ancestors(parents: &[i16], i: usize) -> Vec<usize> {
    let mut out = vec![i];
    let mut cur = i;
    while let Some(p) = parent_of(parents, cur) {
        if out.len() > parents.len() {
            break;
        }
        out.push(p);
        cur = p;
    }
    out
}

/// Model-space bind pivots: with identity bind rotations, the bind translations summed up the
/// chain.
fn pivots(parents: &[i16], binds: &[Vec3]) -> Vec<Vec3> {
    (0..binds.len())
        .map(|i| ancestors(parents, i).iter().map(|&b| binds[b]).sum())
        .collect()
}

/// Cumulative arc lengths along `points`.
fn arc(points: &[Vec3]) -> Vec<f32> {
    let mut out = Vec::with_capacity(points.len());
    let mut sum = 0.0;
    for (k, p) in points.iter().enumerate() {
        if k > 0 {
            sum += p.distance(points[k - 1]);
        }
        out.push(sum);
    }
    out
}

/// Each chain point's place on the ER polyline `er`. `anchors` pair point indices with polyline
/// indices, both strictly increasing, first and last included; a point between two anchors lands
/// at the same fraction of arc length on the polyline between them, measured in `rest`.
fn map_chain(points: &[Vec3], er: &[J], rest: &Pose, anchors: &[(usize, usize)]) -> Vec<ErPoint> {
    let wow = arc(points);
    let er_points: Vec<Vec3> = er.iter().map(|&j| rest.at(j)).collect();
    let er_arc = arc(&er_points);
    let mut out = vec![
        ErPoint {
            a: er[0],
            b: er[0],
            t: 0.0
        };
        points.len()
    ];
    for pair in anchors.windows(2) {
        let ((pa, ea), (pb, eb)) = (pair[0], pair[1]);
        let span_w = wow[pb] - wow[pa];
        let span_e = er_arc[eb] - er_arc[ea];
        for (slot, &w) in out[pa..=pb].iter_mut().zip(&wow[pa..=pb]) {
            let f = if span_w > f32::EPSILON {
                (w - wow[pa]) / span_w
            } else {
                0.0
            };
            let target = er_arc[ea] + f * span_e;
            let mut k = ea;
            while k + 1 < eb && er_arc[k + 1] < target {
                k += 1;
            }
            *slot = if ea == eb {
                ErPoint {
                    a: er[ea],
                    b: er[ea],
                    t: 0.0,
                }
            } else {
                let seg = er_arc[k + 1] - er_arc[k];
                ErPoint {
                    a: er[k],
                    b: er[k + 1],
                    t: if seg > f32::EPSILON {
                        ((target - er_arc[k]) / seg).clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                }
            };
        }
    }
    out
}

/// The first point after `k` apart from it.
fn next_distinct(points: &[Vec3], k: usize, eps: f32) -> Option<usize> {
    (k + 1..points.len()).find(|&j| points[j].distance(points[k]) > eps)
}

/// The rotation whose frame has `up` as `+Y` and `side` (made square to it) as `+X`.
fn basis(up: Vec3, side: Vec3) -> Option<Quat> {
    let y = up.try_normalize()?;
    let x = (side - y * side.dot(y)).try_normalize()?;
    let z = x.cross(y);
    Some(Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize())
}

/// The installed hatchet's shaft is model −Z and its bit is −Y. The ER weapon's shaft is
/// its +Y axis and its bit points along +X. Keep the shaft exact when sampling between frames.
fn grip_rotation((bit, shaft): (Vec3, Vec3)) -> Option<Quat> {
    let x = bit.cross(shaft);
    // Opposing samples can cancel an axis or leave both axes pointing the same way.
    if shaft.length_squared() < 1e-8 || x.length_squared() < 1e-8 {
        return None;
    }
    let z = -shaft.try_normalize()?;
    let x = x.try_normalize()?;
    let y = z.cross(x);
    Some(Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize())
}

/// What the mapping needs from a rig.
struct RigInput<'a> {
    parents: &'a [i16],
    binds: &'a [Vec3],
    head: usize,
    /// The hand attachment bones, `[right, left]`.
    hands: [usize; 2],
    /// Socket-local equipment offsets and authored Stand locals (bind for unkeyed channels).
    hand_offsets: [Vec3; 2],
    stand_local: &'a dyn Fn(usize) -> Transform,
    /// The elbow attachment points in the model frame, `[right, left]`, when the model has them.
    elbows: [Option<Vec3>; 2],
    /// The thumb key bones, `[right, left]`, when the model has them.
    thumbs: [Option<usize>; 2],
}

impl RigMap {
    fn build(rig: &RigInput, rest: &Pose) -> Result<Self, String> {
        let n = rig.binds.len();
        if rig.parents.len() != n || rig.head >= n || rig.hands.iter().any(|&h| h >= n) {
            return Err("bone indices out of range".into());
        }
        let piv = pivots(rig.parents, rig.binds);
        let height = piv[rig.head].y;
        if height <= 0.0 {
            return Err("the Head key bone is not above the origin".into());
        }
        let eps = height * 0.003;
        let head_chain = ancestors(rig.parents, rig.head);
        let on_head_chain = |b: usize| head_chain.contains(&b);
        let children = |b: usize| (0..n).filter(move |&c| parent_of(rig.parents, c) == Some(b));

        // The wrists: the hand attachment's bone, or the bone above when it is a bare attach
        // point; an HD model hangs its hand attachment on bones with a pivot at the model origin,
        // placed only by their animated translation, so the wrist is the first one above with a
        // place in the bind pose.
        let at_origin = |b: usize| piv[b].length() <= eps;
        let wrist_of = |hand: usize| -> Result<usize, String> {
            let mut w = if children(hand).next().is_some() {
                hand
            } else {
                parent_of(rig.parents, hand).ok_or("a hand with no parent")?
            };
            while at_origin(w) {
                w = parent_of(rig.parents, w).ok_or("a hand with no placed bone above it")?;
            }
            Ok(w)
        };
        let wrists = [wrist_of(rig.hands[0])?, wrist_of(rig.hands[1])?];
        // Every bone of the arms, hand to shoulder: nothing under them is a foot (an HD model
        // hangs weapon bones below the hand).
        let arm_bones: Vec<usize> = rig
            .hands
            .iter()
            .flat_map(|&h| ancestors(rig.parents, h))
            .filter(|&b| !on_head_chain(b))
            .collect();

        // The feet: the lowest bone on each side at the end of a leg, at least three bones
        // hanging from the spine and not from an arm. An HD model carries free roots and
        // one-bone cloth and ground bones at the floor, which are no legs.
        let side_width = height * 0.02;
        let on_leg = |b: usize| {
            let up = ancestors(rig.parents, b);
            up.iter()
                .position(|&a| on_head_chain(a))
                .is_some_and(|k| k >= 3)
                && !up.iter().any(|a| arm_bones.contains(a))
        };
        let lowest = |right: bool| {
            (0..n)
                .filter(|&b| !on_head_chain(b) && on_leg(b))
                .filter(|&b| {
                    if right {
                        piv[b].x > side_width
                    } else {
                        piv[b].x < -side_width
                    }
                })
                .min_by(|&a, &b| piv[a].y.total_cmp(&piv[b].y))
        };
        let toes = [
            lowest(true).ok_or("no right foot")?,
            lowest(false).ok_or("no left foot")?,
        ];
        let hip_root = |toe: usize| {
            ancestors(rig.parents, toe)
                .into_iter()
                .find(|&b| on_head_chain(b))
        };
        let pelvis = hip_root(toes[0]).ok_or("the right leg does not join the spine")?;
        if hip_root(toes[1]) != Some(pelvis) {
            return Err("the legs hang from different bones".into());
        }
        if piv[pelvis].y < height * 0.25 {
            return Err("the bone the legs hang from is not at the hips".into());
        }
        let rest_pelvis = rest.at(J::Pelvis);
        if rest_pelvis.y <= 0.0 {
            return Err("the ER rest pelvis is not above the ground".into());
        }
        let scale = piv[pelvis].y / rest_pelvis.y;

        let mut driven = Vec::new();
        let mut grips = [None, None];

        // The spine, pelvis to Head.
        let mut spine: Vec<usize> = head_chain
            .iter()
            .copied()
            .take_while(|&b| b != pelvis)
            .collect();
        spine.push(pelvis);
        spine.reverse();
        if spine.len() < 2 {
            return Err("the Head hangs from the hips".into());
        }
        let points: Vec<Vec3> = spine.iter().map(|&b| piv[b]).collect();
        let last = spine.len() - 1;
        let mapped = map_chain(&points, &SPINE, rest, &[(0, 0), (last, SPINE.len() - 1)]);
        let wow_arc = arc(&points);
        let er_arc = arc(&SPINE.map(|j| rest.at(j)));
        // The side is the shoulders' from the ER Spine2 up.
        let shoulder_frac = (er_arc[3] / er_arc[SPINE.len() - 1]).max(f32::EPSILON);
        for (k, &bone) in spine.iter().enumerate() {
            if k == last {
                driven.push((bone, Drive::Head));
                continue;
            }
            let Some(j) = next_distinct(&points, k, eps) else {
                continue;
            };
            let frac = wow_arc[k] / wow_arc[last].max(f32::EPSILON);
            driven.push((
                bone,
                Drive::Torso {
                    from: mapped[k],
                    // The first spine helper can sit BELOW the pelvis. Aiming that short
                    // offset up flips the hips and both leg sockets. The pelvis frame uses
                    // anatomical up and the ER pelvis-to-spine direction on every model.
                    to: if bone == pelvis {
                        ErPoint {
                            a: J::Spine,
                            b: J::Spine,
                            t: 0.,
                        }
                    } else {
                        mapped[j]
                    },
                    bind_up: if bone == pelvis {
                        Vec3::Y
                    } else {
                        points[j] - points[k]
                    },
                    side_mix: (frac / shoulder_frac).clamp(0.0, 1.0),
                },
            ));
        }

        // The arms, shoulder to wrist.
        if piv[wrists[0]].x <= piv[wrists[1]].x {
            return Err("the right hand is not on the right".into());
        }
        // As with the legs, a bone both arms hang from is neither arm's.
        let shared_arm: Vec<usize> = ancestors(rig.parents, wrists[0])
            .into_iter()
            .filter(|b| ancestors(rig.parents, wrists[1]).contains(b))
            .collect();
        let arm_chain = |wrist: usize| -> Vec<usize> {
            let mut chain: Vec<usize> = ancestors(rig.parents, wrist)
                .into_iter()
                .take_while(|&b| !on_head_chain(b) && !shared_arm.contains(&b))
                .collect();
            chain.reverse();
            chain
        };
        // A clavicle or not, decided once for both arms, from the chain's shape: clavicle,
        // upper arm, forearm and wrist. Matching lengths instead read the HD HumanFemale's
        // short clavicles (0.10-0.15 against ER's 0.20) as upper arms on one side or both,
        // drove them 35-50 degrees off the stock pose and pinched her shoulders.
        let with_clavicle = wrists.iter().all(|&w| arm_chain(w).len() >= 4);
        let mut upper_arms = wrists;
        let mut clavicles = [None; 2];
        for (side, ((&wrist, elbow), arm)) in wrists.iter().zip(rig.elbows).zip(ARMS).enumerate() {
            // The thumb, when it is a placed bone under this wrist.
            let thumb = rig.thumbs[side].filter(|&t| {
                t < n
                    && t != wrist
                    && !at_origin(t)
                    && piv[t].distance(piv[wrist]) > eps
                    && ancestors(rig.parents, t).contains(&wrist)
            });
            let chain = arm_chain(wrist);
            if chain.len() < 2 {
                return Err("an arm with fewer than two bones".into());
            }
            upper_arms[side] = chain[usize::from(with_clavicle)];
            clavicles[side] = with_clavicle.then_some(chain[0]);
            let mut points: Vec<Vec3> = chain.iter().map(|&b| piv[b]).collect();
            let wrist_i = points.len() - 1;
            // The fingers' base, or the forearm carried on a little. An HD hand also carries
            // weapon and attach bones well past the knuckles; only children within half a
            // forearm of the wrist are fingers.
            let forearm = points[wrist_i].distance(points[wrist_i - 1]);
            let fingers: Vec<Vec3> = children(wrist)
                .filter(|&c| !at_origin(c) && Some(c) != thumb)
                .map(|c| piv[c])
                .filter(|p| {
                    let d = p.distance(piv[wrist]);
                    d > eps && d <= forearm * 0.5
                })
                .collect();
            let finger = if fingers.is_empty() {
                piv[wrist] + (piv[wrist] - points[wrist_i - 1]) * 0.25
            } else {
                fingers.iter().copied().sum::<Vec3>() / fingers.len() as f32
            };
            points.push(finger);
            let finger_i = points.len() - 1;
            let total = arc(&points)[wrist_i];
            let er: &[J] = if with_clavicle { &arm } else { &arm[1..] };
            let (elbow_er, hand_er) = (er.len() - 3, er.len() - 2);
            let mut anchors = vec![(0, 0)];
            // The elbow attachment pins the ER elbow to the nearest bone pivot.
            let near = elbow.and_then(|at| {
                (1..wrist_i)
                    .min_by(|&a, &b| points[a].distance(at).total_cmp(&points[b].distance(at)))
                    .filter(|&e| points[e].distance(at) < total * 0.25)
            });
            if let Some(e) = near {
                anchors.push((e, elbow_er));
            }
            anchors.push((wrist_i, hand_er));
            anchors.push((finger_i, er.len() - 1));
            let mapped = map_chain(&points, er, rest, &anchors);
            for (k, &bone) in chain.iter().enumerate() {
                // The clavicle keeps the stock pose. Aimed at ER's shoulder, which sits further
                // forward and in on the narrower ER frame, it swung 30-47 degrees and pinched the
                // shoulders (and their pads) in towards the neck at idle. The arm below still
                // takes ER's directions from the stock shoulder.
                if with_clavicle && k == 0 {
                    continue;
                }
                let Some(j) = next_distinct(&points, k, eps) else {
                    continue;
                };
                // The wrist takes the ER hand's whole frame when the thumb locates the palm.
                if let (true, Some(t)) = (k == wrist_i, thumb) {
                    driven.push((
                        bone,
                        Drive::Hand {
                            hand: arm[3],
                            finger: arm[4],
                            thumb: THUMBS[side],
                            bind_up: points[finger_i] - points[wrist_i],
                            bind_side: piv[t] - piv[wrist],
                        },
                    ));
                    continue;
                }
                driven.push((
                    bone,
                    Drive::Aim {
                        from: mapped[k],
                        to: mapped[j],
                        bind_dir: points[j] - points[k],
                    },
                ));
            }
            let socket = rig.hands[side];
            if socket != wrist {
                let mut chain: Vec<_> = ancestors(rig.parents, socket)
                    .into_iter()
                    .take_while(|&b| b != wrist)
                    .collect();
                let offset = rig.hand_offsets[side];
                let held = chain
                    .iter()
                    .fold(offset, |p, &b| (rig.stand_local)(b).transform_point(p));
                // Origin helpers on some HD rigs are placed entirely by animation. If
                // Stand is unavailable, their unposed origin can be over a yard away.
                let held = if held.length() <= forearm {
                    held
                } else {
                    finger - piv[wrist]
                };
                chain.reverse();
                grips[side] = Some(PalmSocket {
                    chain,
                    offset,
                    held,
                });
                driven.push((socket, Drive::Grip { side }));
            }
        }

        // The legs, hip to toe; the toe follows its foot. A bone both legs hang from (an HD
        // rig's hip root at the body's centre) belongs to neither: one leg would aim it, the
        // other aim it again the opposite way, and the first leg, solved under the first aim,
        // would swing up with it. It keeps the stock pose.
        let shared: Vec<usize> = ancestors(rig.parents, toes[0])
            .into_iter()
            .filter(|b| ancestors(rig.parents, toes[1]).contains(b))
            .collect();
        let mut legs = Vec::with_capacity(2);
        let mut stand_offset = 0.;
        for (&toe, leg) in toes.iter().zip(LEGS) {
            let mut chain: Vec<usize> = ancestors(rig.parents, toe)
                .into_iter()
                .take_while(|b| !shared.contains(b))
                .collect();
            chain.reverse();
            if chain.len() < 2 {
                return Err("a leg with fewer than two bones".into());
            }
            if let Some(mapped) = Leg::build(&chain, &piv, leg, rest, eps) {
                // Calibrate the pelvis against a standing leg on this rig. A tall hoof or
                // short shin changes the bind ankle height without changing the ER clip.
                let bind_delta = piv[mapped.bones[2]] - piv[mapped.bones[0]];
                stand_offset +=
                    bind_delta.y + (rest.at(leg[0]).y - rest.at(leg[2]).y) * mapped.scale;
                legs.push(mapped);
                continue;
            }
            let points: Vec<Vec3> = chain.iter().map(|&b| piv[b]).collect();
            let last = points.len() - 1;
            // Unusual short chains without two separate leg links retain the polyline fallback.
            let hip_y = rest.at(leg[0]).y.max(f32::EPSILON);
            let mut anchors = vec![(0, 0)];
            let mut floor = 0;
            for (er_i, joint) in [(1, leg[1]), (2, leg[2])] {
                let want = points[0].y * rest.at(joint).y / hip_y;
                let pick = (floor + 1..last)
                    .min_by(|&a, &b| {
                        (points[a].y - want)
                            .abs()
                            .total_cmp(&(points[b].y - want).abs())
                    })
                    .filter(|&i| (points[i].y - want).abs() < points[0].y * 0.1);
                if let Some(i) = pick {
                    anchors.push((i, er_i));
                    floor = i;
                }
            }
            anchors.push((last, leg.len() - 1));
            let mapped = map_chain(&points, &leg, rest, &anchors);
            for (k, &bone) in chain.iter().enumerate().take(last) {
                let Some(j) = next_distinct(&points, k, eps) else {
                    continue;
                };
                driven.push((
                    bone,
                    Drive::Aim {
                        from: mapped[k],
                        to: mapped[j],
                        bind_dir: points[j] - points[k],
                    },
                ));
            }
        }

        Ok(Self {
            driven,
            grips,
            upper_arms,
            clavicles,
            pelvis,
            scale: if legs.len() == 2 {
                (legs[0].scale + legs[1].scale) * 0.5
            } else {
                scale
            },
            rest_pelvis,
            stand_offset: if legs.len() == 2 {
                stand_offset * 0.5
            } else {
                0.
            },
            legs,
        })
    }

    /// The retargeted locals of the driven bones over `stock` (every bone's local, stock pose),
    /// written into `work`: rotations of the driven bones and the pelvis translation.
    fn pose(
        &self,
        pose: &Pose,
        parents: &[i16],
        binds: &[Vec3],
        work: &mut [Transform],
        grounded: bool,
    ) {
        let model = |work: &[Transform], b: Option<usize>| -> Quat {
            let Some(b) = b else {
                return Quat::IDENTITY;
            };
            ancestors(parents, b)
                .iter()
                .rev()
                .fold(Quat::IDENTITY, |acc, &a| acc * work[a].rotation)
        };
        let hips = pose.at(J::RThigh) - pose.at(J::LThigh);
        let shoulders = pose.at(J::RUpperArm) - pose.at(J::LUpperArm);
        for &(bone, drive) in &self.driven {
            let parent = model(work, parent_of(parents, bone));
            let desired = match drive {
                Drive::Aim { from, to, bind_dir } => {
                    let (Some(now), Some(want)) = (
                        (parent * bind_dir).try_normalize(),
                        (to.at(pose) - from.at(pose)).try_normalize(),
                    ) else {
                        continue;
                    };
                    Some(Quat::from_rotation_arc(now, want) * parent)
                }
                Drive::Torso {
                    from,
                    to,
                    bind_up,
                    side_mix,
                } => basis(to.at(pose) - from.at(pose), hips.lerp(shoulders, side_mix))
                    .zip(basis(bind_up, Vec3::X))
                    .map(|(now, bind)| now * bind.inverse()),
                Drive::Head => basis(pose.head_up, pose.head_fwd.cross(pose.head_up)),
                Drive::Grip { side } => pose.weapon_axes[side].and_then(grip_rotation),
                Drive::Hand {
                    hand,
                    finger,
                    thumb,
                    bind_up,
                    bind_side,
                } => {
                    let at = pose.at(hand);
                    basis(pose.at(finger) - at, pose.at(thumb) - at)
                        .zip(basis(bind_up, bind_side))
                        .map(|(now, bind)| now * bind.inverse())
                }
            };
            let Some(desired) = desired else {
                continue;
            };
            let local = (parent.inverse() * desired).normalize();
            if local.is_finite() {
                work[bone].rotation = local;
            }
        }
        let offset =
            (pose.at(J::Pelvis) - self.rest_pelvis) * self.scale + Vec3::Y * self.stand_offset;
        let parent = model(work, parent_of(parents, self.pelvis));
        let translation = binds[self.pelvis] + parent.inverse() * offset;
        if translation.is_finite() {
            work[self.pelvis].translation = translation;
        }
        for leg in &self.legs {
            leg.pose(pose, parents, binds, work, grounded);
        }
    }

    fn grip_points(&self, locals: &[Transform]) -> [Vec3; 2] {
        self.grips
            .each_ref()
            .map(|g| g.as_ref().map_or(Vec3::ZERO, |g| g.point(locals)))
    }

    /// Crossfade the rendered attachment point in wrist space. Slerping individual bones
    /// cannot keep this point fixed when a stock helper translates or rotates below the wrist.
    fn blend_grips(
        &self,
        locals: &mut [Transform],
        written: &mut [Transform],
        from: Option<&[Transform]>,
        stock: [Vec3; 2],
        weights: [f32; 2],
        blend: f32,
    ) -> bool {
        let mut dirty = false;
        for (side, grip) in self.grips.iter().enumerate() {
            let Some(grip) = grip else { continue };
            if weights[side] != 0. || blend < 1. {
                let start = from.map_or(stock[side], |pose| grip.point(pose));
                let end = stock[side].lerp(grip.held, weights[side]);
                dirty |= grip.place(locals, start.lerp(end, blend));
            }
            for &b in &grip.chain {
                written[b] = locals[b];
            }
        }
        dirty
    }

    fn measure_stance(&self, rotation: impl Fn(usize) -> Vec2) -> [f32; 5] {
        let pelvis = rotation(self.pelvis);
        let thigh = if self.legs.len() == 2 {
            (rotation(self.legs[0].bones[0]).y - rotation(self.legs[1].bones[0]).y) * 0.5
        } else {
            0.
        };
        let [right, left] = self.clavicles.map(|b| b.map_or(0., |b| rotation(b).y));
        let arm = (rotation(self.upper_arms[0]).y - rotation(self.upper_arms[1]).y) * 0.5;
        [pelvis.x, pelvis.y, thigh, (left - right) * 0.5, arm]
    }

    fn apply_stance(
        &self,
        work: &mut [Transform],
        [pitch, roll, thigh, shoulder, arm]: [f32; 5],
        weight: f32,
    ) {
        let add = |work: &mut [Transform], bone: usize, offset: Quat| {
            work[bone].rotation = (offset * work[bone].rotation).normalize();
        };
        add(
            work,
            self.pelvis,
            Quat::from_rotation_x(pitch * weight) * Quat::from_rotation_z(roll * weight),
        );
        for (side, &upper) in self.upper_arms.iter().enumerate() {
            let sign = if side == 0 { 1. } else { -1. };
            // Clavicles still hold stock. Carry shoulder drop on the driven upper arm instead.
            add(
                work,
                upper,
                Quat::from_rotation_z((arm - shoulder) * sign * weight),
            );
            if let Some(leg) = self.legs.get(side).filter(|_| self.legs.len() == 2) {
                add(
                    work,
                    leg.bones[0],
                    Quat::from_rotation_z(thigh * sign * weight),
                );
            }
        }
    }

    /// The lower body: the legs, the pelvis and the bones above it, which the locomotion keeps
    /// while a released action's upper body carries on.
    fn lower_bones(&self, parents: &[i16]) -> Vec<usize> {
        let mut lower = ancestors(parents, self.pelvis);
        lower.extend(self.legs.iter().flat_map(|leg| leg.bones));
        lower
    }

    /// The bones the pass writes.
    fn bones(&self) -> impl Iterator<Item = usize> + '_ {
        self.driven
            .iter()
            .map(|&(b, _)| b)
            .chain(self.legs.iter().flat_map(|leg| leg.bones))
            .chain(std::iter::once(self.pelvis))
    }
}

/// What plays: a kit action's run, or a locomotion clip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClipKey {
    Action(super::Instance, SwingPart),
    /// The stance bank and the clip id.
    Loco(u8, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwingPart {
    Swing,
    Landing,
}

fn baked_frame(kit: &Kit, frame: Milli) -> f32 {
    frame.0 as f32 / 1000.0 * CLIP_FPS / f32::from(kit.frame_rate_hz.max(1))
}

struct LandingClip {
    source: String,
    /// Corresponding air and landing frames at hit open, hit close, and action end.
    phases: [(f32, f32); 3],
}

impl LandingClip {
    fn frame(&self, elapsed: f32) -> f32 {
        let mut start = (0.0, 0.0);
        for &(air, land) in &self.phases {
            if elapsed < air && air > start.0 {
                return start.1 + (land - start.1) * (elapsed - start.0) / (air - start.0);
            }
            start = (air, land);
        }
        start.1
    }
}

/// Resolve the companion once per action. A jump without a usable landing clip gives the
/// grounded pose back to stock when its last air sample has played.
#[derive(Default)]
enum GroundClip {
    #[default]
    Original,
    Jump(Option<LandingClip>),
}

impl GroundClip {
    fn for_action(kit: &Kit, action: &KitAction) -> Self {
        let name = match action.name.as_str() {
            "JumpLight" => "JumpLightLand",
            "JumpHeavy" => "JumpHeavyLand",
            "PairedJump" => "PairedJumpLand",
            _ => return Self::Original,
        };
        Self::Jump(kit.action_named(name).map(|landing| {
            let pair = |a, b| (baked_frame(kit, a), baked_frame(kit, b));
            let mut phases = [(0.0, 0.0), (0.0, 0.0), pair(action.total, landing.total)];
            if let Some((a, b)) = action.hits.first().zip(landing.hits.first()) {
                phases[0] = pair(a.from, b.from);
                phases[1] = pair(a.to, b.to);
            }
            LandingClip {
                source: landing.source.clone(),
                phases,
            }
        }))
    }

    fn sample<'a>(
        &self,
        clips: &'a ErClips,
        original: &str,
        elapsed: f32,
        grounded: Option<bool>,
    ) -> Option<(SwingPart, &'a ErClip, f32)> {
        if let Self::Jump(landing) = self {
            let air = clips.clips.get(original);
            let past_air = air.is_none_or(|clip| elapsed >= clip.frames as f32);
            if let (true, Some(land)) = (grounded.unwrap_or(past_air), landing) {
                if let Some(clip) = clips.clips.get(&land.source) {
                    return Some((SwingPart::Landing, clip, land.frame(elapsed)));
                }
            }
            if grounded != Some(false) && past_air {
                return None;
            }
            return Some((SwingPart::Swing, air?, elapsed));
        }
        Some((SwingPart::Swing, clips.clips.get(original)?, elapsed))
    }
}

/// The upper body of the last action, carried over the locomotion once moving ended it early.
#[derive(Clone, Debug)]
struct Carry {
    instance: super::Instance,
    source: String,
    /// Baked frames of the movement cancel and the last playable recovery sample.
    release_from: f32,
    end: f32,
}

impl Carry {
    fn frame(&self, tick: u64) -> f32 {
        tick.saturating_sub(self.instance.started) as f32 * CLIP_FPS
            / f32::from(crate::combat_playback::PROFILE_TICK_RATE_HZ)
    }

    fn weight(&self, frame: f32) -> f32 {
        smoothstep(((self.end - frame) / (CARRY_FADE_SECS * CLIP_FPS)).clamp(0.0, 1.0))
    }
}

/// A load on the worker thread: the file's stamp and the parse.
type Pending = Arc<Mutex<Option<(Option<SystemTime>, Result<ErClips, String>)>>>;

/// The retarget's state.
#[derive(Resource, Default)]
pub(super) struct Retarget {
    clips: Option<ErClips>,
    rest: Option<Pose>,
    /// The loaded file's modified time.
    stamp: Option<SystemTime>,
    pending: Option<Pending>,
    /// The missing file was reported.
    told_missing: bool,
    /// The rig the map was built for, its bone count, and the map (or why there is none).
    map: Option<(Entity, usize, Result<RigMap, String>)>,
    playing: Option<ClipKey>,
    /// The pose blended from at the last clip change, per bone; `None` blends from stock.
    from: Option<Vec<Transform>>,
    fade_at: f32,
    fade_len: f32,
    /// What the pass wrote last frame, per bone, when it wrote.
    written: Option<Vec<Transform>>,
    /// The stock local under each written bone (an unkeyed bone keeps the write).
    stock: Vec<Transform>,
    /// Fraction of a locomotion loop, so a change of gait preserves the footfall phase.
    loco_phase: f32,
    last_pos: Option<Vec3>,
    /// The retarget switches were on last frame.
    was_on: bool,
    action_instance: Option<super::Instance>,
    ground_clip: GroundClip,
    /// Touchdown is latched through this swing's recovery, including a movement cancel.
    grounded: bool,
    carry: Option<Carry>,
    /// How far the guard's arms are up over a moving body, and how far the stock draw/stow
    /// ceremony holds the arms, 0 to 1.
    guard_upper: f32,
    ceremony: [f32; 2],
    stance_styles: std::collections::HashMap<(u8, u8), [f32; 5]>,
    style_weight: f32,
    unstyled: Option<Vec<Transform>>,
}

impl Retarget {
    /// Start a load when the file is new or newer than the loaded one.
    fn request_load(&mut self) {
        if self.pending.is_some() {
            return;
        }
        let Some(path) =
            crate::local_state::souls_config_path().map(|p| p.with_file_name(CLIP_FILE))
        else {
            return;
        };
        let stamp = match std::fs::metadata(&path) {
            Ok(meta) => meta.modified().ok(),
            Err(_) => {
                if !self.told_missing {
                    self.told_missing = true;
                    info!(
                        "souls retarget: no {} — copy the reference's assets/{CLIP_FILE} there \
                         for the Elden Ring motion; stock clips play until then",
                        path.display()
                    );
                }
                return;
            }
        };
        if self.clips.is_some() && stamp.is_some() && stamp == self.stamp {
            return;
        }
        let slot: Pending = Arc::default();
        let out = Arc::clone(&slot);
        let spawned = std::thread::Builder::new()
            .name("souls-retarget-load".into())
            .spawn(move || {
                let parsed = std::fs::read(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))
                    .and_then(|bytes| ErClips::parse(&bytes));
                if let Ok(mut guard) = out.lock() {
                    *guard = Some((stamp, parsed));
                }
            });
        match spawned {
            Ok(_) => self.pending = Some(slot),
            Err(e) => warn!("souls retarget: the loader thread did not start: {e}"),
        }
    }

    /// Take a finished load.
    fn poll_load(&mut self) {
        let done = self
            .pending
            .as_ref()
            .and_then(|slot| slot.try_lock().ok().and_then(|mut g| g.take()));
        let Some((stamp, parsed)) = done else {
            return;
        };
        self.pending = None;
        match parsed.and_then(|clips| {
            let rest = clips.rest().ok_or("no idle clip (a000_000000)")?;
            Ok((clips, rest))
        }) {
            Ok((clips, rest)) => {
                info!(
                    "souls retarget: {} Elden Ring clips loaded",
                    clips.clips.len()
                );
                self.clips = Some(clips);
                self.rest = Some(rest);
                self.stamp = stamp;
                // A new rest pose remaps the rig.
                self.map = None;
            }
            Err(e) => warn!("souls retarget: the clips did not load: {e}"),
        }
    }
}

/// `t` eased in and out.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Carry the fraction of a stride into a gait of a different length. The last baked sample
/// duplicates the first, as in ErClips::sample. Entering a gait from a one-shot starts at zero.
fn gait_frame(
    phase: &mut f32,
    previous: Option<u32>,
    id: u32,
    frames: usize,
    delta_frames: f32,
) -> f32 {
    let stride = |k| !matches!(k, IDLE | GUARD | CROUCH_IDLE | AIRBORNE);
    let period = frames.saturating_sub(1).max(1) as f32;
    *phase = if previous.is_some_and(|old| old == id || (stride(old) && stride(id))) {
        (*phase + delta_frames / period).rem_euclid(1.0)
    } else {
        0.0
    };
    *phase * period
}

/// Groups 0 and 1 of the stock pose exclude bones outside the right and left arm, respectively.
fn ceremony_weight(mask: u64, arms: [f32; 2]) -> f32 {
    match mask & 3 {
        2 => arms[0],
        1 => arms[1],
        _ => 0.0,
    }
}

/// Every written bone above the lower body moves `weight` of the way from `work` to `upper`.
fn blend_upper(
    work: &mut [Transform],
    upper: &[Transform],
    map: &RigMap,
    lower: &[usize],
    weight: f32,
) {
    for b in map.bones().filter(|b| !lower.contains(b)) {
        work[b] = Transform {
            translation: work[b].translation.lerp(upper[b].translation, weight),
            rotation: work[b]
                .rotation
                .slerp(upper[b].rotation, weight)
                .normalize(),
            scale: work[b].scale,
        };
    }
}

/// The locomotion clip for the body's state, and its rate.
fn loco_clip(souls: &SoulsState, airborne: bool, forward: f32, right: f32) -> (u32, f32) {
    let speed = forward.hypot(right);
    let dir = if forward.abs() >= right.abs() {
        if forward >= 0.0 {
            0
        } else {
            1
        }
    } else if right < 0.0 {
        2
    } else {
        3
    };
    let rate = |clip_speed: f32| (speed / clip_speed).clamp(0.5, 2.0);
    if airborne {
        (AIRBORNE, 1.0)
    } else if souls.crouched {
        if speed < STILL {
            (CROUCH_IDLE, 1.0)
        } else if speed < WALK_TOP {
            (CROUCH_WALK + dir, rate(CROUCH_WALK_CLIP_SPEED))
        } else {
            (CROUCH_RUN + dir, rate(CROUCH_RUN_CLIP_SPEED))
        }
    } else if speed < STILL {
        (if souls.guarding { GUARD } else { IDLE }, 1.0)
    } else if souls.sprinting {
        (SPRINT, rate(SPRINT_CLIP_SPEED))
    } else if speed < WALK_TOP {
        (WALK + dir, rate(WALK_CLIP_SPEED))
    } else {
        (RUN + dir, rate(RUN_CLIP_SPEED))
    }
}

/// The player's rig mapped onto the ER skeleton, from its Head key bone and its hand and elbow
/// attachments.
fn map_rig(
    rig: &RigPose,
    twist: Option<&BodyTwist>,
    attach: Option<&BoneAttach>,
    hands: Option<&HandKeyBones>,
    anims: Option<&benilla_assets::ModelAnimations>,
    rest: &Pose,
) -> Result<RigMap, String> {
    let head = twist
        .and_then(|t| t.key_bones().1)
        .map(usize::from)
        .ok_or("the model has no Head key bone")?;
    let attach = attach.ok_or("the model has no attachments")?;
    let hand = |side: usize| {
        attach
            .points
            .get(&HAND_ATTACH[side])
            .map(|&(bone, offset)| (usize::from(bone), offset))
            .ok_or("the model has no hand attachments")
    };
    let piv = pivots(&rig.parents, rig.binds());
    let elbow = |side: usize| {
        attach
            .points
            .get(&ELBOW_ATTACH[side])
            .and_then(|&(bone, offset)| piv.get(usize::from(bone)).map(|p| *p + offset))
    };
    let stand = anims.and_then(|anims| {
        let node = anims.pose.node(anims.find(0)?.node)?;
        anims.pose.clips.get(node.clip as usize)
    });
    let hands_and_offsets = [hand(0)?, hand(1)?];
    RigMap::build(
        &RigInput {
            parents: &rig.parents,
            binds: rig.binds(),
            head,
            hands: hands_and_offsets.map(|(bone, _)| bone),
            hand_offsets: hands_and_offsets.map(|(_, offset)| offset),
            stand_local: &|b| {
                let bind = rig.binds()[b];
                let Some(track) =
                    stand.and_then(|s| s.bones.iter().find(|t| usize::from(t.bone) == b))
                else {
                    return Transform::from_translation(bind);
                };
                Transform {
                    translation: track.translation.sample(0.).unwrap_or(bind),
                    rotation: track.rotation.sample(0.).unwrap_or(Quat::IDENTITY),
                    scale: track.scale.sample(0.).unwrap_or(Vec3::ONE),
                }
            },
            elbows: [elbow(0), elbow(1)],
            thumbs: hands.map_or([None, None], |h| h.thumbs.map(|t| t.map(usize::from))),
        },
        rest,
    )
}

fn stance_angles(q: Quat) -> Vec2 {
    let (x, _, z) = q.to_euler(EulerRot::XYZ);
    Vec2::new(x, z)
}

/// Use the resident profile's stock data: the same race/sex can have different bind and
/// animation channels in base and HD. Sample the primary variation of each cycle uniformly.
fn stock_stance(map: &RigMap, anims: &benilla_assets::ModelAnimations) -> [f32; 5] {
    let mut mean = [0.; 5];
    let mut count = 0;
    for id in [0, 4, 5] {
        let Some((clip, pose)) = anims.find(id).and_then(|clip| {
            let node = anims.pose.node(clip.node)?;
            Some((clip, anims.pose.clips.get(node.clip as usize)?))
        }) else {
            continue;
        };
        for frame in 0..32 {
            let time = clip.duration * frame as f32 / 32.;
            let row = map.measure_stance(|bone| {
                pose.bones
                    .iter()
                    .find(|b| usize::from(b.bone) == bone)
                    .and_then(|b| b.rotation.sample(time))
                    .map_or(Vec2::ZERO, stance_angles)
            });
            for (value, sample) in mean.iter_mut().zip(row) {
                *value += sample;
            }
            count += 1;
        }
    }
    if count == 0 {
        mean
    } else {
        mean.map(|angle| angle / count as f32)
    }
}

/// The stock evaluator can key one channel while the others still hold a post-pass write.
fn capture_stock(
    stock: &mut Transform,
    current: Transform,
    written: Option<Transform>,
    twist_rotation: Option<Quat>,
) {
    let Some(last) = written else {
        *stock = current;
        return;
    };
    if current.translation != last.translation {
        stock.translation = current.translation;
    }
    if current.rotation != last.rotation && Some(current.rotation) != twist_rotation {
        stock.rotation = current.rotation;
    }
    if current.scale != last.scale {
        stock.scale = current.scale;
    }
}

/// Pose the local player's rig from the ER clips.
#[allow(clippy::type_complexity)] // one Bevy system's full input set
pub(super) fn drive_retarget(
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    playback: Res<CombatPlaybackState>,
    souls: Res<SoulsState>,
    player: Res<crate::player::Player>,
    mut retarget: ResMut<Retarget>,
    mut me: Query<
        (
            Entity,
            &mut RigPose,
            &Transform,
            Option<&BodyTwist>,
            Option<&BoneAttach>,
            Option<&HandKeyBones>,
            Option<&AnimDriver>,
            Option<&crate::creature_anim::Wielded>,
            Option<&crate::creature_anim::CommittedSheath>,
            Option<&crate::creature_anim::VisualSheath>,
            Option<(&AnimationPlayer, &benilla_assets::ModelAnimations)>,
            Option<&crate::net::ObjectStore>,
        ),
        With<SelfPlayer>,
    >,
) {
    let features = config.features;
    let on = souls_on(&view) && (features.retarget || features.retarget_locomotion);
    let r = &mut *retarget;
    if on && !r.was_on {
        r.request_load();
    }
    r.was_on = on;
    r.poll_load();
    if !on && r.written.is_none() {
        r.playing = None;
        r.last_pos = None;
        return;
    }
    let Ok((
        entity,
        mut rig,
        body,
        twist,
        attach,
        hands,
        driver,
        wielded,
        sheath,
        visual,
        anims,
        store,
    )) = me.single_mut()
    else {
        r.written = None;
        r.playing = None;
        r.carry = None;
        r.last_pos = None;
        return;
    };
    let now = time.elapsed_secs();
    let dt = time.delta_secs();

    // The controller wrote this body's Transform in Update. PosePost precedes global propagation.
    let pos = body.translation;
    let velocity = match r.last_pos {
        Some(last) if dt > 0.0 => (pos - last) / dt / YARDS_PER_METRE,
        _ => Vec3::ZERO,
    };
    r.last_pos = Some(pos);
    let local_v = body.rotation.inverse() * velocity;
    let (forward, right) = (-local_v.z, local_v.x);
    let moving = forward.hypot(right) >= STILL;

    // The weapon's stance banks: its gait and idle, and the guard (a shield held in the off hand
    // guards in the common bank; a weapon guards with itself, held in both hands, as ER's empty
    // left hand does).
    let stance = kit.kit.as_ref().and_then(|kit| kit.stance);
    let both_hands = kit.held_in_both_hands();
    let melee_visible = |slot| {
        visual
            .map(|v| v.for_slot(slot, 0))
            .or_else(|| sheath.and_then(|s| s.state()))
            == Some(1)
    };
    let shield = melee_visible(1)
        && wielded.and_then(|w| w.armed_off()) == Some((ITEM_CLASS_ARMOR, ARMOR_SUBCLASS_SHIELD));
    // Stowed, the hands are empty: the common bank.
    let drawn = melee_visible(0) && wielded.and_then(|w| w.armed_main()).is_some();
    let stowed = (0..2).all(|slot| {
        visual
            .map(|v| v.for_slot(slot, 0))
            .or_else(|| sheath.and_then(|s| s.state()))
            == Some(0)
    });
    let (loco_bank, guard_bank) = match stance.filter(|_| drawn) {
        Some([one, two]) => (
            if both_hands { two } else { one },
            if shield && !both_hands { 0 } else { two },
        ),
        None => (0, 0),
    };

    // The map, rebuilt for a new rig.
    let bones = rig.locals.len();
    if let (Some(rest), true) = (
        r.rest.as_ref(),
        r.map
            .as_ref()
            .is_none_or(|(e, n, _)| *e != entity || *n != bones),
    ) {
        let built = map_rig(
            &rig,
            twist,
            attach,
            hands,
            anims.map(|(_, model)| model),
            rest,
        );
        if let Err(e) = &built {
            warn!("souls retarget: the player's model cannot be mapped, stock clips play: {e}");
        }
        r.map = Some((entity, bones, built));
        r.written = None;
        r.from = None;
        r.playing = None;
        r.action_instance = None;
        r.carry = None;
        r.guard_upper = 0.0;
        r.ceremony = [0.0; 2];
        r.stance_styles.clear();
        r.unstyled = None;
        r.style_weight = 0.;
    }

    // What plays.
    let mut target: Option<(ClipKey, f32, bool, &ErClip)> = None;
    let mut has_action = false;
    let mut attack_style = 1.;
    let airborne = player.airborne();
    r.grounded |= !airborne;
    if on {
        if let Some(clips) = r.clips.as_ref() {
            let running = current_kit(&kit, &profiles)
                .and_then(|kit| running_action(kit, &playback).map(|run| (kit, run)));
            has_action = features.retarget && running.is_some();
            if let Some((kit, (_, action, frame))) = running {
                let remaining = action
                    .hits
                    .iter()
                    .map(|hit| hit.from)
                    .min()
                    .map_or(0., |first| {
                        baked_frame(kit, first) - baked_frame(kit, frame)
                    });
                attack_style =
                    smoothstep((remaining / (UPPER_BLEND_SECS * CLIP_FPS)).clamp(0., 1.));
            }
            // A running action plays its own clip, or the stock one when it has none here (or the
            // switch is off); locomotion never covers it.
            let action = running.filter(|_| features.retarget).and_then(
                |(kit, (instance, action, frame))| {
                    if r.action_instance != Some(instance) {
                        use benilla_combat_core::ActionKind::{MeleeHeavy, MeleeLight};
                        r.action_instance = Some(instance);
                        r.ground_clip = GroundClip::for_action(kit, action);
                        r.grounded = !airborne;
                        r.carry = action
                            .cancel
                            .movement
                            .filter(|_| matches!(action.kind, MeleeLight | MeleeHeavy))
                            .and_then(|from| {
                                let total = baked_frame(kit, action.total);
                                let end = match r.ground_clip {
                                    GroundClip::Original => total
                                        .min((clips.clips.get(&action.source)?.frames - 1) as f32),
                                    GroundClip::Jump(_) => total,
                                };
                                Some(Carry {
                                    instance,
                                    source: action.source.clone(),
                                    release_from: baked_frame(kit, from),
                                    end,
                                })
                            });
                    }
                    let (part, clip, frame) = r.ground_clip.sample(
                        clips,
                        &action.source,
                        baked_frame(kit, frame),
                        Some(r.grounded),
                    )?;
                    Some((ClipKey::Action(instance, part), frame, false, clip))
                },
            );
            target = action.or_else(|| {
                if running.is_some() {
                    return None;
                }
                // The stock driver may still be finishing the clip of an action that already
                // ended (its own swing or dodge): locomotion covers that, not another one-shot.
                let own = |id: u16| profiles.anims.values().any(|&anim| anim == id);
                let stock_loco = driver.is_none_or(|d| {
                    let (base, overlay) = d.playing();
                    overlay.is_none_or(own) && base.is_some_and(|b| stands_in_for(b) || own(b))
                });
                if !features.retarget_locomotion || !stock_loco {
                    return None;
                }
                let (id, rate) = loco_clip(&souls, airborne, forward, right);
                if features.sheathed_stock_gait
                    && stowed
                    && !souls.guarding
                    && ((WALK..WALK + 4).contains(&id) || (RUN..RUN + 4).contains(&id))
                    && driver.is_some_and(|d| matches!(d.playing().0, Some(4 | 5 | 11..=13)))
                {
                    return None;
                }
                let bank = if id == GUARD { guard_bank } else { loco_bank };
                let clip = clips.named_in(bank, id)?;
                let previous = match r.playing {
                    Some(ClipKey::Loco(_, old)) => Some(old),
                    _ => None,
                };
                let gait = gait_frame(
                    &mut r.loco_phase,
                    previous,
                    id,
                    clip.frames,
                    dt * rate * CLIP_FPS,
                );
                let frame = if id == GUARD { now * CLIP_FPS } else { gait };
                Some((ClipKey::Loco(bank, id), frame, true, clip))
            });
        }
    }
    let key = target.map(|(k, ..)| k);
    let in_loco = matches!(key, Some(ClipKey::Loco(..)));
    let tick = playback.ticks_at().0;
    if in_loco {
        r.carry.take_if(|carry| {
            let frame = carry.frame(tick);
            !features.retarget
                || frame >= carry.end
                || frame < carry.release_from
                || (matches!(r.playing, Some(ClipKey::Action(i, _)) if i == carry.instance)
                    && !moving)
        });
    } else if key.is_none() && !has_action {
        r.action_instance = None;
        r.carry = None;
    }
    let Some((_, _, Ok(map))) = r.map.as_ref() else {
        r.unstyled = None;
        r.written = None;
        return;
    };
    if target.is_none() && r.written.is_none() {
        r.playing = None;
        return;
    }

    if r.stock.len() != bones {
        r.stock = rig.locals.clone();
    }
    for b in map.bones() {
        capture_stock(
            &mut r.stock[b],
            rig.locals[b],
            r.written.as_ref().map(|w| w[b]),
            twist.and_then(|t| t.last_written(b)),
        );
    }

    // A clip change starts a blend from what was last written.
    if key != r.playing {
        r.from = r.unstyled.take().or_else(|| r.written.clone());
        r.fade_at = now;
        r.fade_len = match target {
            Some((ClipKey::Loco(..), ..))
                if matches!(r.playing, Some(ClipKey::Action(..))) && r.carry.is_some() =>
            {
                MIN_BLEND_SECS
            }
            Some((ClipKey::Loco(..), .., clip)) => (clip.blend / CLIP_FPS).max(MIN_LOCO_BLEND_SECS),
            Some((.., clip)) => (clip.blend / CLIP_FPS).max(MIN_BLEND_SECS),
            None => FADE_OUT_SECS,
        };
        r.playing = key;
    }
    // Eased: a linear crossfade starts and stops with a visible kink.
    let k = smoothstep(((now - r.fade_at) / r.fade_len.max(f32::EPSILON)).clamp(0.0, 1.0));

    // The arms' own layers over the locomotion: the guard raised while moving, the stock
    // draw/stow ceremony (its masked arm clips are in the stock pose).
    let step = if UPPER_BLEND_SECS > 0.0 {
        dt / UPPER_BLEND_SECS
    } else {
        1.0
    };
    let approach = |value: f32, on: bool| {
        if on {
            (value + step).min(1.0)
        } else {
            (value - step).max(0.0)
        }
    };
    let style = store
        .and_then(|s| s.0.unit_race().zip(s.0.unit_gender()))
        .zip(anims)
        .filter(|_| features.sex_stance)
        .map(|(key, (_, model))| {
            *r.stance_styles
                .entry(key)
                .or_insert_with(|| stock_stance(map, model))
        });
    let style_on = features.sex_stance && in_loco && !airborne;
    r.style_weight = approach(r.style_weight, style_on);
    r.guard_upper = approach(r.guard_upper, in_loco && souls.guarding && moving);
    // The model loader assigns mask groups 0/1 to bones outside the right/left arm.
    // The stock driver can finish one arm's ceremony while the other is still playing.
    for (side, weight) in r.ceremony.iter_mut().enumerate() {
        let active = in_loco
            && driver.is_some_and(AnimDriver::sheath_ceremony_active)
            && anims.is_some_and(|(player, model)| {
                player
                    .playing_animations()
                    .any(|(node, _)| model.pose.node(*node).is_some_and(|n| n.mask == 1 << side))
            });
        *weight = approach(*weight, active);
    }

    let mut work: Vec<Transform> = rig.locals.clone();
    for b in map.bones() {
        work[b] = r.stock[b];
    }
    let stock_grips = map.grip_points(&work);
    if let (Some((key, frame, looped, clip)), Some(clips)) = (target, r.clips.as_ref()) {
        let pose = clips.sample(clip, frame, looped);
        map.pose(&pose, &rig.parents, rig.binds(), &mut work, !airborne);
        // Moving cut the last action short: its upper body settles over the running legs.
        let carried = r
            .carry
            .as_ref()
            .filter(|_| matches!(key, ClipKey::Loco(..)))
            .and_then(|carry| {
                let elapsed = carry.frame(tick);
                let (_, clip, frame) =
                    r.ground_clip
                        .sample(clips, &carry.source, elapsed, Some(r.grounded))?;
                Some((carry.weight(elapsed), clip, frame))
            });
        if let Some((weight, clip, frame)) = carried {
            let mut upper: Vec<Transform> = rig.locals.clone();
            for b in map.bones() {
                upper[b] = r.stock[b];
            }
            let pose = clips.sample(clip, frame, false);
            map.pose(&pose, &rig.parents, rig.binds(), &mut upper, !airborne);
            let lower = map.lower_bones(&rig.parents);
            blend_upper(&mut work, &upper, map, &lower, weight);
        }
        // The guard held while moving: its arms over the gait's legs.
        let guard_clip = (in_loco && r.guard_upper > 0.0)
            .then(|| clips.named_in(guard_bank, GUARD))
            .flatten();
        if let Some(clip) = guard_clip {
            let mut upper: Vec<Transform> = rig.locals.clone();
            for b in map.bones() {
                upper[b] = r.stock[b];
            }
            let pose = clips.sample(clip, now * CLIP_FPS, true);
            map.pose(&pose, &rig.parents, rig.binds(), &mut upper, !airborne);
            let lower = map.lower_bones(&rig.parents);
            blend_upper(&mut work, &upper, map, &lower, smoothstep(r.guard_upper));
        }
    }

    let weight = smoothstep(r.style_weight) * attack_style;
    let unstyled = style.filter(|_| weight > 0.).map(|style| {
        let unstyled = work.clone();
        map.apply_stance(&mut work, style, weight);
        unstyled
    });

    // Blend and write.
    let arm_weights = r.ceremony.map(smoothstep);
    let masks = anims
        .filter(|_| in_loco && arm_weights != [0.0; 2])
        .map(|(_, model)| model.pose.bone_masks.as_slice());
    let mut written = r.written.take().unwrap_or_else(|| rig.locals.clone());
    let mut unstyled_out = unstyled.as_ref().map(|_| written.clone());
    let mut dirty = false;
    for b in map.bones() {
        let base = r.from.as_ref().map_or(r.stock[b], |from| from[b]);
        if let (Some(source), Some(out)) = (&unstyled, &mut unstyled_out) {
            out[b] = Transform {
                translation: base.translation.lerp(source[b].translation, k),
                rotation: base.rotation.slerp(source[b].rotation, k).normalize(),
                scale: source[b].scale,
            };
        }
        let mut out = Transform {
            translation: base.translation.lerp(work[b].translation, k),
            rotation: base.rotation.slerp(work[b].rotation, k).normalize(),
            scale: work[b].scale,
        };
        if let Some(&mask) = masks.and_then(|m| m.get(b)) {
            let w = ceremony_weight(mask, arm_weights);
            if w > 0.0 {
                out.rotation = out.rotation.slerp(r.stock[b].rotation, w).normalize();
                out.translation = out.translation.lerp(r.stock[b].translation, w);
            }
        }
        if !(out.translation.is_finite() && out.rotation.is_finite()) {
            continue;
        }
        if rig.locals[b] != out {
            rig.locals[b] = out;
            dirty = true;
        }
        written[b] = out;
    }
    let grip_weights = if target.is_some() {
        if masks.is_some() {
            arm_weights.map(|w| 1. - w)
        } else {
            [1.; 2]
        }
    } else {
        [0.; 2]
    };
    dirty |= map.blend_grips(
        &mut rig.locals,
        &mut written,
        r.from.as_deref(),
        stock_grips,
        grip_weights,
        k,
    );
    if dirty {
        rig.pose_dirty = true;
    }
    r.unstyled = unstyled_out;
    if target.is_none() && k >= 1.0 {
        r.unstyled = None;
        // Faded out: the stock pose is back, and the pass stops writing.
        r.written = None;
        r.from = None;
        r.playing = None;
    } else {
        r.written = Some(written);
    }
}

/// The relay is a name on older scripts, or an action id on newer ones. Resolve against our
/// local kit; never invent a clip for an unknown action.
pub(super) fn relayed_row<'a>(
    kit: Option<&'a benilla_combat_core::Kit>,
    action: &str,
) -> Option<&'a benilla_combat_core::KitAction> {
    let kit = kit?;
    action
        .parse::<u32>()
        .ok()
        .and_then(|id| kit.action(benilla_combat_core::ActionId(id)))
        .or_else(|| kit.action_named(action))
}

#[derive(Component)]
pub(super) struct RemoteAction {
    pub(super) source: String,
    pub(super) started: f32,
    pub(super) duration: f32,
}

impl RemoteAction {
    /// The relay already resolved the clip source against the receiver's kit.
    fn jump(&self, kit: Option<&Kit>) -> (GroundClip, Option<f32>) {
        if let Some(kit) = kit {
            for name in ["JumpLight", "JumpHeavy", "PairedJump"] {
                if let Some(row) = kit
                    .action_named(name)
                    .filter(|row| row.source == self.source)
                {
                    return (
                        GroundClip::for_action(kit, row),
                        row.cancel.movement.map(|from| baked_frame(kit, from)),
                    );
                }
            }
        }
        (GroundClip::Original, None)
    }
}

#[derive(Component)]
pub(super) struct RemotePeer {
    pub(super) until: f32,
    pub(super) sprint: bool,
}

fn remote_gait(speed: f32, sprint: bool) -> (u32, f32) {
    if speed < STILL {
        (IDLE, 1.0)
    } else if sprint && speed >= WALK_TOP {
        (SPRINT, (speed / SPRINT_CLIP_SPEED).clamp(0.5, 2.0))
    } else if speed < WALK_TOP {
        (WALK, (speed / WALK_CLIP_SPEED).clamp(0.5, 2.0))
    } else {
        (RUN, (speed / RUN_CLIP_SPEED).clamp(0.5, 2.0))
    }
}

/// Bounds include fading bodies; overflow keeps the generic animation.
const MAX_REMOTE_BODIES: usize = 16;
const MAX_REMOTE_BONES: usize = 512;

#[derive(Clone, Copy, PartialEq)]
enum RemoteClip {
    /// The relay start distinguishes successive swings using the same clip.
    Action(f32, SwingPart),
    Gait(u32),
}

struct RemotePose {
    map: Result<RigMap, String>,
    stock: Vec<Transform>,
    written: Option<Vec<Transform>>,
    from: Vec<Transform>,
    playing: Option<RemoteClip>,
    was_airborne: bool,
    action_started: Option<f32>,
    ground_clip: GroundClip,
    jump_move_cancel: Option<f32>,
    landed: bool,
    carried_action: Option<f32>,
    lower: Vec<usize>,
    fade_at: f32,
    gait_phase: f32,
    last_pos: Vec3,
}

#[derive(Resource, Default)]
struct RemotePoses(std::collections::HashMap<Entity, RemotePose>);

/// Same bake, rig mapping, sampling and pose solver as our own body. No per-body file loads.
/// At most 16 bodies, 512 bones each. A movement-cancelled jump layers its upper body on the gait.
#[allow(clippy::type_complexity)]
fn drive_remote_retarget(
    time: Res<Time>,
    view: Res<CombatModeView>,
    config: Res<SoulsConfig>,
    kit: Res<PlayerKit>,
    profiles: Res<CombatProfiles>,
    retarget: Res<Retarget>,
    mut poses: ResMut<RemotePoses>,
    mut upper: Local<Vec<Transform>>,
    mut bodies: Query<
        (
            Entity,
            &mut RigPose,
            Option<&BodyTwist>,
            Option<&BoneAttach>,
            Option<&HandKeyBones>,
            Option<&RemoteAction>,
            Option<&RemotePeer>,
            &GlobalTransform,
            Option<&AnimDriver>,
            Option<&benilla_assets::ModelAnimations>,
            Option<&crate::net::RemoteMotion>,
        ),
        Without<SelfPlayer>,
    >,
) {
    let on = souls_on(&view) && config.features.server;
    let actions_on = on && config.features.retarget;
    let loco_on = on && config.features.retarget_locomotion;
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    let clips = retarget.clips.as_ref();
    let rest = retarget.rest.as_ref();
    let kit_now = current_kit(&kit, &profiles);
    // Evict despawned bodies before admitting new ones.
    poses.0.retain(|entity, _| bodies.contains(*entity));
    for (entity, rig, twist, attach, hands, action, peer, at, _, anims, _) in &bodies {
        if poses.0.len() >= MAX_REMOTE_BODIES {
            break;
        }
        if poses.0.contains_key(&entity) || rig.locals.len() > MAX_REMOTE_BONES {
            continue;
        }
        let ready =
            actions_on && clips.is_some() && action.is_some_and(|a| now < a.started + a.duration);
        if !(ready || loco_on && peer.is_some_and(|p| now < p.until)) {
            continue;
        }
        let Some(rest) = rest else {
            continue;
        };
        poses.0.insert(
            entity,
            RemotePose {
                map: map_rig(rig, twist, attach, hands, anims, rest),
                stock: rig.locals.clone(),
                written: None,
                from: rig.locals.clone(),
                playing: None,
                was_airborne: false,
                action_started: None,
                ground_clip: GroundClip::Original,
                jump_move_cancel: None,
                landed: false,
                carried_action: None,
                lower: Vec::new(),
                fade_at: now,
                gait_phase: 0.0,
                last_pos: at.translation(),
            },
        );
    }
    poses.0.retain(|entity, state| {
        let Ok((_, mut rig, twist, attach, hands, action, peer, at, driver, anims, motion)) =
            bodies.get_mut(*entity)
        else {
            return false;
        };
        let bones = rig.locals.len();
        if bones > MAX_REMOTE_BONES {
            return false;
        }
        if bones != state.stock.len() {
            state.map = rest.map_or_else(
                || Err("no rest pose".into()),
                |rest| map_rig(&rig, twist, attach, hands, anims, rest),
            );
            state.stock = rig.locals.clone();
            state.from = rig.locals.clone();
            state.written = None;
            state.playing = None;
            state.lower.clear();
            state.fade_at = now;
        }
        let Ok(map) = &state.map else {
            return actions_on && action.is_some_and(|a| now < a.started + a.duration)
                || loco_on && peer.is_some_and(|p| now < p.until);
        };
        let mut work = rig.locals.clone();
        for b in map.bones() {
            capture_stock(
                &mut state.stock[b],
                work[b],
                state.written.as_ref().map(|w| w[b]),
                twist.and_then(|t| t.last_written(b)),
            );
            work[b] = state.stock[b];
        }
        let stock_grips = map.grip_points(&work);
        let delta = at.translation() - state.last_pos;
        state.last_pos = at.translation();
        let speed = if dt > 0.0 {
            delta.x.hypot(delta.z) / dt / YARDS_PER_METRE
        } else {
            0.0
        };
        let running = action.filter(|a| now < a.started + a.duration);
        let flags = motion.map(|m| m.flags);
        let airborne = flags.is_some_and(|f| f & move_flags::FALLING != 0);
        let touchdown = state.was_airborne && flags.is_some() && !airborne;
        state.was_airborne = airborne;
        let mut pose_airborne = airborne;
        let mut carried = None;
        let mut target = running.filter(|_| actions_on).and_then(|a| {
            let frame = (now - a.started).max(0.0) * CLIP_FPS;
            // SWING and movement arrive separately. Latch a witnessed touchdown, while a
            // first grounded snapshot can still be corrected by a late jump flag.
            if state.action_started != Some(a.started) {
                (state.ground_clip, state.jump_move_cancel) = a.jump(kit_now);
                state.action_started = Some(a.started);
                state.landed = false;
            }
            state.landed |= touchdown;
            let grounded = if state.landed {
                Some(true)
            } else {
                airborne.then_some(false)
            };
            if loco_on
                && state.jump_move_cancel.is_some_and(|from| frame >= from)
                && flags.is_some_and(|f| f & move_flags::ANY_MOVE != 0)
            {
                state.carried_action = Some(a.started);
            }
            let (part, clip, sample) = state
                .ground_clip
                .sample(clips?, &a.source, frame, grounded)?;
            if matches!(state.ground_clip, GroundClip::Jump(_)) {
                pose_airborne = part == SwingPart::Swing && grounded != Some(true);
            }
            if loco_on && state.carried_action == Some(a.started) {
                let left = a.duration - frame / CLIP_FPS;
                let weight = smoothstep((left / CARRY_FADE_SECS).clamp(0.0, 1.0));
                carried = Some((clip, sample, weight));
                return None;
            }
            Some((RemoteClip::Action(a.started, part), clip, sample))
        });
        let carrying =
            actions_on && running.is_some_and(|a| state.carried_action == Some(a.started));
        if target.is_none() && (running.is_none() || carrying) && loco_on {
            let stock_loco = driver.is_some_and(|d| {
                let (base, overlay) = d.playing();
                overlay.is_none()
                    && matches!(
                        base,
                        Some(0 | 4 | 5 | 11..=13 | 25..=29 | 37..=40 | 143 | 187)
                    )
            });
            let peer = peer.filter(|p| now < p.until);
            if carrying || (stock_loco && peer.is_some()) {
                let (id, rate) = if pose_airborne {
                    (AIRBORNE, 1.0)
                } else {
                    remote_gait(speed, peer.is_some_and(|p| p.sprint))
                };
                if let Some(clip) = clips.and_then(|clips| clips.named(id)) {
                    let previous = match state.playing {
                        Some(RemoteClip::Gait(old)) => Some(old),
                        _ => None,
                    };
                    let frame = gait_frame(
                        &mut state.gait_phase,
                        previous,
                        id,
                        clip.frames,
                        dt * rate * CLIP_FPS,
                    );
                    target = Some((RemoteClip::Gait(id), clip, frame));
                }
            }
        }
        let playing = target.map(|(key, ..)| key);
        if playing != state.playing {
            state.from = state.written.clone().unwrap_or_else(|| state.stock.clone());
            state.fade_at = now;
            state.playing = playing;
        }
        let fade_len = match target {
            Some((key, clip, frame)) => {
                let gait = matches!(key, RemoteClip::Gait(_));
                if let Some(clips) = clips {
                    let pose = clips.sample(clip, frame, gait);
                    map.pose(&pose, &rig.parents, rig.binds(), &mut work, !pose_airborne);
                }
                let minimum = if gait {
                    MIN_LOCO_BLEND_SECS
                } else {
                    MIN_BLEND_SECS
                };
                (clip.blend / CLIP_FPS).max(minimum)
            }
            None => FADE_OUT_SECS,
        };
        if let (Some(RemoteClip::Gait(_)), Some((clip, frame, weight)), Some(clips)) =
            (playing, carried, clips)
        {
            upper.clone_from(&rig.locals);
            for b in map.bones() {
                upper[b] = state.stock[b];
            }
            map.pose(
                &clips.sample(clip, frame, false),
                &rig.parents,
                rig.binds(),
                &mut upper,
                !pose_airborne,
            );
            if state.lower.is_empty() {
                state.lower = map.lower_bones(&rig.parents);
            }
            blend_upper(&mut work, &upper, map, &state.lower, weight);
        }
        let blend = smoothstep(((now - state.fade_at) / fade_len).clamp(0.0, 1.0));
        let mut written = rig.locals.clone();
        for b in map.bones() {
            let from = state.from[b];
            let out = Transform {
                translation: from.translation.lerp(work[b].translation, blend),
                rotation: from.rotation.slerp(work[b].rotation, blend).normalize(),
                scale: work[b].scale,
            };
            if !out.translation.is_finite() || !out.rotation.is_finite() {
                continue;
            }
            if rig.locals[b] != out {
                rig.locals[b] = out;
                rig.pose_dirty = true;
            }
            written[b] = out;
        }
        let grip_weight = if target.is_some() { 1. } else { 0. };
        if map.blend_grips(
            &mut rig.locals,
            &mut written,
            Some(&state.from),
            stock_grips,
            [grip_weight; 2],
            blend,
        ) {
            rig.pose_dirty = true;
        }
        state.written = Some(written);
        target.is_some() || blend < 1.0
    });
}

/// Register [`drive_retarget`] in the pose post-pass, before the strafe twist composes onto the
/// spine and head.
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Retarget>()
        .init_resource::<RemotePoses>()
        .add_systems(
            PostUpdate,
            (drive_retarget, drive_remote_retarget)
                .chain()
                .in_set(benilla_world::rig_anim::PosePost)
                .before(crate::creature_anim::apply_body_twist),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample the installed stock clip underneath the ER pose. The keys are expressed in
    /// WoW coordinates; the production loader applies the same basis change per channel.
    fn stock_pose(
        binds: &[Vec3],
        anim: &benilla_formats::ModelAnimation,
        time: f32,
    ) -> Vec<Transform> {
        use benilla_assets::{coords::wow_to_bevy, PoseTrack};
        let mut locals: Vec<_> = binds
            .iter()
            .copied()
            .map(Transform::from_translation)
            .collect();
        for keys in &anim.bones {
            let local = &mut locals[usize::from(keys.bone)];
            let translations: Vec<_> = keys
                .translation
                .iter()
                .map(|&(t, v)| (t, wow_to_bevy(v)))
                .collect();
            if let Some(delta) = PoseTrack::new(&translations).sample(time) {
                local.translation += delta;
            }
            let rotations: Vec<_> = keys
                .rotation
                .iter()
                .map(|&(t, [x, y, z, w])| (t, Quat::from_xyzw(-y, z, -x, w).normalize()))
                .collect();
            if let Some(q) = PoseTrack::new(&rotations).sample(time) {
                local.rotation = q;
            }
            let scales: Vec<_> = keys
                .scale
                .iter()
                .map(|&(t, [x, y, z])| (t, Vec3::new(y, z, x)))
                .collect();
            if let Some(scale) = PoseTrack::new(&scales).sample(time) {
                local.scale = scale;
            }
        }
        locals
    }

    const PLAYER_RACES: [&str; 10] = [
        "Human", "Orc", "Dwarf", "NightElf", "Scourge", "Tauren", "Gnome", "Troll", "BloodElf",
        "Draenei",
    ];

    /// Mean signed stock rotations relative to bind: pelvis pitch/roll, bilateral
    /// outward thigh, downward clavicle and outward upper-arm roll, in degrees.
    #[test]
    #[ignore = "requires WOW_DATA and ERAN_FILE from the owner's install"]
    fn installed_stance_offsets() {
        use benilla_assets::coords::wow_to_bevy;
        use std::path::{Path, PathBuf};
        let data = PathBuf::from(std::env::var_os("WOW_DATA").unwrap());
        let bake = std::fs::read(std::env::var_os("ERAN_FILE").unwrap()).unwrap();
        let rest = ErClips::parse(&bake).unwrap().rest().unwrap();
        let manifests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/profiles");
        for profile in ["base", "wm-hd"] {
            let chain = benilla_assets::open_profile_chain(
                &data,
                &manifests.join(format!("{profile}.mpqlist")),
                profile,
            )
            .unwrap();
            for race in PLAYER_RACES {
                let mut means = [[0.; 5]; 2];
                for (mean, sex) in means.iter_mut().zip(["Male", "Female"]) {
                    let path = format!(r"Character\{race}\{sex}\{race}{sex}");
                    let bytes = chain.read(&format!("{path}.m2")).unwrap();
                    let skel = benilla_formats::parse_m2_skeleton(&bytes).unwrap();
                    let att = benilla_formats::parse_m2_attachments(&bytes).unwrap();
                    let key = |id| skel.bones.iter().position(|b| b.key_bone == id);
                    let parents: Vec<_> = skel.bones.iter().map(|b| b.parent).collect();
                    let piv: Vec<_> = skel.bones.iter().map(|b| wow_to_bevy(b.pivot)).collect();
                    let binds: Vec<_> = (0..piv.len())
                        .map(|i| piv[i] - parent_of(&parents, i).map_or(Vec3::ZERO, |p| piv[p]))
                        .collect();
                    let anims = benilla_formats::parse_m2_animations_with_external(
                        &bytes,
                        |id, variation| {
                            matches!(id, 0 | 4 | 5)
                                .then(|| {
                                    chain
                                        .read(&format!("{path}{id:04}-{variation:02}.anim"))
                                        .ok()
                                })
                                .flatten()
                        },
                    );
                    let stand = anims.iter().find(|a| a.anim_id == 0).unwrap();
                    let standing = stock_pose(&binds, stand, 0.);
                    let hands = HAND_ATTACH.map(|id| att.iter().find(|a| a.id == id).unwrap());
                    let input = RigInput {
                        parents: &parents,
                        binds: &binds,
                        head: key(6).unwrap(),
                        hands: hands.map(|a| usize::from(a.bone)),
                        hand_offsets: hands
                            .map(|a| wow_to_bevy(a.position) - piv[usize::from(a.bone)]),
                        stand_local: &|b| standing[b],
                        elbows: ELBOW_ATTACH.map(|id| {
                            att.iter()
                                .find(|a| a.id == id)
                                .map(|a| wow_to_bevy(a.position))
                        }),
                        thumbs: [key(12), key(17)],
                    };
                    let map = match RigMap::build(&input, &rest) {
                        Ok(map) if map.legs.len() == 2 => map,
                        result => {
                            println!("STANCE_MISSING {profile} {race}{sex}: {result:?}");
                            continue;
                        }
                    };
                    for id in [0, 4, 5] {
                        let anim = anims.iter().find(|a| a.anim_id == id).unwrap();
                        for frame in 0..32 {
                            let locals =
                                stock_pose(&binds, anim, anim.duration * frame as f32 / 32.);
                            let rot = |bone: usize| {
                                let (x, _, z) = locals[bone].rotation.to_euler(EulerRot::XYZ);
                                (x.to_degrees(), z.to_degrees())
                            };
                            let (pitch, roll) = rot(map.pelvis);
                            let thigh =
                                (rot(map.legs[0].bones[0]).1 - rot(map.legs[1].bones[0]).1) * 0.5;
                            let shoulder = (map.clavicles[1].map_or(0., |b| rot(b).1)
                                - map.clavicles[0].map_or(0., |b| rot(b).1))
                                * 0.5;
                            let arm = (rot(map.upper_arms[0]).1 - rot(map.upper_arms[1]).1) * 0.5;
                            for (sum, value) in
                                mean.iter_mut().zip([pitch, roll, thigh, shoulder, arm])
                            {
                                *sum += value / 96.;
                            }
                        }
                    }
                }
                let delta = std::array::from_fn::<_, 5, _>(|i| means[1][i] - means[0][i]);
                println!("STANCE {profile} {race} {delta:.3?}");
            }
        }
    }

    /// Both profiles supply real skeletons, four-weight meshes, stock clips and ER samples.
    #[test]
    #[ignore = "requires WOW_DATA and ERAN_FILE from the owner's install"]
    fn installed_player_limbs() {
        use benilla_assets::coords::wow_to_bevy;
        use std::path::{Path, PathBuf};
        let data = PathBuf::from(std::env::var_os("WOW_DATA").unwrap());
        let bake = std::fs::read(std::env::var_os("ERAN_FILE").unwrap()).unwrap();
        let clips = ErClips::parse(&bake).unwrap();
        let rest = clips.rest().unwrap();
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/profiles");
        let mut worst_pose = (0.0_f32, String::new());
        for (profile, landmarks) in [
            (
                "base",
                [
                    [[6, 13, 23, 36], [5, 12, 21, 35]],
                    [[8, 15, 22, 33], [7, 14, 20, 32]],
                    [[8, 17, 24, 39], [7, 16, 22, 38]],
                    [[5, 13, 45, 45], [6, 14, 46, 46]],
                ],
            ),
            (
                "wm-hd",
                [
                    [[6, 26, 40, 46], [8, 27, 41, 47]],
                    [[5, 17, 30, 36], [6, 18, 31, 37]],
                    [[15, 21, 34, 44], [16, 22, 35, 45]],
                    [[5, 13, 31, 40], [6, 14, 34, 41]],
                ],
            ),
        ] {
            let chain = benilla_assets::open_profile_chain(
                &data,
                &manifest_dir.join(format!("{profile}.mpqlist")),
                profile,
            )
            .unwrap();
            for ((race, sex), sides) in [
                ("Human", "Female"),
                ("Human", "Male"),
                ("Orc", "Male"),
                ("Tauren", "Male"),
            ]
            .into_iter()
            .zip(landmarks)
            {
                let path = format!(r"Character\{race}\{sex}\{race}{sex}");
                let bytes = chain.read(&format!("{path}.m2")).unwrap();
                let skel = benilla_formats::parse_m2_skeleton(&bytes).unwrap();
                let skin = benilla_formats::parse_external_skin(
                    &chain.read(&format!("{path}00.skin")).unwrap(),
                )
                .unwrap();
                let meshes = benilla_formats::parse_m2_render_submeshes_with_skin(
                    &bytes,
                    "",
                    &[],
                    Some(&skin),
                )
                .unwrap();
                let att = benilla_formats::parse_m2_attachments(&bytes).unwrap();
                let key = |id| skel.bones.iter().position(|b| b.key_bone == id);
                let point = |id| att.iter().find(|a| a.id == id).unwrap();
                let parents: Vec<_> = skel.bones.iter().map(|b| b.parent).collect();
                let piv: Vec<_> = skel.bones.iter().map(|b| wow_to_bevy(b.pivot)).collect();
                let binds: Vec<_> = (0..piv.len())
                    .map(|i| piv[i] - parent_of(&parents, i).map_or(Vec3::ZERO, |p| piv[p]))
                    .collect();
                let anims =
                    benilla_formats::parse_m2_animations_with_external(&bytes, |id, variation| {
                        matches!(id, 0 | 4 | 5 | 18)
                            .then(|| {
                                chain
                                    .read(&format!("{path}{id:04}-{variation:02}.anim"))
                                    .ok()
                            })
                            .flatten()
                    });
                let stand = anims.iter().find(|a| a.anim_id == 0).unwrap();
                let standing = stock_pose(&binds, stand, 0.);
                let rig = RigInput {
                    parents: &parents,
                    binds: &binds,
                    head: key(6).unwrap(),
                    hands: HAND_ATTACH.map(|id| usize::from(point(id).bone)),
                    hand_offsets: HAND_ATTACH.map(|id| {
                        let a = point(id);
                        wow_to_bevy(a.position) - piv[usize::from(a.bone)]
                    }),
                    stand_local: &|b| standing[b],
                    elbows: ELBOW_ATTACH.map(|id| {
                        att.iter()
                            .find(|a| a.id == id)
                            .map(|a| wow_to_bevy(a.position))
                    }),
                    thumbs: [key(12), key(17)],
                };
                let map = RigMap::build(&rig, &rest).unwrap();
                // Use independently identified anatomical landmarks. A wrong Leg::build
                // classification must not change the oracle for the skinned helper vertices.
                let helper: Vec<bool> = (0..parents.len())
                    .map(|b| {
                        sides.iter().any(|&[hip, knee, foot, _]| {
                            ![hip, knee, foot].contains(&b) && ancestors(&parents, b).contains(&hip)
                        })
                    })
                    .collect();
                if (profile, race, sex) == ("wm-hd", "Human", "Female") {
                    // Socket 210 hangs from helper 115 under wrist 62. Measure the rendered
                    // grip in wrist space, through every stock Attack2HTight variation.
                    let a = point(1);
                    assert_eq!(a.bone, 210);
                    assert_eq!((parents[210], parents[115]), (115, 62));
                    let offset = wow_to_bevy(a.position) - piv[210];
                    let seat = |locals: &[Transform]| {
                        locals[115].transform_point(locals[210].transform_point(offset))
                    };
                    let stand_seat = seat(&standing);
                    let heavy = &clips.clips["a030_032505"];
                    let mut old_drift = 0.0_f32;
                    let mut fixed_drift = 0.0_f32;
                    for attack in anims.iter().filter(|a| a.anim_id == 18) {
                        for frame in 0..=(attack.duration * CLIP_FPS).ceil() as usize {
                            let mut work = stock_pose(&binds, attack, frame as f32 / CLIP_FPS);
                            old_drift = old_drift.max(seat(&work).distance(stand_seat));
                            let pose = clips.sample(heavy, frame as f32, false);
                            let stock_grips = map.grip_points(&work);
                            let mut written = work.clone();
                            map.pose(&pose, &parents, &binds, &mut work, true);
                            map.blend_grips(
                                &mut work,
                                &mut written,
                                None,
                                stock_grips,
                                [1.; 2],
                                1.,
                            );
                            let drift = seat(&work).distance(stand_seat);
                            assert!(drift.is_finite());
                            fixed_drift = fixed_drift.max(drift);
                        }
                    }
                    println!("wm-hd HumanFemale heavy grip displacement {old_drift:.6} -> {fixed_drift:.6} yd");
                    assert!(old_drift > 0.6, "missing the stock weapon excursion");
                    assert!(
                        fixed_drift < 0.003,
                        "heavy attack displaced the grip {fixed_drift} yd"
                    );
                }
                let mut stand_world = Vec::new();
                for (i, t) in standing.iter().enumerate() {
                    let local = t.compute_affine();
                    stand_world
                        .push(parent_of(&parents, i).map_or(local, |p| stand_world[p] * local));
                }
                for (clip, frame) in [
                    (IDLE, 0.),
                    (GUARD, 0.),
                    (WALK, 10.),
                    (RUN, 0.),
                    (RUN, 11.),
                    (CROUCH_IDLE, 0.),
                    (27110, 14.),
                ] {
                    let pose = clips.sample(clips.named(clip).unwrap(), frame, false);
                    let stock_id = match clip {
                        WALK => 4,
                        RUN => 5,
                        _ => 0,
                    };
                    let stock_clip = anims.iter().find(|a| a.anim_id == stock_id).unwrap();
                    let stock = stock_pose(&binds, stock_clip, frame / CLIP_FPS);
                    let mut work = stock.clone();
                    map.pose(&pose, &parents, &binds, &mut work, true);
                    if profile == "wm-hd" && race == "Human" && sex == "Female" && clip == IDLE {
                        for clavicle in [34, 35] {
                            assert_eq!(work[clavicle], stock[clavicle], "idle clavicle {clavicle}");
                        }
                    }
                    let mut world = Vec::new();
                    for (i, t) in work.iter().enumerate() {
                        let local = t.compute_affine();
                        world.push(parent_of(&parents, i).map_or(local, |p| world[p] * local));
                    }
                    // Recompose the stock deform branches below the solved landmarks, then
                    // compare the actual linear-blend skin with that expected deformation.
                    let mut expected = world.clone();
                    for (b, &is_helper) in helper.iter().enumerate() {
                        if is_helper {
                            expected[b] = expected[parent_of(&parents, b).unwrap()]
                                * stock[b].compute_affine();
                        }
                    }
                    let mut samples = 0;
                    let mut worst = 0.0_f32;
                    for mesh in &meshes {
                        for ((&position, joints), weights) in
                            mesh.positions.iter().zip(&mesh.joints).zip(&mesh.weights)
                        {
                            let mut error = Vec3::ZERO;
                            let mut influenced = false;
                            for (&joint, &weight) in joints.iter().zip(weights) {
                                let b = usize::from(joint);
                                if weight > 0. && helper[b] {
                                    influenced = true;
                                    let offset = wow_to_bevy(position) - piv[b];
                                    error += weight
                                        * (world[b].transform_point3(offset)
                                            - expected[b].transform_point3(offset));
                                }
                            }
                            if influenced {
                                assert!(error.is_finite());
                                samples += 1;
                                worst = worst.max(error.length());
                            }
                        }
                    }
                    assert!(
                        samples > 0,
                        "{profile} {race}{sex} has no skinned leg helpers"
                    );
                    if worst > worst_pose.0 {
                        worst_pose = (worst, format!("{profile} {race}{sex} {clip}:{frame}"));
                    }
                    println!("{profile} {race}{sex} {clip}:{frame} helper_vertices={samples} max_error={worst:.6} yd");
                    for (side, [hip, knee, ankle, toe]) in sides.into_iter().enumerate() {
                        let at = |b: usize| Vec3::from(world[b].translation);
                        let er = LEGS[side].map(|j| pose.at(j));
                        let foot_forward = world[ankle].transform_vector3(Vec3::NEG_Z).normalize();
                        let sole = world[toe].transform_point3(Vec3::NEG_Y * piv[toe].y);
                        let thigh_error = (at(knee) - at(hip))
                            .angle_between(er[1] - er[0])
                            .to_degrees();
                        let shin_error = (at(ankle) - at(knee))
                            .angle_between(er[2] - er[1])
                            .to_degrees();
                        assert!(
                            thigh_error.is_finite() && shin_error.is_finite() && sole.is_finite()
                        );
                        if clip == IDLE {
                            assert!(
                                foot_forward.dot(Vec3::NEG_Z) > 0.,
                                "{profile} {race}{sex} side {side} faces backward"
                            );
                            assert!((at(ankle).x - at(hip).x) * (er[2].x - er[0].x) >= -0.002);
                        }
                        println!("{profile} {race}{sex} {clip}:{frame} {side} thigh={thigh_error:.1} shin={shin_error:.1} sole={:.3} er_toe={:.3} foot_fwd={foot_forward:.2} ankle={:.2}",sole.y,er[3].y,at(ankle));
                    }
                    // The shoulder pads' attachments (5 right, 6 left) and the bones above
                    // them, against the stock Stand: where they sit and how far they turned.
                    for id in [5u16, 6] {
                        let Some(a) = att.iter().find(|a| a.id == id) else {
                            continue;
                        };
                        let mut b = usize::from(a.bone);
                        let offset = wow_to_bevy(a.position) - piv[b];
                        let moved = world[b]
                            .transform_point3(offset)
                            .distance(stand_world[b].transform_point3(offset));
                        let mut chain = String::new();
                        for _ in 0..4 {
                            let q = Quat::from_mat3a(&world[b].matrix3).normalize();
                            let q0 = Quat::from_mat3a(&stand_world[b].matrix3).normalize();
                            let up = (q * Vec3::Y).angle_between(q0 * Vec3::Y).to_degrees();
                            let driven = map.driven.iter().any(|(d, _)| *d == b);
                            chain += &format!(
                                " b{b}{}:turn={:.0} up={up:.0}",
                                if driven { "*" } else { "" },
                                q.angle_between(q0).to_degrees()
                            );
                            let Some(p) = parent_of(&parents, b) else {
                                break;
                            };
                            b = p;
                        }
                        println!(
                            "{profile} {race}{sex} {clip}:{frame} pad{id} moved={moved:.3}{chain}"
                        );
                    }
                }
            }
        }
        assert!(
            worst_pose.0 < 0.0001,
            "helper skin displaced by {:.5} yards on {}",
            worst_pose.0,
            worst_pose.1
        );
    }

    #[test]
    fn stock_capture_tracks_independently_keyed_channels() {
        let written = Transform {
            translation: Vec3::X,
            rotation: Quat::from_rotation_y(0.5),
            scale: Vec3::splat(2.0),
        };
        let twist = Quat::from_rotation_y(0.8);
        let base = Transform::from_rotation(Quat::from_rotation_z(0.2));
        let mut stock = Transform::IDENTITY;
        capture_stock(&mut stock, base, None, Some(base.rotation));
        assert_eq!(stock, base);

        let mut expected = base;
        expected.rotation = Quat::from_rotation_z(0.4);
        let current = Transform {
            rotation: expected.rotation,
            ..written
        };
        capture_stock(&mut stock, current, Some(written), Some(twist));
        assert_eq!(stock, expected);

        expected.scale = Vec3::splat(3.0);
        let current = Transform {
            rotation: twist,
            scale: expected.scale,
            ..written
        };
        capture_stock(&mut stock, current, Some(written), Some(twist));
        assert_eq!(stock, expected);

        expected.translation = Vec3::Y;
        let current = Transform {
            translation: expected.translation,
            rotation: twist,
            ..written
        };
        capture_stock(&mut stock, current, Some(written), Some(twist));
        assert_eq!(stock, expected);
        capture_stock(&mut stock, written, Some(written), Some(twist));
        assert_eq!(stock, expected);
    }

    #[test]
    fn changing_gait_preserves_stride_but_releasing_a_swing_restarts_it() {
        let mut phase = 0.5;
        assert_eq!(gait_frame(&mut phase, Some(WALK), RUN, 23, 0.0), 11.0);
        assert_eq!(gait_frame(&mut phase, None, RUN, 23, 1.0), 0.0);
        assert_eq!(
            gait_frame(&mut phase, Some(RUN), RUN, 23, 23.0).round(),
            1.0
        );
    }

    #[test]
    fn recovery_uses_the_action_clock_and_ceremony_selects_only_its_arm() {
        let swing = Carry {
            instance: super::super::Instance {
                id: benilla_combat_core::ActionId(1),
                started: 20,
            },
            source: String::new(),
            release_from: 12.,
            end: 36.,
        };
        let frame = swing.frame(20 + u64::from(crate::combat_playback::PROFILE_TICK_RATE_HZ));
        assert_eq!(frame, 30.);
        assert_eq!(swing.weight(frame), 1.);
        assert!((swing.weight(36. - CARRY_FADE_SECS * CLIP_FPS * 0.5) - 0.5).abs() < 1e-5);
        assert_eq!(swing.weight(36.), 0.);
        let arms = [0.8, 0.2];
        assert_eq!(ceremony_weight(0b10, arms), arms[0]);
        assert_eq!(ceremony_weight(0b01, arms), arms[1]);
        assert_eq!(ceremony_weight(0b11, arms), 0.);
    }

    #[test]
    fn remote_gait_uses_speed_and_peer_sprint() {
        assert_eq!(remote_gait(0.0, true).0, IDLE);
        assert_eq!(remote_gait(1.5, false).0, WALK);
        assert_eq!(remote_gait(4.5, false).0, RUN);
        assert_eq!(remote_gait(6.0, true).0, SPRINT);
    }

    #[test]
    fn relay_resolves_local_kit_ids_names_and_unknowns() {
        let kit = benilla_combat_core::Kit::parse("rustwrath-kit 1\nlabel relay\nframe_rate 30\naction 100 dodge\n name RollFront\n source a001\n total 30\n input 15 15\n cancel 20 20 20 never 20 20 never\n stamina 4\n iframes 0 6\nend\n").unwrap();
        assert_eq!(
            relayed_row(Some(&kit), "100").map(|row| row.source.as_str()),
            Some("a001")
        );
        assert_eq!(
            relayed_row(Some(&kit), "RollFront").map(|row| row.id),
            Some(benilla_combat_core::ActionId(100))
        );
        assert!(relayed_row(Some(&kit), "999").is_none());
        assert!(relayed_row(Some(&kit), "Unknown").is_none());
        assert!(relayed_row(None, "100").is_none());
    }

    /// A bake with every joint the retarget reads parses, and a sample between frames is the
    /// mix, turned into the model frame.
    #[test]
    fn bake_parses_and_samples_in_the_model_frame() {
        let mut bytes = b"ERAN".to_vec();
        bytes.extend(u32::try_from(JOINTS).unwrap().to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        for name in JOINT_NAMES.iter().chain(["Head"].iter()) {
            bytes.push(u8::try_from(name.len()).unwrap());
            bytes.extend(name.as_bytes());
        }
        let name = "a000_000000";
        bytes.push(u8::try_from(name.len()).unwrap());
        bytes.extend(name.as_bytes());
        bytes.extend(10.0f32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        let stride = JOINTS * 3 + 9;
        for frame in 0..2 {
            for i in 0..stride {
                let v = if i == 0 { 1.0 + frame as f32 } else { 0.5 };
                bytes.extend(f32::to_le_bytes(v));
            }
        }
        let clips = ErClips::parse(&bytes).expect("parses");
        let clip = clips.named(IDLE).expect("the idle");
        assert_eq!((clip.frames, clip.blend), (2, 10.0));
        let pose = clips.sample(clip, 0.5, false);
        assert_eq!(pose.weapon_axes, [None; 2]);
        // Pelvis x 1.5 in the bake (+X left) is −1.5 in the model (+X right); z flips too.
        assert!((pose.at(J::Pelvis) - Vec3::new(-1.5, 0.5, -0.5)).length() < 1e-6);
        assert!(ErClips::parse(b"ERAX").is_err());
    }

    #[test]
    fn grip_aligns_the_hatchet_under_an_animated_helper() {
        let map = RigMap {
            driven: vec![(2, Drive::Grip { side: 1 })],
            grips: [None, None],
            pelvis: 0,
            scale: 1.0,
            rest_pelvis: Vec3::ZERO,
            legs: Vec::new(),
            stand_offset: 0.0,
        };
        let stock = [
            Transform::from_rotation(Quat::from_rotation_x(0.4)),
            Transform::from_xyz(0.2, 0.1, -0.3).with_rotation(Quat::from_rotation_y(-0.7)),
            Transform::from_rotation(Quat::from_rotation_z(0.3)),
        ];
        let solve = |axes| {
            let pose = Pose {
                p: [Vec3::ZERO; JOINTS],
                head_up: Vec3::Y,
                head_fwd: Vec3::NEG_Z,
                weapon_axes: [None, axes],
            };
            let mut work = stock;
            map.pose(&pose, &[-1, 0, 1], &[Vec3::ZERO; 3], &mut work, true);
            assert_eq!(&work[..2], &stock[..2]);
            work[2].rotation
        };
        let shaft = Vec3::new(-2.0, 1.0, 2.0).normalize();
        let bit = Vec3::new(1.0, 2.0, 0.0).normalize();
        // Interpolated axes can lose orthogonality. The shaft takes priority over the bit.
        let local = solve(Some((bit + shaft * 0.25, shaft * 0.9)));
        let grip = stock[0].rotation * stock[1].rotation * local;
        assert!((grip * Vec3::NEG_Z).distance(shaft) < 1e-5);
        assert!((grip * Vec3::NEG_Y).distance(bit) < 1e-5);
        assert!((grip * Vec3::X).distance(bit.cross(shaft)) < 1e-5);
        for axes in [None, Some((bit, Vec3::ZERO)), Some((shaft, shaft))] {
            assert_eq!(solve(axes), stock[2].rotation);
        }
    }

    /// A chain twice the ER limb's length, with an extra point mid-thigh, lands on the ER joints
    /// at the anchors and at the matching arc fraction between them; a pose equal to the bind
    /// leaves an aimed bone where it was.
    #[test]
    fn chains_map_by_arc_and_a_bind_pose_stays() {
        let mut p = [Vec3::ZERO; JOINTS];
        p[J::RThigh as usize] = Vec3::new(0.1, 1.0, 0.0);
        p[J::RCalf as usize] = Vec3::new(0.1, 0.5, 0.0);
        p[J::RFoot as usize] = Vec3::new(0.1, 0.1, 0.0);
        p[J::RToe0 as usize] = Vec3::new(0.1, 0.0, -0.2);
        let rest = Pose {
            p,
            head_up: Vec3::Y,
            head_fwd: Vec3::NEG_Z,
            weapon_axes: [None; 2],
        };
        let points = [
            Vec3::new(0.2, 2.0, 0.0),
            Vec3::new(0.2, 1.5, 0.0),
            Vec3::new(0.2, 1.0, 0.0),
            Vec3::new(0.2, 0.2, 0.0),
            Vec3::new(0.2, 0.0, -0.4),
        ];
        let leg = LEGS[0];
        let mapped = map_chain(&points, &leg, &rest, &[(0, 0), (2, 1), (3, 2), (4, 3)]);
        assert_eq!((mapped[0].a, mapped[0].t), (J::RThigh, 0.0));
        assert_eq!((mapped[1].a, mapped[1].b), (J::RThigh, J::RCalf));
        assert!((mapped[1].t - 0.5).abs() < 1e-5);
        assert!((mapped[2].at(&rest) - rest.at(J::RCalf)).length() < 1e-5);
        assert!((mapped[4].at(&rest) - rest.at(J::RToe0)).length() < 1e-5);

        // One aimed bone, its pose the bind direction: the local stays identity.
        let map = RigMap {
            legs: Vec::new(),
            grips: [None, None],
            stand_offset: 0.,
            driven: vec![(
                1,
                Drive::Aim {
                    from: mapped[0],
                    to: mapped[2],
                    bind_dir: points[2] - points[0],
                },
            )],
            pelvis: 0,
            scale: 2.0,
            rest_pelvis: Vec3::new(0.0, 1.0, 0.0),
        };
        let mut work = vec![Transform::IDENTITY; 2];
        let parents = [-1i16, 0];
        let binds = [Vec3::new(0.0, 2.0, 0.0), Vec3::ZERO];
        map.pose(&rest, &parents, &binds, &mut work, true);
        assert!(work[1].rotation.angle_between(Quat::IDENTITY) < 1e-4);
        // The rest pelvis (here the origin) is a metre under the map's: the body drops two units.
        assert!((work[0].translation - Vec3::new(0.0, 0.0, 0.0)).length() < 1e-5);
        assert!(basis(Vec3::Y, Vec3::X).is_some_and(|q| q.angle_between(Quat::IDENTITY) < 1e-6));
    }

    /// An HD-shaped rig whose legs hang from one hip root at the body's centre: no bone is driven
    /// twice, and after the whole pass every aimed bone points where its ER limb points (a hip
    /// root aimed by both legs left the right leg swung up).
    #[test]
    fn a_shared_hip_root_is_no_leg_and_every_limb_lands() {
        // (parent, model pivot); the left side mirrors the right.
        let mut bones: Vec<(i16, Vec3)> = vec![
            (-1, Vec3::ZERO),               // 0 root
            (0, Vec3::new(0.0, 1.16, 0.0)), // 1 pelvis
            (1, Vec3::new(0.0, 1.15, 0.0)), // 2 hip root, shared
            (1, Vec3::new(0.0, 1.4, 0.0)),  // 3 chest
            (3, Vec3::new(0.0, 1.65, 0.0)), // 4 neck
            (4, Vec3::new(0.0, 1.75, 0.0)), // 5 head
        ];
        for s in [1.0, -1.0] {
            let at = bones.len() as i16;
            bones.extend([
                (3, Vec3::new(0.04 * s, 1.59, 0.0)),        // clavicle
                (at, Vec3::new(0.145 * s, 1.6, 0.0)),       // upper arm
                (at + 1, Vec3::new(0.30 * s, 1.35, 0.0)),   // forearm
                (at + 2, Vec3::new(0.45 * s, 1.12, 0.0)),   // hand
                (at + 3, Vec3::new(0.50 * s, 1.02, 0.0)),   // finger
                (at + 3, Vec3::new(0.62 * s, 1.0, -0.1)),   // weapon bone
                (at + 3, Vec3::new(0.46 * s, 1.07, -0.05)), // thumb
            ]);
        }
        let mut helpers = Vec::new();
        for s in [1.0, -1.0] {
            let at = bones.len() as i16;
            bones.extend([
                (2, Vec3::new(0.082 * s, 1.117, 0.0)),        // hip
                (at, Vec3::new(0.082 * s, 0.6, 0.0)),         // knee
                (at + 1, Vec3::new(0.082 * s, 0.08, 0.1)),    // end of shin
                (at + 2, Vec3::new(0.082 * s, 0.08, 0.1)),    // foot hinge
                (at + 3, Vec3::new(0.082 * s, 0.028, -0.06)), // toe
                (at, Vec3::new(0.082 * s, 0.6, 0.0)),         // lower-thigh twist
            ]);
            helpers.extend([at as usize + 2, at as usize + 5]);
        }
        let parents: Vec<i16> = bones.iter().map(|b| b.0).collect();
        let binds: Vec<Vec3> = bones
            .iter()
            .map(|&(p, at)| at - usize::try_from(p).map_or(Vec3::ZERO, |p| bones[p].1))
            .collect();

        let mut p = [Vec3::ZERO; JOINTS];
        let joints = [
            (J::Pelvis, [0.0, 1.0, 0.0]),
            (J::Spine, [0.0, 1.1, 0.0]),
            (J::Spine1, [0.0, 1.25, 0.0]),
            (J::Spine2, [0.0, 1.35, 0.0]),
            (J::Neck, [0.0, 1.5, 0.0]),
            (J::Head, [0.0, 1.6, 0.0]),
            (J::RClavicle, [0.03, 1.45, 0.0]),
            (J::RUpperArm, [0.18, 1.45, 0.0]),
            (J::RForearm, [0.25, 1.2, 0.0]),
            (J::RHand, [0.3, 0.95, 0.0]),
            (J::RFinger2, [0.32, 0.88, 0.0]),
            (J::RFinger0, [0.31, 0.92, -0.04]),
            (J::RThigh, [0.1, 0.95, 0.0]),
            (J::RCalf, [0.1, 0.5, 0.0]),
            (J::RFoot, [0.1, 0.08, 0.05]),
            (J::RToe0, [0.1, 0.02, -0.1]),
        ];
        let mirror = |j: J| match j {
            J::RClavicle => Some(J::LClavicle),
            J::RUpperArm => Some(J::LUpperArm),
            J::RForearm => Some(J::LForearm),
            J::RHand => Some(J::LHand),
            J::RFinger2 => Some(J::LFinger2),
            J::RFinger0 => Some(J::LFinger0),
            J::RThigh => Some(J::LThigh),
            J::RCalf => Some(J::LCalf),
            J::RFoot => Some(J::LFoot),
            J::RToe0 => Some(J::LToe0),
            _ => None,
        };
        for (j, [x, y, z]) in joints {
            p[j as usize] = Vec3::new(x, y, z);
            if let Some(l) = mirror(j) {
                p[l as usize] = Vec3::new(-x, y, z);
            }
        }
        let rest = Pose {
            p,
            head_up: Vec3::Y,
            head_fwd: Vec3::NEG_Z,
            weapon_axes: [None; 2],
        };
        let map = RigMap::build(
            &RigInput {
                parents: &parents,
                binds: &binds,
                head: 5,
                hands: [9, 16],
                hand_offsets: [Vec3::ZERO; 2],
                stand_local: &|b| Transform::from_translation(binds[b]),
                elbows: [None, None],
                thumbs: [Some(12), Some(19)],
            },
            &rest,
        )
        .expect("maps");
        let mut seen: Vec<usize> = map.driven.iter().map(|&(b, _)| b).collect();
        seen.sort_unstable();
        assert!(seen.windows(2).all(|w| w[0] != w[1]), "a bone driven twice");
        assert!(!seen.contains(&2), "the shared hip root is driven");

        // A kick forward with the right leg, the left knee bent, the right arm raised.
        let mut pose = Pose { ..rest };
        pose.p[J::RCalf as usize] = Vec3::new(0.15, 0.7, -0.4);
        pose.p[J::RFoot as usize] = Vec3::new(0.15, 0.6, -0.8);
        pose.p[J::RToe0 as usize] = Vec3::new(0.15, 0.65, -0.95);
        pose.p[J::LCalf as usize] = Vec3::new(-0.1, 0.55, -0.2);
        pose.p[J::RForearm as usize] = Vec3::new(0.3, 1.65, -0.1);
        pose.p[J::RHand as usize] = Vec3::new(0.35, 1.9, -0.2);
        pose.p[J::RFinger2 as usize] = Vec3::new(0.36, 1.97, -0.22);
        pose.p[J::RFinger0 as usize] = Vec3::new(0.31, 1.93, -0.25);
        let mut work: Vec<Transform> = binds
            .iter()
            .map(|&t| Transform::from_translation(t))
            .collect();
        let twist = Quat::from_rotation_y(0.2);
        for &b in &helpers {
            work[b].rotation = twist;
        }
        map.pose(&pose, &parents, &binds, &mut work, true);
        for b in helpers {
            assert_eq!(work[b].rotation, twist, "overwrote stock deform helper {b}");
        }
        let mut hands = 0;
        for &(bone, drive) in &map.driven {
            let model = ancestors(&parents, bone)
                .iter()
                .rev()
                .fold(Quat::IDENTITY, |acc, &a| acc * work[a].rotation);
            match drive {
                Drive::Aim { from, to, bind_dir } => {
                    let got = (model * bind_dir).normalize();
                    let want = (to.at(&pose) - from.at(&pose)).normalize();
                    assert!(
                        got.angle_between(want) < 1e-3,
                        "bone {bone} points {got}, not {want}"
                    );
                }
                // The wrist's whole frame is the ER hand's, palm and all.
                Drive::Hand {
                    hand,
                    finger,
                    thumb,
                    bind_up,
                    bind_side,
                } => {
                    hands += 1;
                    let at = pose.at(hand);
                    let want = basis(pose.at(finger) - at, pose.at(thumb) - at).unwrap()
                        * basis(bind_up, bind_side).unwrap().inverse();
                    assert!(model.angle_between(want) < 1e-3, "hand {bone} is off");
                }
                _ => {}
            }
        }
        assert_eq!(hands, 2, "both wrists take the hand frame");
    }
}
