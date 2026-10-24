# Project B baseline identity manifest (B-001)

Started 2026-10-24. Status: PARTIAL — first full inventory pass done this session; remaining items listed at the end. All hashes computed first-hand with certutil this session unless noted.

## Executables (client)

| File | SHA-256 | Notes |
|---|---|---|
| Wow.exe.multiclass-original | 3e27e6e5c79a6689877c9c680811de07d553d9311c8335c98cb2e6b1e091a411 | matches client-pin.json original_exe_sha256 |
| installed wow.exe | 537df9fc60dd946c3f1f3c65c37b586f3da99c6d9b58a783349d1c39431d3556 | matches client-pin.json exe_sha256; PE FileVersion 3,3,5,12340; 7,705,600 bytes |

Client pin: D:/WOW/wm-project/client_extensions/multiclass/client-pin.json. Stock authenticity of the "original": NOT independently established — blocks only conclusions that need it. Locale: **enUS confirmed** (WTF/Config.wtf `SET locale "enUS"` + Data/enus).

## Server deployment identity

- **Live deployment home: `WM_BridgeLab/run/bin/worldserver.exe`** — 48,444,928 bytes, 2026-10-07 02:07, SHA-256 `9cd24a0cca001bf04e4af7ea4faccf3b3825eabab2d98a824363def2c21ee0f8`. Companion authserver.exe, DLLs, Playerbots.log/Server.log/Crashes in the same dir.
- **`WM_BridgeLab/run/worldserver.exe` (Jun 11, 46,621,696 B) is a STALE duplicate** — one directory up from the real deployment. Do not use or patch it.
- Deployment lineage Oct 2→7 recoverable from `WM_BridgeLab/state/*-before*/worldserver.exe` backups (24 binaries catalogued, sizes/dates recorded in session log; `spellwright-los-…-220357` and `multiclass-vendor-raidreset-…-020834` bracket the final two builds).
- `Azerothcore_WoTLK_Repack/worldserver.exe` (Mar 13, 58.5 MB) = separate repack distribution, reference only. `Azerothcore_WoTLK_Rebuild/run` = launcher wrapper with symlinks into the Repack (data, mysql), no binaries of its own.
- Server OFFLINE at inventory time (no worldserver/mysqld processes) — "which tree is live" settled by artifact evidence above, not process inspection.
- Source: AC HEAD `5e741c37…` (WM_BridgeLab/src/azerothcore). Binary-vs-source reproducibility: unverified (would need rebuild; PDB sits beside the binary for symbol-level comparison if ever needed).
- Config: `run/configs/worldserver.conf`, `MapUpdateInterval = 10` declared (line 1354).

## Module ownership map

| Module | Owner (ahead) | Deployment copy | Drift |
|---|---|---|---|
| mod-multiclass | **wm-project/native_modules** | WM_BridgeLab/src/modules | wm-project ahead: characters SQL 008–011 (incl. 011_spellforge_v3), newer world SQL 001/003/004+, newer README. C++ SpellmakingV3.cpp byte-identical. |
| mod-wm-bridge | effectively in sync | WM_BridgeLab | BridgeLab adds `test/out` artifacts only |
| mod-wm-spells | effectively in sync | WM_BridgeLab | BridgeLab adds `test/out` artifacts only |

Rule confirmed: edit in wm-project (source owner); BridgeLab is the integration/deployment checkout. ~60 community modules also present in BridgeLab only (incl. mod-playerbots, mod-ollama-chat).

## Schema revisions (static evidence)

wm-project mod-multiclass SQL: world 001→011, characters 008→011 (011_spellforge_v3 = Spellwright v3 tables: wm_spellforge_module/revision/design/binding/retired). Actual DB contents unverified — **needs a running mysqld** (offline now).

## Extension-ID ledger (custom protocol slots)

| Slot | Direction | Owner | Evidence |
|---|---|---|---|
| 0x051b | client→server request | multiclass client ext | client-pin.json ("obsolete commentator command", patched .text forwarding) |
| 0x051a | server→client reply | multiclass client ext | client-pin.json ("confirmed empty in both Glue and the active world client") |

Grep of client_extensions + native modules found no further custom opcode reservations. WM server↔client messaging otherwise rides addon whisper channels (WM_FORGE2/WM_FORGE3). Constraint carried forward: client "refuses a nonempty receive slot" — client-slot reuse must account for this.

## Fork base (this repo)

- Fork base commit `359eff416a411bdf3a6a39cea6d2b06d63b82c55`, branch `wotlk`, upstream remote = github.com/samwhosung/benilla (Astra reviewed tip `bc59b5d`; upstream has moved since — files were reread at fork base, not at review tip).
- Toolchain pin: Rust **1.98.1** (rust-toolchain.toml; local default 1.99.1 — rustup handles the pin). Bevy **0.18.1**, wgpu renderer (no D3D9; `gxApi` deviation is descriptive only).
- Crate map highlights (docs/MAP.md, generated): pure-Rust no-Bevy format crates (mpq/blp/dbc/adt/wdt/wmo/m2/bytes/formats), `benilla-lua` in-house interpreter + `benilla-mlua` shim, `benilla-ui` engine-free UI core, `benilla-protocol`/`benilla-srp`, `benilla-world` renderer with "no game attached", `benilla-worldview` standalone viewer, **`benilla-visual` perceptual image-diff A/B harness** (reusable for our differential rendering tests).
- DEVIATIONS.md discipline: deviations live as `Deviation:` comments in code and are regenerated into docs — the retarget must follow this convention.

## Remaining B-001 items

- [ ] DB revision identity against a running lab MySQL (world + characters schema versions, wm_spellforge_* present)
- [ ] Archive enumeration: static content listing of base vs wm-hd MPQs; empirical verification of what the known-original pair actually loads (mechanism currently unknown)
- [ ] Full file-level drift appendix for mod-multiclass (beyond the sampled diff)
- [ ] Baseline toolchain proof: `cargo check` on fork base (running, background)
