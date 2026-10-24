# Project B baseline identity manifest (B-001 working draft)

Started 2026-10-24. Status: PARTIAL — populated from verified planning-round evidence; recomputation and remaining inventory items tracked by ticket B-001.

## Executables (hashed first-hand this session, certutil SHA-256)

| File | SHA-256 | Notes |
|---|---|---|
| Wow.exe.multiclass-original | 3e27e6e5c79a6689877c9c680811de07d553d9311c8335c98cb2e6b1e091a411 | matches client-pin.json original_exe_sha256 (3e27e6e5…a411) |
| installed wow.exe | 537df9fc60dd946c3f1f3c65c37b586f3da99c6d9b58a783349d1c39431d3556 | matches client-pin.json exe_sha256 (537df9fc…d356); PE FileVersion 3,3,5,12340; 7,705,600 bytes |

Client pin record: D:/WOW/wm-project/client_extensions/multiclass/client-pin.json (custom opcodes 0x051b client / 0x051a server reserved; dll_sha256 recorded there).
Stock authenticity of the "original": NOT independently established — blocks only conclusions needing it.

## Source revisions

| Tree | HEAD | Role |
|---|---|---|
| D:/WOW/WM_BridgeLab/src/azerothcore | 5e741c37364f4ec5e3bc2860bcf21f13812fb66d | inspected AC integration checkout; D:/WOW/WM_BridgeLab/run = documented deployment |
| D:/WOW/wm-project | 011a7b35aead069691a9a8370ecd6bcd42fc4356 | WM source owner (substantial dirty changes — preserve) |
| D:/Projects/project-b-client (this fork) | fork base 359eff416a411bdf3a6a39cea6d2b06d63b82c55, branch wotlk, upstream = github.com/samwhosung/benilla | Astra reviewed tip bc59b5d; upstream has since moved — implementation must re-read files at pinned fork base |

## Server config

worldserver.conf (WM_BridgeLab/run/configs): MapUpdateInterval = 10 declared (worldserver.conf:1354) — declared, not measured.

## Remaining B-001 items

- [ ] deployed binary ↔ source reconciliation (run/worldserver.exe vs HEAD; module copies/links)
- [ ] DB revision identity (world/characters schema versions)
- [ ] asset profile enumeration: base vs wm-hd lettered patches; archive enumeration of what the known pair actually loads (mechanism currently UNKNOWN)
- [ ] locale confirmation (enUS assumed — verify)
- [ ] benilla pinned-revision reread of MAP.md/DEVIATIONS.md/Cargo.toml (Bevy 0.18.1 noted at review tip)
