//! Borrowed, host-allocated hotpatch module buffers across presentation and gameplay seams.
//! A module owns no world, callbacks or threads. All inputs and outputs use `#[repr(C)]` value
//! slices owned by the host and swapped only at frame boundaries after validation.

pub const MAGIC: u64 = 0x5042_504F_5345_3031;
pub const ABI_VERSION: u32 = 1;
pub const ENTRY: &[u8] = b"project_b_pose_module_v1\0";

pub const MOVEMENT_MAGIC: u64 = 0x5042_4D4F_5645_3031;
pub const MOVEMENT_ENTRY: &[u8] = b"project_b_movement_module_v1\0";

pub const COMBAT_MAGIC: u64 = 0x5042_434D_4254_3031;
pub const COMBAT_ENTRY: &[u8] = b"project_b_combat_module_v1\0";

pub const SPACE_MAGIC: u64 = 0x5042_5350_4143_3031;
pub const SPACE_ENTRY: &[u8] = b"project_b_space_module_v1\0";

pub const STORY_MAGIC: u64 = 0x5042_5354_5259_3031;
pub const STORY_ENTRY: &[u8] = b"project_b_story_module_v1\0";

/// Identifies which hot-pluggable seam a module artifact implements.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleKind {
    Pose = 1,
    Movement = 2,
    Combat = 3,
    Space = 4,
    Story = 5,
}

impl ModuleKind {
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Pose => "pose-",
            Self::Movement => "movement-",
            Self::Combat => "combat-",
            Self::Space => "space-",
            Self::Story => "story-",
        }
    }

    pub const fn entry_symbol(self) -> &'static [u8] {
        match self {
            Self::Pose => ENTRY,
            Self::Movement => MOVEMENT_ENTRY,
            Self::Combat => COMBAT_ENTRY,
            Self::Space => SPACE_ENTRY,
            Self::Story => STORY_ENTRY,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneLocal {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub parent: i32,
}

impl BoneLocal {
    pub fn valid(&self) -> bool {
        self.translation
            .iter()
            .chain(&self.rotation)
            .chain(&self.scale)
            .all(|n| n.is_finite())
            && self.rotation.iter().map(|n| n * n).sum::<f32>().is_finite()
            && self.rotation.iter().map(|n| n * n).sum::<f32>() > 1e-12
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RigSpan {
    /// Process-local entity identity, not a persistent or server identifier.
    pub entity: u64,
    pub first: u32,
    pub count: u32,
}

#[repr(C)]
pub struct PoseFrame {
    pub bones: *const BoneLocal,
    pub bone_count: u32,
    pub rigs: *const RigSpan,
    pub rig_count: u32,
    pub time_seconds: f64,
}

/// Zero means success. Output has exactly `bone_count` slots, owned by the caller.
/// No panic, pointer retention, callbacks, allocation ownership or worker thread may cross here.
pub type ProcessPose = unsafe extern "C" fn(*const PoseFrame, *mut BoneLocal, u32) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub magic: u64,
    pub abi_version: u32,
    pub api_size: u32,
    pub bone_size: u32,
    pub rig_size: u32,
    pub frame_size: u32,
    pub pointer_size: u32,
}

impl Header {
    pub const fn current() -> Self {
        Self {
            magic: MAGIC,
            abi_version: ABI_VERSION,
            api_size: std::mem::size_of::<ModuleApi>() as u32,
            bone_size: std::mem::size_of::<BoneLocal>() as u32,
            rig_size: std::mem::size_of::<RigSpan>() as u32,
            frame_size: std::mem::size_of::<PoseFrame>() as u32,
            pointer_size: std::mem::size_of::<usize>() as u32,
        }
    }

    pub fn compatible(&self) -> bool {
        let expected = Self::current();
        self.magic == expected.magic
            && self.abi_version == expected.abi_version
            && self.api_size == expected.api_size
            && self.bone_size == expected.bone_size
            && self.rig_size == expected.rig_size
            && self.frame_size == expected.frame_size
            && self.pointer_size == expected.pointer_size
    }
}

#[repr(C)]
pub struct ModuleApi {
    pub header: Header,
    pub process: Option<ProcessPose>,
}

pub type EntryPoint = unsafe extern "C" fn() -> *const ModuleApi;

// ============================================================================
// Seam 2: Hot-Pluggable Movement Module (`MovementModuleV1`)
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementFrame {
    pub dt_seconds: f32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub facing: f32,
    pub pitch: f32,
    pub camera_yaw: f32,
    /// Bitmask of directional/jump/dodge/sprint intents for this frame.
    pub input_buttons: u32,
    /// The baseline's current move flags. Input may carry baseline/server-owned state (root,
    /// falling, swimming, transport); a module must not echo non-delegated bits in
    /// [`MovementOutput::move_flags`], which is checked against the host's `permitted_flags`.
    pub move_flags: u32,
    pub run_speed: f32,
    pub walk_speed: f32,
    pub swim_speed: f32,
    pub ground_height: f32,
    pub ground_normal: [f32; 3],
    /// `NEG_INFINITY` means no liquid is present; every other value is a height.
    pub liquid_height: f32,
    pub stamina_norm: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementOutput {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub facing: f32,
    pub pitch: f32,
    pub move_flags: u32,
    pub anim_override: u16,
    pub emit_move_opcode: u16,
}

impl MovementOutput {
    pub fn valid(&self) -> bool {
        self.position
            .iter()
            .chain(&self.velocity)
            .chain([&self.facing, &self.pitch])
            .all(|v| v.is_finite())
    }
}

pub type ProcessMovement =
    unsafe extern "C" fn(*const MovementFrame, *mut MovementOutput) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MovementModuleV1 {
    pub magic: u64,
    pub abi_version: u32,
    pub frame_size: u32,
    pub output_size: u32,
    pub process: Option<ProcessMovement>,
}

pub type MovementEntryPoint = unsafe extern "C" fn() -> *const MovementModuleV1;

// ============================================================================
// Seam 3: Hot-Pluggable Combat Module (`CombatModuleV1` — Souls / Elden Ring)
// ============================================================================

/// Combat state phase inspired by `Funny-Bones/ELDEN-RING-Combat-Rewrite`:
/// `0 = Neutral`, `1 = Windup`, `2 = ActiveHitbox`, `3 = Recovery`, `4 = DodgeIFrame`,
/// `5 = Guard`, `6 = ParryWindow`, `7 = Staggered`, `8 = Riposte`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatEntitySnapshot {
    pub guid: u64,
    pub position: [f32; 3],
    pub facing: f32,
    pub health: u32,
    pub max_health: u32,
    pub combat_reach: f32,
    pub poise: f32,
    pub max_poise: f32,
    pub flags: u32,
}

#[repr(C)]
pub struct CombatFrame {
    pub dt_seconds: f32,
    pub player_guid: u64,
    pub lock_on_target: u64,
    /// Bitmask: light attack, heavy attack, charged R2, dodge/roll, guard, parry, weapon art, lock-on toggle.
    pub action_buttons: u32,
    pub current_phase: u32,
    pub phase_elapsed_sec: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub poise: f32,
    pub max_poise: f32,
    pub weapon_reach: f32,
    pub weapon_arc_rad: f32,
    pub entities: *const CombatEntitySnapshot,
    pub entity_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatOutput {
    pub next_phase: u32,
    pub phase_elapsed_sec: f32,
    pub stamina: f32,
    pub poise: f32,
    pub lock_on_target: u64,
    pub primary_hit_target: u64,
    pub trigger_spell_id: u32,
    pub anim_kit_override: u16,
    pub root_motion_speed: f32,
    pub iframe_active: u8,
    pub hyperarmor_active: u8,
}

impl CombatOutput {
    pub fn valid(&self) -> bool {
        self.phase_elapsed_sec.is_finite()
            && self.stamina.is_finite()
            && self.poise.is_finite()
            && self.root_motion_speed.is_finite()
    }
}

pub type ProcessCombat = unsafe extern "C" fn(*const CombatFrame, *mut CombatOutput) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CombatModuleV1 {
    pub magic: u64,
    pub abi_version: u32,
    pub frame_size: u32,
    pub output_size: u32,
    pub process: Option<ProcessCombat>,
}

pub type CombatEntryPoint = unsafe extern "C" fn() -> *const CombatModuleV1;

// ============================================================================
// Seam 4: Hot-Pluggable Procedural Space & Spawner Module (`SpaceModuleV1`)
// ============================================================================

/// Kind of dynamic spawn generated by a procedural space module:
/// `1 = Creature/Mob`, `2 = Decoration/M2 Prop`, `3 = Interactive Item/Chest`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralSpawn {
    pub spawn_uid: u64,
    pub spawn_kind: u32,
    pub template_entry: u32,
    pub display_id: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub scale: f32,
    pub faction: u32,
    pub level: u32,
}

impl ProceduralSpawn {
    pub fn valid(&self) -> bool {
        self.position.iter().all(|v| v.is_finite())
            && self.orientation.is_finite()
            && self.scale.is_finite()
            && self.scale > 0.0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceRequest {
    pub world_seed: u64,
    pub map_id: u32,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub chunk_origin_ws: [f32; 3],
    pub player_level: u32,
}

/// Zero means success. Writes up to `spawn_capacity` spawns and stores the written count in
/// `*out_spawn_count`. Optional `out_heightfield_256` (16x16 f32 heights) may be null when the
/// caller only requests dynamic mob/prop/item spawns.
pub type GenerateSpaceChunk = unsafe extern "C" fn(
    *const SpaceRequest,
    *mut f32,
    *mut ProceduralSpawn,
    u32,
    *mut u32,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SpaceModuleV1 {
    pub magic: u64,
    pub abi_version: u32,
    pub request_size: u32,
    pub spawn_size: u32,
    pub generate_chunk: Option<GenerateSpaceChunk>,
}

pub type SpaceEntryPoint = unsafe extern "C" fn() -> *const SpaceModuleV1;

// ============================================================================
// Seam 5: Hot-Pluggable Story & Quest Module (`StoryModuleV1`)
// ============================================================================

/// Event kind passed into a story module:
/// `1 = NpcInteract`, `2 = CreatureKilled`, `3 = ItemLooted`, `4 = AreaEntered`, `5 = QuestAccept`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoryEvent {
    pub event_kind: u32,
    pub subject_entry: u32,
    pub subject_guid: u64,
    pub quest_id: u32,
    pub count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoryStepDirective {
    pub quest_id: u32,
    /// `0 = Unchanged`, `1 = OfferQuest`, `2 = AdvanceObjective`, `3 = CompleteQuest`, `4 = SpawnEventEncounter`.
    pub directive_kind: u32,
    pub objective_index: u32,
    pub objective_progress: u32,
    pub objective_required: u32,
    pub follow_up_quest_id: u32,
    pub spawn_encounter_entry: u32,
}

pub type EvaluateStoryEvent =
    unsafe extern "C" fn(*const StoryEvent, *mut StoryStepDirective) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StoryModuleV1 {
    pub magic: u64,
    pub abi_version: u32,
    pub event_size: u32,
    pub directive_size: u32,
    pub evaluate: Option<EvaluateStoryEvent>,
}

pub type StoryEntryPoint = unsafe extern "C" fn() -> *const StoryModuleV1;
