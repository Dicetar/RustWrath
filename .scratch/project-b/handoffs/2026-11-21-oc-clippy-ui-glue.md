# Player-clippy debt: crates/benilla-ui/src/script/glue_select.rs

Status: ready to merge
Owner: OpenCode / Hy4 flash Free
Branch: oc/clippy-ui-glue
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Finding: line ~146 GlueRoster::selected_guid had no caller (grep: only its doc and definition; every other hit is the distinct UiScript::glue_selected_guid VM facade) - clippy dead_code.
Fix: removed the method and its doc comment.
Finding: line ~497 the two signed-64-bit bound comparisons (clippy manual_range_contains).
Fix: replaced with (-9223372036854775808.0..9223372036854775808.0).contains(&number); lower inclusive, upper exclusive, unchanged.
CI: not run yet
no behavior change; no new test; the existing the glue roster/SetRealmSplitState tests are the check
