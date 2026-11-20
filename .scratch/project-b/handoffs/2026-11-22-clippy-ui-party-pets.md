# Player-clippy debt: crates/benilla-app/src/ui_party/pets.rs

Status: ready to merge
Owner: DeepSeek V4.1 Flash Free
Branch: oc/clippy-ui-party-pets
Issue: none; CI gates
Next: land after CI.

Finding: clippy `useless_conversion` at lines 187 and 188 of
`crates/benilla-app/src/ui_party/pets.rs`, where `u32::from(...)` wrapped an already-`u32`
value.

Fix: in `record_state`, `health` and `max_health` now read
`r.pet_cur_hp.unwrap_or(0)` and `r.pet_max_hp.unwrap_or(0)` directly. Both fields are
`Option<u32>` in `benilla-protocol-wrath`'s group record, so the conversion was redundant and
the resulting `UnitState` values are unchanged. Diff is two lines in one source file.

Checks not run: no Cargo (build, check, clippy or tests), no client, no server. CI on this
branch is the compile/test gate and also the confirmation that the two findings are gone.

What is not proven: the clippy run itself has not been observed green locally; this rests on
the type of `pet_cur_hp`/`pet_max_hp` being `Option<u32>` and on CI. No gameplay or pet-token
path was exercised, though the change is a type-level no-op with no behavior change.
