# Preserve byte based UTF-8 clipping in debugstack

Status: ready to merge
Owner: Claude Opus 5.5
Branch: fix/wrath-debugstack-utf8
Issue: none
Next: run the focused `benilla-ui` stdlib test in the owner's Cargo slot, then review/merge

## Change

`debugstack()` now uses the VM-provided `DebugSource::short_src` directly. The VM already applies the WoW/Lua 5.1 `luaO_chunkid` rule using byte limits. The UI had been rebuilding that label from the full source and counting Unicode characters, so a long UTF-8 addon name could exceed the 60-byte runtime buffer and produce a different stack label.

The obsolete duplicate `chunk_id` implementation and its constants were removed. The fallback to the full source remains for debug providers that do not expose `short_src`.

## Verification

- `rustfmt --edition 2021 --check crates/benilla-ui/src/script/stdlib.rs crates/benilla-ui/src/script/tests/stdlib.rs`: passed.
- `git diff --check`: passed.
- Cargo tests/builds were not run because the repository reserves the Cargo slot for the owner.
- Client/server were not launched; no game data, credentials, or runtime state was accessed.

## Focused test

`debugstack_clips_long_utf8_chunk_names_by_bytes` loads a named chunk with 30 copies of `é` (60 UTF-8 bytes), calls `debugstack`, and checks the `...` prefix plus the expected 26-character, 52-byte tail.
