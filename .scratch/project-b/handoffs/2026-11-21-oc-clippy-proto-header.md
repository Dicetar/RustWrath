# Player-clippy debt: crates/benilla-protocol-wrath/src/header.rs

Status: ready to merge
Owner: OpenCode / LongCat 2.5 Free
Branch: oc/clippy-proto-header
Issue: none; CI gates
Next: none for this file; other player-clippy files remain.

## Finding
`pub fn server_header_len` is called only from the file's `#[cfg(test)]` module, so the lib build reported it as dead code.

## Fix
Added `#[cfg(test)]` immediately after its doc comment; body and test unchanged.
CI: not run yet; no behavior change; no new test.
