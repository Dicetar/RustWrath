# RELEASED FILE CLAIM: Astra — WotLK interaction and channel catalogs

Status: SOURCE IMPLEMENTED; UNCOMPILED / UNTESTED. Delivery is the commit containing this checkpoint and its two source files. OpenCode remains integration lead. These previously clean source paths were claimed and are now released:

- `crates/benilla-formats/src/lock_type.rs`
- `crates/benilla-formats/src/chat_channels.rs`
- This checkpoint.

Scope: support explicit vanilla/WotLK localized table layouts in existing production loaders. LockType has 29/53 columns with CursorName at 28/52; ChatChannels has 21/37 columns with Shortcut at 12/20. Keep public APIs, blank-name handling, existing channel policy and vanilla behavior. Do not edit OC's application/network/protocol/shared DBC files or any tests.

Evidence: WoWDBDefs LockType.dbd and ChatChannels.dbd build ranges including 3.3.5.12340; local AzerothCore ChatChannelsEntry and its DBCfmt declaration. Existing source callers: entities startup loads LockType, ui_chat/channels startup loads ChatChannels.

Execution restrictions: no builds, tests, linters, new test files, imports, probes, services, game or visual execution. Source/Git review only. Commit source and release this claim when complete; no full replacement or rendered acceptance claim.

## Delivered and source reviewed

Both loaders select only the two known field counts and report unsupported/truncated headers explicitly. Existing DBC parsing retains record-size/body checks. LockType preserves raw localized blank names versus missing rows and omits empty cursor strings. ChatChannels sizes both string arrays from the selected layout, retaining ID/flags/faction positions, name at 3, row order, key field and existing empty-string handling. No public API, app, protocol, shared parser or test changes.

Primary definitions, including build 3.3.5.12340:

- [LockType](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/LockType.dbd): ID, three localized text blocks and CursorName. Eight/sixteen locales plus each block's flags yield 29/53 columns; the cursor follows at 28/52.
- [ChatChannels](https://github.com/wowdev/WoWDBDefs/blob/master/definitions/ChatChannels.dbd): ID/Flags/FactionGroup followed by localized Name and Shortcut. Expanded blocks yield 21/37 columns. Local AzerothCore DBCStructure.h::ChatChannelsEntry and DBCfmt.h confirm name 3-18, flags 19, shortcut 20-35, flags 36 for Wrath.

Reviewed the scoped diff, changed private schema call sites, SchemaField::new_array's usize parameter and the existing production callers. No checks were executed. Real-profile parsing, cursor assets, stock UI presentation and channel joining remain unverified. Locale selection still supports the existing eight locale choices. Existing vanilla channel flags, faction/guild policy and join packet handling are outside this slice; loading Wrath names does not prove Wrath chat compatibility.

When execution is separately authorized: check vanilla regression, real Wrath row/name/shortcut/cursor resolution, blank-name preservation, malformed-layout rejection, then client interaction and channel presentation. OpenCode can consume this commit directly in the shared checkout; do not reimplement or cherry-pick it there. The approved full stock-client replacement gate remains unchanged.
