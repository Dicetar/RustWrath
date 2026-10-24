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

## Schema revisions (verified live, 2026-10-24)

Lab MySQL started via `wm-project/start-bridge-lab-mysql.bat` (mysqld PID 39852, port 33307, left running). Server: **MySQL 8.4.7** (not MariaDB — relevant for any DB tooling choice). Databases: `acore_auth`, `acore_world`, `acore_characters`.

- `acore_world.updates`: 2,805 applied (latest: `zone_zangarmarsh.sql`). `acore_characters.updates`: 52.
- **Spellwright v3 confirmed live**: all five tables present in `acore_characters` — `wm_spellforge_module/revision/design/binding/retired`.
- `acore_world` carries the full 50-table WM machinery: bridge bus (`wm_bridge_action_request`, `wm_bridge_event`, context/perception/presence/scope), control contracts (`wm_control_proposal/apply_lock/schema_version`), director (`wm_director_intake/work/request/artifact/authorization/effect/proof/transition/development_task`), reserved IDs/slots, `wm_rollback_snapshot`, `wm_publish_log`, shell bank (`wm_spell_shell/behavior/grant/trigger/debug_request`), journal/subject tables, plus content-specific tables (`wm_broug_*`, `wm_lanathel_stance_state`, `wm_bonebound_echo_stasis`, `wmoareatable_dbc`).
- `acore_characters` multiclass lineage: `wm_multiclass_profile/snapshot/schema/training/hunt/bonus_xp/receipt/dungeon_receipt/unlocked_equipment` + `wm_character_*` journal/memory/arc tables.

**mod-multiclass drift (full appendix: `mod-multiclass-drift.txt`): zero C++/header drift** between wm-project owner and BridgeLab deployment copy — differences are SQL only (wm-project ahead: world 006–011, characters 008–011) plus README. The live DB already contains those tables, so the deployment copy's older SQL set matters only for fresh bootstraps.

## Fork base (this repo)

- Fork base commit `359eff416a411bdf3a6a39cea6d2b06d63b82c55`, branch `wotlk`, upstream remote = github.com/samwhosung/benilla (Astra reviewed tip `bc59b5d`; upstream has moved since — files were reread at fork base, not at review tip).
- Toolchain pin: Rust **1.98.1** (rust-toolchain.toml; local default 1.99.1 — rustup handles the pin). Bevy **0.18.1**, wgpu renderer (no D3D9; `gxApi` deviation is descriptive only).
- Crate map highlights (docs/MAP.md, generated): pure-Rust no-Bevy format crates (mpq/blp/dbc/adt/wdt/wmo/m2/bytes/formats), `benilla-lua` in-house interpreter + `benilla-mlua` shim, `benilla-ui` engine-free UI core, `benilla-protocol`/`benilla-srp`, `benilla-world` renderer with "no game attached", `benilla-worldview` standalone viewer, **`benilla-visual` perceptual image-diff A/B harness** (reusable for our differential rendering tests).
- DEVIATIONS.md discipline: deviations live as `Deviation:` comments in code and are regenerated into docs — the retarget must follow this convention.

## Archive enumeration (static, benilla-mpq reader, 2026-10-24)

The fork's own MPQ reader lists WotLK archives without modification. Profile split (stock 3.3.5a ships only patch/patch-2/patch-3 + patch-enus/-2/-3 — **everything else lettered/numbered beyond 3 is custom**):

**base profile (stock):**
- Root: common (83,670 files), common-2 (43,083: 14,726 m2 / 6,532 wmo / 2,418 adt), expansion (11,871), lichking (16,179: 2,742 m2 / 2,380 wmo / 1,804 adt), patch (37,550), patch-2 (10,577), patch-3 (3,099)
- enus: locale-enus (11,054: 224 dbc), speech-enus, expansion-locale/speech-enus, lichking-locale/speech-enus, patch-enus (5,372: 167 dbc), patch-enus-2 (2,335: 139 dbc), patch-enus-3 (949: 111 dbc)

**wm-hd profile additions (custom):**
- Root: patch-6 (1,935: 85 adt — custom terrain), patch-7 (22,281 — large content patch), patch-8 (1,295), patch-S (1,785), **patch-a (40,249: 6,169 m2 — the HD model pack)**, patch-b (42), patch-c (2,049), patch-e (586), patch-f (10,564), patch-k (194), patch-w (479: `Liquidtype.dbc` override), patch-y (2), **patch-z (6: the WM patch — 83 MB `Spell.dbc` with extended carrier ranges + `Item.dbc`/`SkillLineAbility.dbc`/`SkillRaceClassInfo.dbc` + `Interface\AddOns\WMBridge`)**
- enus custom: patch-enus-a (756), -c (67), -f (4: 3 dbc), -k (59), -m (1,660), -q (26), -w (1 dbc); plus **nonstandard** `backup-enus` (192) and `base-enus` (188) — not stock names, provenance unknown
- Owner-disabled (`.disabled`): patch-5, patch-j, patch-u — excluded from both profiles; loader must honor this toggling pattern

Patch precedence: MPQ patch chain loads in lexicographic order (patch < patch-2 < … < patch-9 < patch-a < … < patch-z); higher archives override lower for the same internal path. **Empirical verification of what the client actually opens (load order + final resolution) remains pending — ticket 05.**

## Remaining B-001 items

- [x] DB revision identity — verified live 2026-10-24 (section above)
- [x] Archive enumeration (static) — profiles defined above; **empirical load-order verification carries to ticket 05**
- [x] Full file-level drift appendix — `mod-multiclass-drift.txt` (zero C++ drift)
- [x] Baseline toolchain proof: `cargo check` on fork base — PASSED exit 0, 2026-10-24
- [x] benilla-extract built and proven against WotLK archives (bonus: upstream's MPQ reader needs no changes for listing)
