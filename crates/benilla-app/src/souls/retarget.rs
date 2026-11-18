//! The Elden Ring motion on the local player's own rig: the kit's actions play the ER clip each
//! row was read from (`source`), and, on their own switch, idle, guard, walk, run, sprint, crouch
//! and airborne play the ER locomotion clips.
//!
//! **The clips** are the reference's bake, `player_anims.bin` (`ERAN`: model-space joint positions
//! per frame at 30 fps, plus the head's axes; root motion already removed). It is read from
//! `benilla-config/combat/player_anims.bin`, next to `souls.toml` and `player.kit`, on a worker
//! thread the first time `F10` turns the mode on with a retarget switch set, and again when the
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
//! - each arm (clavicle, if the M2 chain has one, to the hand) and leg (thigh to toe) bone is
//!   turned from its parent's frame along the shortest arc that points it where the ER limb
//!   points, so the roll comes down from the torso;
//! - the pelvis also takes the ER pelvis's offset from its stand height, scaled to the model, so
//!   a crouch or a roll lowers the body.
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

use benilla_world::rig_anim::RigPose;
use bevy::prelude::*;

use super::{current_kit, running_action, souls_on, SoulsConfig, SoulsState, YARDS_PER_METRE};
use crate::combat_kit::PlayerKit;
use crate::combat_mode::CombatModeView;
use crate::combat_playback::CombatPlaybackState;
use crate::combat_profiles::CombatProfiles;
use crate::creature_anim::{AnimDriver, BodyTwist};
use crate::entities::BoneAttach;
use crate::net::SelfPlayer;

/// The bake's rate.
const CLIP_FPS: f32 = 30.0;
/// Seconds back to the stock pose once nothing plays.
const FADE_OUT_SECS: f32 = 0.2;
/// The shortest blend into a clip, seconds.
const MIN_BLEND_SECS: f32 = 0.05;
/// The file's name, next to `souls.toml`.
const CLIP_FILE: &str = "player_anims.bin";

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
}

const JOINTS: usize = 24;

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
/// 37-40, StealthWalk 119, StealthStand 120 and Sprint 143. Anything else (a cast, an emote, a
/// swim, a mount) keeps the stock animation.
fn stands_in_for(base: u16) -> bool {
    matches!(base, 0 | 4 | 5 | 11 | 12 | 13 | 25..=29 | 37..=40 | 119 | 120 | 143)
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
    clips: std::collections::HashMap<String, ErClip>,
}

/// One sampled ER pose in the M2 model frame (`+X` right, `+Y` up, `−Z` forward).
#[derive(Clone, Copy, Debug)]
struct Pose {
    p: [Vec3; JOINTS],
    head_up: Vec3,
    head_fwd: Vec3,
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
        let head_axes = oriented_names
            .iter()
            .position(|j| j == "Head")
            .ok_or("no Head axes")?
            * 9
            + joints * 3;
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
        }
    }

    fn named(&self, id: u32) -> Option<&ErClip> {
        self.clips.get(&format!("a000_{id:06}"))
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
}

/// The roles of one rig's bones.
#[derive(Debug)]
struct RigMap {
    /// Driven bones, parents first.
    driven: Vec<(usize, Drive)>,
    pelvis: usize,
    /// Model units per ER metre.
    scale: f32,
    /// The ER pelvis at rest.
    rest_pelvis: Vec3,
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

/// What the mapping needs from a rig.
struct RigInput<'a> {
    parents: &'a [i16],
    binds: &'a [Vec3],
    head: usize,
    /// The hand attachment bones, `[right, left]`.
    hands: [usize; 2],
    /// The elbow attachment points in the model frame, `[right, left]`, when the model has them.
    elbows: [Option<Vec3>; 2],
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

        // The feet: the lowest bone on each side, off the head chain.
        let side_width = height * 0.02;
        let lowest = |right: bool| {
            (0..n)
                .filter(|&b| !on_head_chain(b))
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
                    to: mapped[j],
                    bind_up: points[j] - points[k],
                    side_mix: (frac / shoulder_frac).clamp(0.0, 1.0),
                },
            ));
        }

        // The arms: the hand attachment's bone, or the bone above when it is a bare attach point.
        if piv[rig.hands[0]].x <= piv[rig.hands[1]].x {
            return Err("the right hand is not on the right".into());
        }
        for ((&hand, elbow), arm) in rig.hands.iter().zip(rig.elbows).zip(ARMS) {
            let wrist = if children(hand).next().is_some() {
                hand
            } else {
                parent_of(rig.parents, hand).ok_or("a hand with no parent")?
            };
            let mut chain: Vec<usize> = ancestors(rig.parents, wrist)
                .into_iter()
                .take_while(|&b| !on_head_chain(b))
                .collect();
            chain.reverse();
            if chain.len() < 2 {
                return Err("an arm with fewer than two bones".into());
            }
            let mut points: Vec<Vec3> = chain.iter().map(|&b| piv[b]).collect();
            let wrist_i = points.len() - 1;
            // The fingers' base, or the forearm carried on a little.
            let fingers: Vec<Vec3> = children(wrist)
                .map(|c| piv[c])
                .filter(|p| p.distance(piv[wrist]) > eps)
                .collect();
            let finger = if fingers.is_empty() {
                piv[wrist] + (piv[wrist] - points[wrist_i - 1]) * 0.25
            } else {
                fingers.iter().copied().sum::<Vec3>() / fingers.len() as f32
            };
            points.push(finger);
            let finger_i = points.len() - 1;
            // A clavicle or not: whichever ER length the chain's matches.
            let len = |a: J, b: J| rest.at(a).distance(rest.at(b)) * scale;
            let upper_fore = len(arm[1], arm[2]) + len(arm[2], arm[3]);
            let total = arc(&points)[wrist_i];
            let with_clavicle =
                (total - upper_fore - len(arm[0], arm[1])).abs() <= (total - upper_fore).abs();
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

        // The legs, hip to toe; the toe follows its foot.
        for (&toe, leg) in toes.iter().zip(LEGS) {
            let mut chain: Vec<usize> = ancestors(rig.parents, toe)
                .into_iter()
                .take_while(|&b| b != pelvis)
                .collect();
            chain.reverse();
            if chain.len() < 2 {
                return Err("a leg with fewer than two bones".into());
            }
            let points: Vec<Vec3> = chain.iter().map(|&b| piv[b]).collect();
            let last = points.len() - 1;
            // The knee and ankle by height: a bind leg hangs straight.
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
            pelvis,
            scale,
            rest_pelvis,
        })
    }

    /// The retargeted locals of the driven bones over `stock` (every bone's local, stock pose),
    /// written into `work`: rotations of the driven bones and the pelvis translation.
    fn pose(&self, pose: &Pose, parents: &[i16], binds: &[Vec3], work: &mut [Transform]) {
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
            };
            let Some(desired) = desired else {
                continue;
            };
            let local = (parent.inverse() * desired).normalize();
            if local.is_finite() {
                work[bone].rotation = local;
            }
        }
        let offset = (pose.at(J::Pelvis) - self.rest_pelvis) * self.scale;
        let parent = model(work, parent_of(parents, self.pelvis));
        let translation = binds[self.pelvis] + parent.inverse() * offset;
        if translation.is_finite() {
            work[self.pelvis].translation = translation;
        }
    }

    /// The bones the pass writes.
    fn bones(&self) -> impl Iterator<Item = usize> + '_ {
        self.driven
            .iter()
            .map(|&(b, _)| b)
            .chain(std::iter::once(self.pelvis))
    }
}

/// What plays: a kit action's run, or a locomotion clip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClipKey {
    Action(super::Instance),
    Loco(u32),
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
    /// The locomotion clock, seconds of clip at rate 1.
    loco_clock: f32,
    last_pos: Option<Vec3>,
    /// The retarget switches were on last frame.
    was_on: bool,
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

/// The locomotion clip for the body's state, and its rate.
fn loco_clip(souls: &SoulsState, forward: f32, right: f32) -> (u32, f32) {
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
    if souls.was_airborne {
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
            .map(|&(bone, _)| usize::from(bone))
            .ok_or("the model has no hand attachments")
    };
    let piv = pivots(&rig.parents, rig.binds());
    let elbow = |side: usize| {
        attach
            .points
            .get(&ELBOW_ATTACH[side])
            .and_then(|&(bone, offset)| piv.get(usize::from(bone)).map(|p| *p + offset))
    };
    RigMap::build(
        &RigInput {
            parents: &rig.parents,
            binds: rig.binds(),
            head,
            hands: [hand(0)?, hand(1)?],
            elbows: [elbow(0), elbow(1)],
        },
        rest,
    )
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
    mut retarget: ResMut<Retarget>,
    mut me: Query<
        (
            Entity,
            &mut RigPose,
            &GlobalTransform,
            Option<&BodyTwist>,
            Option<&BoneAttach>,
            Option<&AnimDriver>,
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
    let Ok((entity, mut rig, global, twist, attach, driver)) = me.single_mut() else {
        r.written = None;
        r.playing = None;
        return;
    };
    let now = time.elapsed_secs();
    let dt = time.delta_secs();

    // The body's velocity in its own frame, metres per second.
    let (_, facing, pos) = global.to_scale_rotation_translation();
    let velocity = match r.last_pos {
        Some(last) if dt > 0.0 => (pos - last) / dt / YARDS_PER_METRE,
        _ => Vec3::ZERO,
    };
    r.last_pos = Some(pos);
    let local_v = facing.inverse() * velocity;
    let (forward, right) = (-local_v.z, local_v.x);

    // The map, rebuilt for a new rig.
    let bones = rig.locals.len();
    if let (Some(rest), true) = (
        r.rest.as_ref(),
        r.map
            .as_ref()
            .is_none_or(|(e, n, _)| *e != entity || *n != bones),
    ) {
        let built = map_rig(&rig, twist, attach, rest);
        if let Err(e) = &built {
            warn!("souls retarget: the player's model cannot be mapped, stock clips play: {e}");
        }
        r.map = Some((entity, bones, built));
        r.written = None;
        r.from = None;
        r.playing = None;
    }

    // What plays.
    let mut target: Option<(ClipKey, f32, bool, f32)> = None;
    if on {
        if let Some(clips) = r.clips.as_ref() {
            let running = current_kit(&kit, &profiles)
                .and_then(|kit| running_action(kit, &playback).map(|run| (kit, run)));
            // A running action plays its own clip, or the stock one when it has none here (or the
            // switch is off); locomotion never covers it.
            let action = running.filter(|_| features.retarget).and_then(
                |(kit, (instance, action, frame))| {
                    let clip = clips.clips.get(&action.source)?;
                    let at =
                        frame.0 as f32 / 1000.0 * CLIP_FPS / f32::from(kit.frame_rate_hz.max(1));
                    Some((ClipKey::Action(instance), at, false, clip.blend))
                },
            );
            target = action.or_else(|| {
                if running.is_some() {
                    return None;
                }
                let stock_loco = driver.is_none_or(|d| {
                    let (base, overlay) = d.playing();
                    overlay.is_none() && base.is_some_and(stands_in_for)
                });
                if !features.retarget_locomotion || !stock_loco {
                    return None;
                }
                let (id, rate) = loco_clip(&souls, forward, right);
                let clip = clips.named(id)?;
                r.loco_clock += dt * rate;
                Some((ClipKey::Loco(id), r.loco_clock * CLIP_FPS, true, clip.blend))
            });
        }
    }
    let Some((_, _, Ok(map))) = r.map.as_ref() else {
        r.written = None;
        return;
    };
    if target.is_none() && r.written.is_none() {
        r.playing = None;
        return;
    }

    // The stock pose under the pass. A bone still holding last frame's write (ours, or the strafe
    // twist's on top of it) was not re-keyed, so its stock value stands.
    if r.stock.len() != bones {
        r.stock = rig.locals.clone();
    }
    for b in map.bones() {
        let cur = rig.locals[b];
        let wrote = r.written.as_ref().is_some_and(|w| {
            w[b] == cur
                || (w[b].translation == cur.translation
                    && twist.and_then(|t| t.last_written(b)) == Some(cur.rotation))
        });
        if !wrote {
            r.stock[b] = cur;
        }
    }

    // A clip change starts a blend from what was last written.
    let key = target.map(|(k, ..)| k);
    if key != r.playing {
        r.from = r.written.clone();
        r.fade_at = now;
        r.fade_len = match target {
            Some((.., blend)) => (blend / CLIP_FPS).max(MIN_BLEND_SECS),
            None => FADE_OUT_SECS,
        };
        r.playing = key;
    }
    let k = ((now - r.fade_at) / r.fade_len.max(f32::EPSILON)).clamp(0.0, 1.0);

    let mut work: Vec<Transform> = rig.locals.clone();
    for b in map.bones() {
        work[b] = r.stock[b];
    }
    if let (Some((key, frame, looped, _)), Some(clips)) = (target, r.clips.as_ref()) {
        let clip = match key {
            ClipKey::Action(_) => current_kit(&kit, &profiles).and_then(|kit| {
                let (_, action, _) = running_action(kit, &playback)?;
                clips.clips.get(&action.source)
            }),
            ClipKey::Loco(id) => clips.named(id),
        };
        if let Some(clip) = clip {
            let pose = clips.sample(clip, frame, looped);
            map.pose(&pose, &rig.parents, rig.binds(), &mut work);
        }
    }

    // Blend and write.
    let mut written = r.written.take().unwrap_or_else(|| rig.locals.clone());
    let mut dirty = false;
    for b in map.bones() {
        let base = r.from.as_ref().map_or(r.stock[b], |from| from[b]);
        let out = Transform {
            translation: base.translation.lerp(work[b].translation, k),
            rotation: base.rotation.slerp(work[b].rotation, k).normalize(),
            scale: work[b].scale,
        };
        if !(out.translation.is_finite() && out.rotation.is_finite()) {
            continue;
        }
        if rig.locals[b] != out {
            rig.locals[b] = out;
            dirty = true;
        }
        written[b] = out;
    }
    if dirty {
        rig.pose_dirty = true;
    }
    if target.is_none() && k >= 1.0 {
        // Faded out: the stock pose is back, and the pass stops writing.
        r.written = None;
        r.from = None;
        r.playing = None;
    } else {
        r.written = Some(written);
    }
}

/// Register [`drive_retarget`] in the pose post-pass, before the strafe twist composes onto the
/// spine and head.
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Retarget>().add_systems(
        PostUpdate,
        drive_retarget
            .in_set(benilla_world::rig_anim::PosePost)
            .before(crate::creature_anim::apply_body_twist),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Pelvis x 1.5 in the bake (+X left) is −1.5 in the model (+X right); z flips too.
        assert!((pose.at(J::Pelvis) - Vec3::new(-1.5, 0.5, -0.5)).length() < 1e-6);
        assert!(ErClips::parse(b"ERAX").is_err());
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
        map.pose(&rest, &parents, &binds, &mut work);
        assert!(work[1].rotation.angle_between(Quat::IDENTITY) < 1e-4);
        // The rest pelvis (here the origin) is a metre under the map's: the body drops two units.
        assert!((work[0].translation - Vec3::new(0.0, 0.0, 0.0)).length() < 1e-5);
        assert!(basis(Vec3::Y, Vec3::X).is_some_and(|q| q.angle_between(Quat::IDENTITY) < 1e-6));
    }
}
