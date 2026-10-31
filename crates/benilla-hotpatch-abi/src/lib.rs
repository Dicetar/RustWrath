//! Borrowed, host-allocated presentation buffers. A module owns no world, callbacks or threads.

pub const MAGIC: u64 = 0x5042_504F_5345_3031;
pub const ABI_VERSION: u32 = 1;
pub const ENTRY: &[u8] = b"project_b_pose_module_v1\0";

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
