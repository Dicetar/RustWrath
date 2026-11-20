# Player-clippy debt: crates/benilla-ui/src/script/secure.rs

Status: ready to merge
Owner: OpenCode / Iceman Free
Branch: oc/clippy-ui-secure
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

Findings: unused `std::collections::HashMap` import (L17) and `redundant_guard` on `Ok(Value::Number(n)) if n == 0.0` (L324).
Fix: removed unused import; replaced guarded arm with `Ok(Value::Number(n)) => n != 0.0`.
CI: not run yet
no behavior change; no new test; the existing the secure/attribute tests in this file are the check
