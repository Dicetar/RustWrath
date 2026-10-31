# Astra — 12340 particle data contract

Claim before source editing: `crates/benilla-formats/src/wrath_particles.rs`, the export in
`src/lib.rs`, and `examples/wrath_particles.rs`. Baseline `80c5139a`; OC's marker/camera,
UI/gameplay files and owner's dirty instruction documents are preserved.

Reference: `D:/Projects/wowdecomp-reference`, pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`, `Engine/Source/Model2/M2Data.h` M2Particle,
M2Track/M2PartTrack and `M2Shared.cpp` array validation; M2Model.cpp emitter setup.

Scope: bounded v264 record/track decoder with synthetic and selected-profile asset checks,
plus runtime integration for compatible effects. Retain refusal for effects whose necessary
rendering semantics are not consumed. No automated combat/server/install writes.

Integration scope added: `benilla-formats/src/particles.rs` version dispatch and optional
Wrath age-curve storage; `benilla-world/src/particles/{quads,model,sim,depthdump}.rs` route their
age sampling through the version-aware method, plus the shared world test fixture in lib.rs.

The decoder retains every 0x1dc-record field, all eleven nested tracks, normalized-age color,
alpha, XY-size and head/tail-cell curves. A model-wide expansion budget limits malicious aliased
descriptors as well as validating each span. It does not depend on the old flat-global-track
assumption: global clocks still use nested arrays as M2Data.h defines.

Runtime adapter enables only representable planar/spherical quad effects. Explicit guards
remain for nonzero priority planes, geometry/recursive emitters, lifespan/rate variations,
spin variation, wind/tumble, anisotropic/variable scale and unsupported interpolation. Keep
those gaps open rather than dropping their fields. Existing vanilla sampling remains intact.

Selected-profile data inspected read-only: ElwynnCampfire (2 records, nonzero lifespan/rate
variation), Fireball_missile_low (4, includes priority), GeneralTorch01 (2, compatible runtime
projection at rate 20 / lifespan 1). Thus broad particle parity is not established.

Verification: `cargo test --locked --offline -p benilla-formats --lib particles::`
reports 13 passed; new deterministic cases cover record strides, forged/truncated counts,
independent age curves, two-dimensional scale retention, byte-domain RGB normalization,
sequence-slot/global nesting and aliased-array expansion limits. The old install-dependent
cases may skip internally when their data is absent; selected-profile evidence is the direct
CLI runs on the three explicitly extracted models above. No game data is committed.

`cargo build --locked --offline -p benilla` passed with incremental compilation disabled.
`astra-graphics-run.ps1` includes the existing torch emission dump and particle census,
an explicit start signal, a 70-second wall bound, 5-GiB memory bound and no combat probe.

Runtime result (2026-10-31): build passed, then bounded run
`connected-run-20261031-121149-198`, screenshot `astra-graphics-20261031-121148.png`.
Binary SHA-256 `FC128D541D4D4A00B872CAAF12FA1AAF06276A47D23B1627BF421D1CC75BC7F1`,
source base `80c5139a` plus this particle patch. Profile wm-hd/shadow-data. Camp torches now
show visible flames where the earlier capture showed bare poles. Emission dump reports
rate 20, live 20 and lifespan 1 for nearby emitter components; census live_total=200,
drawn_beyond_wall=0. Peak private memory 2.22 GiB. Probe exited code 0 at its 40-second limit;
no combat actions. This is basic-quad effect acceptance, not full Wrath particle acceptance
or a sustained leak test. Specific guards are visible for smoke, campfire and other effects.

OC began editing `benilla-app/src/quest_markers/mod.rs` during verification; it is not included
in this patch or the evidence claim. Preserve that work. No owned client remains running.
