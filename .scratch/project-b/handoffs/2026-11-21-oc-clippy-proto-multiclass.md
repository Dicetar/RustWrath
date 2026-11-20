# Player-clippy debt: crates/benilla-protocol-wrath/src/multiclass.rs

Status: ready to merge
Owner: OpenCode / Beam Free
Branch: oc/clippy-proto-multiclass
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Finding: line ~176 chunks_exact(18) -> as_chunks::<18>().0 (clippy chunks_exact_to_as_chunks).
Fix: replaced chunks_exact(18) with as_chunks::<18>().0; loop body only indexes rows, no as_slice needed.
CI: not run yet
no behavior change; no new test; the existing the multiclass reader tests are the check
