# Astra source review of OC O5

Date: 2026-10-26, Europe/Moscow. Reviewed source: `920273d5` and its ancestors. Ownership for this review: this checkpoint and the O5 reconciliation in `.scratch/project-b/compatibility/first-loop-behaviors.md`, released in the containing commit. No OC source/checkpoint or parser edits.

## Finding — the claimed MOGP offset shift is not demonstrated

OC's route checkpoint compares whole-chunk-relative offsets from the extractor with payload-relative offsets in Rust. The purported +8 layout difference is exactly the chunk prefix, not a demonstrated vanilla/Wrath version difference.

Authoritative local source: `D:/WOW/WM_BridgeLab/src/azerothcore/src/tools/vmap4_extractor/wmo.cpp::WMOGroup::open`. Lines 184–185 read fourcc and size (eight bytes). Lines 198 onward then read groupName, descGroupName, flags and the remaining header from the payload position. Thus groupName is payload +0, not +8.

| Field | MOGP payload offset | Offset including 8-byte FourCC/size prefix |
|---|---|---|
| groupName | 0x00 | 0x08 |
| descGroupName | 0x04 | 0x0C |
| flags | 0x08 | 0x10 |
| Bounding boxes, 24 bytes | 0x0C–0x24 | 0x14–0x2C |
| portal start/count | 0x24 / 0x26 | 0x2C / 0x2E |
| batch A/B | 0x28 / 0x2A | 0x30 / 0x32 |
| extractor's four-byte nBatchC read | 0x2C | 0x34 |
| fog indices | 0x30 | 0x38 |
| groupLiquid | 0x34 | 0x3C |
| groupWMOID | 0x38 | 0x40 |

The extractor's batch-C read spans the C word and adjacent header word; it is listed as the actual four-byte read, not a claim that every external format defines that count as a u32. Its cursor nevertheless lands on the same fog/liquid offsets.

`crates/benilla-bytes/src/lib.rs::chunks` sets `start = pos + 8` and returns `&b[start..end]`. `benilla-wmo::parse_wmo` passes that payload to `parse_group`, which reads flags at 8 and liquid at 0x34. `crates/benilla-formats/src/models/wmo/mod.rs::find_wmo_chunk` likewise strips the prefix before `group::wmo_group_header` reads flags/portal/fog/liquid/area ID. Those offsets agree with the extractor reads. The geometry sub-chunks start at payload +68, or whole MOGP chunk +76; using 68 on both coordinate systems is inconsistent.

Do not shift either parser or add a version branch based on this report. OC correctly held the edit, but should retract/correct the mismatch claim. This does not prove every WMO format/asset is compatible; a real asset/layout defect still needs evidence using one consistent offset origin. Source inspection resolves this specific arithmetic issue without a build, extraction or graphical run.

## O5 delivery classification

Commit `920273d5` changes one Markdown checkpoint and no source. It adds useful static route facts: tile 32/48 and a minimum 3×3 neighborhood, display/model dependencies, item icons and Fireball visual/kit IDs. These improve preparation, not graphical or gameplay acceptance.

The checkpoint still does not provide exact Northshire abbey WMO/doodad internal paths, complete kit-to-texture dependencies or manifest/archive hash identities for the gate. Its two NPC display chains do not establish that the actual selected player is the intended human mage; the Auto boot still selects the first enumerated character. Worn reward appearance is outside a narrow receipt proof but remains required replacement behavior. Geometry, batch/material and collision correctness cannot be concluded as accepted solely from source pipeline existence.

## Source blockers still observed at this checkpoint

- `wrath.rs` defines `W_FLAG_FALLING = 0x2000` and `W_FLAG_SWIMMING = 0x02000000`; the pinned 12340 definitions use falling 0x1000 and swimming 0x00200000. The latter current constant is flying.
- `wrath_stream_movement` still removes the shared outbox's oldest entry at length 64, while action admission permits up to 256, so critical commands can be silently removed by movement. R01/R03 from the architectural review remain open.
- Compilation/check/graphical/lab/receiver/WM acceptance remains UNKNOWN. Do not interpret “scenario item path verified” as more than source review without matching execution evidence.

Recommended OC next source work: correct the O5 offset claim, finish concrete missing route dependency pins, repair movement and command admission, then continue O6 through existing WM owners. No new runtime authorization is inferred. O7 consumes the updated behavior inventory; it does not waive the full stock-client replacement gate.

## Provenance and restrictions

The mixed-index disclosure matches the existing Astra record: initial inventory/procgen docs landed in OC's `e0be0d95`, then Astra committed refinements/reconciliation. History and contents are preserved. Do not rewrite shared commits to clean up attribution.

This review ran only source/Git reads and explicit-path document edits. No tests, builds, linters, formatter, imports, archive extraction, renderer, browser, sockets, game, WM/receiver/server process, DB mutation or external message. ADR-0001 remains Proposed. The untracked GLM patch script is untouched.
