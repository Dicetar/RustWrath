# Hotpatch host and rig ownership

Superseding owner update: Astra now owns graphics, rig and scheduling integration. OpenCode
continues playable combat; follow `docs/agents/active-ownership.md` and the newer
`2026-10-31-astra-lead-oc-combat.md` handoff. The historical scheduling reservation below no
longer applies. PoseModulePass and the restoration adapter are implemented; their focused
roundtrip check passes. External animation decoding is the current graphics blocker, detailed
in `2026-10-31-astra-external-animation.md`. No in-game native reload claim yet.

Owner requested native module and asset hotpatching, and explicitly approved the one-time
bootstrap host rebuild/restart. Astra owns new benilla-hotpatch-abi, benilla-hotpatch and
benilla-pose-module crates; benilla-app/hotpatch.rs; their Cargo features and app registration;
publisher tool and ADR-0002. These use the existing public scheduling seam after
AnimationSystems and before PosePost. No rig evaluator, palette, transform, M2 or profile
fallback implementation is edited by Astra. OC's dirty rig_anim/pose.rs remains his.

Source finding for OC's current pending repair: RigPose::compose sanitizes a local copy of
self.locals[i] and uses it for self.model, but does not write it back into self.locals[i].
compose.rs::rig_worlds later multiplies rig.locals[i] directly. A malformed local can therefore
re-enter the world palette despite the model-space guard. Please inspect both passes when
finishing the repair; this is source evidence, not a new rendered result or a claimed fix.

The reference module normalizes finite nonzero quaternion rotations. It is a real reloadable
postprocessor example, not a proven repair for the corrupt talking animation. It rejects
non-finite input rather than inventing missing bind-pose data. The host preserves the existing
pose or previous module on rejection. Full evaluator/import hotpatching and content invalidation
are later seams described in ADR-0002; native DLL load success alone is not game acceptance.

Do not launch a competing client/account session during bootstrap verification. Coordinate the
bootstrap build through the existing shared process checks. Current source evidence and live
claims must remain separate. Preserve decomp reference pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a.

Owner coordination update: OC retains compose.rs and rig_anim.rs too. The initially proposed
before-PosePost adapter was removed before deployment. It is not safe for arbitrary pose
patches: postprocessors save locals for later restoration, and unkeyed channels keep previous
values. Patching before those edits can retain or accumulate a module's output.

Required OC-owned seam: export a public SystemSet after every PosePost writer but before
compose_rig_models, and order compose_rig_models after that set. Example name: PoseModulePass.
No DLL/host dependency is required in benilla-world. Astra will own the app adapter in that
set, preserve the post-edit input locals, and restore those locals after the world/palette
passes so patch output does not become the next frame's baseline. Verify all downstream
readers before choosing that restoration point. Preserve animation clocks and event cursors.

Until that seam exists and the restoration ordering is verified, the client plugin stages
compatible DLLs but explicitly warns that native pose dispatch is unavailable. Standalone
loader swap/rejection is proven; in-game pose hotpatching is not. Do not promise the bootstrap
deployment is complete or restart the owner's session for this staging-only build.
