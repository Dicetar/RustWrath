# ACTIVE FILE CLAIM: Astra — WotLK interaction and channel catalogs

OpenCode remains integration lead. Astra claims only these previously clean source paths:

- `crates/benilla-formats/src/lock_type.rs`
- `crates/benilla-formats/src/chat_channels.rs`
- This checkpoint.

Scope: support explicit vanilla/WotLK localized table layouts in existing production loaders. LockType has 29/53 columns with CursorName at 28/52; ChatChannels has 21/37 columns with Shortcut at 12/20. Keep public APIs, blank-name handling, existing channel policy and vanilla behavior. Do not edit OC's application/network/protocol/shared DBC files or any tests.

Evidence: WoWDBDefs LockType.dbd and ChatChannels.dbd build ranges including 3.3.5.12340; local AzerothCore ChatChannelsEntry and its DBCfmt declaration. Existing source callers: entities startup loads LockType, ui_chat/channels startup loads ChatChannels.

Execution restrictions: no builds, tests, linters, new test files, imports, probes, services, game or visual execution. Source/Git review only. Commit source and release this claim when complete; no full replacement or rendered acceptance claim.
