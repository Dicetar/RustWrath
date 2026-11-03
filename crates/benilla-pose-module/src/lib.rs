//! Reference module: normalize finite, nonzero rotation quaternions. No game state is owned here.
use benilla_hotpatch_abi::{BoneLocal, Header, ModuleApi, PoseFrame};

unsafe extern "C" fn process(
    frame: *const PoseFrame,
    output: *mut BoneLocal,
    capacity: u32,
) -> i32 {
    if frame.is_null() || output.is_null() {
        return 1;
    }
    // SAFETY: the host supplies a live frame and disjoint, aligned buffers for this call only.
    let frame = unsafe { &*frame };
    if frame.bone_count > capacity || frame.bones.is_null() {
        return 2;
    }
    for index in 0..frame.bone_count as usize {
        let mut bone = unsafe { *frame.bones.add(index) };
        if !cfg!(feature = "passthrough") {
            if !bone.valid() {
                return 3;
            }
            let norm = bone.rotation.iter().map(|n| n * n).sum::<f32>().sqrt();
            for n in &mut bone.rotation {
                *n /= norm;
            }
        }
        // Explicit presentation-only live acceptance lever, not a server position change.
        if cfg!(feature = "lift-demo") && bone.parent < 0 {
            bone.translation[1] += 0.5;
        }
        unsafe {
            output.add(index).write(bone);
        }
    }
    0
}

static API: ModuleApi = ModuleApi {
    header: Header::current(),
    process: Some(process),
};

/// # Safety
/// The returned descriptor lives for the library's lifetime; callers must pin the library.
#[no_mangle]
pub unsafe extern "C" fn project_b_pose_module_v1() -> *const ModuleApi {
    &API
}
