# Crate Splitting & Live-Code Hotpatch Plan

Date: 2026-11-26  
Related: [`adr-0002-live-client-modules.md`](adr-0002-live-client-modules.md), `crates/benilla-client/src/lib.rs`, `crates/benilla-app/src/lib.rs`, `Cargo.toml`

## 1. Goal & Core Principle

Speed up live-code hotpatch iteration (`dx serve --hot-patch` / `-LiveCode`) and incremental `dev` rebuilds by shrinking `crates/benilla-app` **selectively** — peeling cohesive, high-churn leaf subsystems **above** `benilla-app` and wiring them in the thin `benilla-client` assembler (the pattern already established by `benilla-dev` and `benilla-souls`).

### Why "split above `benilla-app`", not "split everything"

1. **Live code (`dx serve --hot-patch`) replays upward through the dependency graph.**  
   As documented in [`adr-0002-live-client-modules.md`](adr-0002-live-client-modules.md) (`## Live code`), our patched `dx` watches all workspace crates transitively and replays **the edited crate plus every workspace crate that depends on it** (`compile_workspace_hotpatch`):
   - Editing a leaf crate **above** `benilla-app` (wired into thin `benilla-client` + `benilla`) skips compiling `benilla-app` completely.
   - Editing a crate **below** `benilla-app` (such as `benilla-ui`, `benilla-formats`, `benilla-world`, `benilla-protocol*`) still forces `dx` to replay `benilla-app` + `benilla-client` + `benilla`.
2. **Splitting crates does not remove subsecond's runtime limits.**  
   Struct/enum layout changes, new or reordered Bevy systems/sets/observers/plugins, static or `thread_local!` initializer changes, and new dependencies still require a restart regardless of crate boundaries.
3. **Native DLL hotpatch modules (`benilla-hotpatch`) are a separate mechanism.**  
   Moving Rust code into a separate crate does not make it a hot-swappable production DLL. That path requires an explicit Bevy-free C-ABI contract (`benilla-hotpatch-abi`), host-owned state copies, validation, quarantine, and rollback.
4. **Over-splitting hurts both compile time and dev FPS.**  
   Deep dependency chains cause sequential `rustc` replays in `dx`. With `lto = "off"` in `dev` and `play` (`Cargo.toml`), splitting tight per-frame loops across tiny crates prevents cross-crate inlining and duplicates Bevy `Query`/`System` monomorphization.

---

## 2. Current Workspace Baseline (2026-11-26)

### Largest workspace crates

| Crate | `.rs` Files | Total LOC | Position relative to `benilla-app` |
|---|---:|---:|---|
| `benilla-app` | 801 | 594,798 | Core (main bottleneck; ~25 s single-line `dev` edit noted in `Cargo.toml`) |
| `benilla-ui` | 399 | 265,810 | Below `benilla-app` (edits replay `benilla-app`) |
| `benilla-formats` | 173 | 66,207 | Below `benilla-app` |
| `benilla-world` | 135 | 64,351 | Below `benilla-app` |
| `benilla-protocol` | 152 | 37,277 | Below `benilla-app` |
| `benilla-dev` | 71 | 30,009 | **Above `benilla-app`** (skips core on edit) |
| `benilla-souls` | 9 | 7,762 | **Above `benilla-app`** (skips core on edit) |
| `benilla-client` | 1 | 332 | Thin assembler on top |

### Inside `crates/benilla-app/src`

- **170** top-level modules
- **593,907** total lines: **188,258 production LOC (31.7%)** and **405,649 unit-test LOC (68.3%)**
- Single biggest structural blocker to extracting UI features: **`crates/benilla-app/src/script_calls.rs` (`Appliers`)**, a monolithic `disjoint_set!` `SystemParam` that statically imports ~45 `ui_*` modules. Because 65 other modules take `HostedVm` / `VmWith`, any module wired into `Appliers` is pulled to the bottom of `benilla-app` even if nothing else in the core calls it.

---

## 3. Non-Negotiable Architecture Constraints

Every new leaf crate extracted above `benilla-app` must follow these rules:

1. **Narrow explicit seams only (Linux CI OOM guard).**  
   Per `crates/benilla-app/src/lib.rs` (`souls_seam` / `dev_seam`), **never** expose whole hub modules (`pub mod net`, `pub mod ui_script`, etc.). Making every `pub` item in a hub externally reachable caused the Linux CI runner to OOM. Each leaf crate gets a minimal `#[doc(hidden)] pub mod <crate>_seam` re-exporting only the exact symbols it imports.
2. **State-in-core, engine-above pattern.**  
   Where core modules need to trigger or read state owned by a leaf crate (e.g., audio requests, combat log triggers, Souls state), keep the tiny data/message/state resource and system-set markers in `benilla-app`, and move the heavy implementation crate above `benilla-app` (as `benilla_app::souls::SoulsStatePlugin` + `benilla_souls::SoulsPlugin` already do).
3. **Thin `benilla-client` wiring and feature hygiene.**  
   - Leaf crates depend on `benilla-app = { path = "../benilla-app", default-features = false }` and forward `dev = ["benilla-app/dev"]` so `--no-default-features` player builds remain clean.
   - Register leaf plugins in `crates/benilla-client/src/lib.rs` with explicit `.add_after::<...>()` / `.add_before::<...>()` edges matching the original `game_plugins.rs` ordering.
4. **Cohesive domain crates (target 3–5 leaves, not 30).**  
   Keep mutually dependent modules together (for example `portrait` + `minimap` + `ui_models`) so seam maintenance and `dx` replay depth stay minimal.

---

## 4. Prioritized Extraction Plan

### Phase 0 — Low-Risk Pilots (Zero Core Redesign)

Validate the extraction template on two modules that are already near-pure leaves:

1. **`benilla-addon-harness`** (or move into `benilla-dev`)
   - **Modules:** `addon_harness`
   - **Size:** 5,810 total / 819 prod LOC (9 files)
   - **Current inbound prod callers in `benilla-app`:** **0**
   - **Work:** Move module, re-export the small `ui_script` harness hooks via a narrow seam, update tests.
2. **`benilla-ui-tooltip`** (can later merge into a broader UI panels crate or stay standalone)
   - **Modules:** `ui_tooltip`
   - **Size:** 7,220 total / 2,255 prod LOC (9 files)
   - **Current inbound prod callers in `benilla-app`:** **2** (`ui_aura` only orders `.after(crate::ui_tooltip::MouseoverPublish)`; `spell` has one narrow edge)
   - **Unlock:** Keep `MouseoverPublish` (a unit `SystemSet` marker) in `benilla-app` and extract the tooltip builder/renderer above `benilla-app`.

---

### Phase 1 — Biggest Single Engine Drop: `benilla-sound`

- **Modules:** `sound/`
- **Size:** **16,465 total / 11,468 prod LOC** (38 files — 3rd largest production module in `benilla-app`)
- **Why first after pilots:** Largest self-contained engine block in `benilla-app`, and removes the `kira` audio stack from `benilla-app`'s compile graph.
- **Current coupling (15 prod callers):** Callers in `benilla-app` already communicate mostly through data resources (`MessageSounds`, `EmoteSounds`, sound-kit trigger queues) rather than calling mixer internals directly.
- **Unlock:**
  1. Leave a slim `benilla_app::sound_state` (or `sound` facade) holding only `MessageSounds`, `EmoteSounds`, `SoundConfig`, and event/request types read/written by core modules.
  2. Move the entire `kira` backend, spatial mixing, ambient/music/footstep/UI playback systems, and DBC sound-kit decoding into `crates/benilla-sound` above `benilla-app`.
  3. Wire `benilla_sound::SoundPlugin` in `benilla-client` right after the core sound-state plugin.
  4. Re-export only the required read-only core resources through `benilla_app::sound_seam`.

---

### Phase 2 — Highest Live-Code Iteration Win: `ScriptCallHost` Seam + UI Feature Panels

This is the highest-leverage split for day-to-day Wrath development because most active UI/gameplay feature work lands in `ui_*` panels.

#### Step 2A: Decouple non-combat window verbs from `script_calls::Appliers`
- **Keep in `benilla-app`'s static `Appliers`:** macro-ordered, combat-critical synchronous calls where ordering within a single Lua turn is load-bearing (`select`, `cursor`, `self_cancel`, `cast_state`, `cast`, `action`, `action_states`, `item_use`, `items`, `attack`, `stand`, `chat`, `emote`, `session`, `latch`).
- **Introduce one extensible synchronous hook on `ScriptCallHost`:** a registered handler table (keyed by `ScriptCall` category or trait-object dispatchers registered by leaf plugins at startup) so non-combat window/panel verbs still execute **synchronously inside the Lua call** without `script_calls.rs` statically naming every `ui_*` module.

#### Step 2B: Extract cohesive UI panel leaf crates above `benilla-app`
Once Step 2A lands, ~40+ `ui_*` modules (**~40k–133k total / ~14k–37k prod LOC**) become clean leaves. Group them into **2–3 cohesive domain crates** rather than per-window crates:

1. **`benilla-ui-economy`** (~16k+ total LOC)
   - `ui_auction` (5,101 tot / 547 prod), `ui_mail` (4,393 tot / 892 prod), `ui_trade` (5,320 tot / 431 prod), `ui_merchant` (2,757 tot / 547 prod), `ui_bank` (340 tot / 298 prod), `ui_tradeskill` (3,062 tot), `ui_craft` (1,474 tot / 540 prod).
   - Shared state (e.g., `AuctionOpen`, `MailOpen`, `BankOpen`, `MerchantOpen`) either stays as thin marker/open-state resources in `benilla-app` if `dev_seam` / `target` reads them, or moves cleanly with narrow re-exports.
2. **`benilla-ui-social-pvp`** (~12k+ total LOC)
   - `ui_battlefield`, `ui_battlefield_positions`, `ui_battlefield_score`, `ui_honor`, `ui_duel`, `ui_guild`, `ui_petition`, `ui_tabard`, `ui_social`.
   - Start with the zero-external-caller subset (`ui_battlefield`, `ui_duel`, `ui_honor`), then pull the remaining guild/social panels once their `script_calls` verbs route through Step 2A.
3. **`benilla-ui-panels`** (character, travel & dialogs; ~15k+ total LOC)
   - Zero-external-caller batch first: `ui_char` (1,705 tot / 919 prod), `ui_instance` (995 tot / 497 prod), `ui_gm_ticket` (742 tot / 452 prod), `ui_shapeshift` (565 tot / 302 prod), `ui_summon` (400 tot / 269 prod), `ui_quest_share` (360 tot / 253 prod), `ui_loot_roll` (1,843 tot / 231 prod).
   - Followed by: `ui_talent`, `ui_talent_wipe`, `ui_taxi`, `ui_trainer`, `ui_dressup`, `ui_binder`, `ui_stable`, `ui_item_text`, `ui_reputation`, `ui_tooltip` (if consolidated).

---

### Phase 3 — Secondary Presentation & Log Leaves (Optional, After Phases 1–2)

Pursue only if `benilla-app` rebuild time is still noticeable after Phases 1–2:

1. **`benilla-combat-log`**
   - **Modules:** `combat_log` (**6,048 total / 2,589 prod LOC**, 11 files)
   - **Current coupling:** Only 5 prod callers (`creature_anim`, `spell`, `ui_chat`, `ui_reputation`, `ui_unit`).
   - **Unlock:** Publish combat-log events into a core event buffer in `benilla-app`; move formatting, filtering, and chat dispatch above `benilla-app`.
2. **`benilla-presentation`**
   - **Modules:** `portrait` (11,499 tot / 4,020 prod) + `minimap` (4,139 tot / 2,317 prod) + `ui_models` (2,894 tot / 2,077 prod) + world FX polish (`chat_bubble`, `combat_text`, `raid_marks`, `quest_markers`, `poi_marker`, `footprints`, `blob_shadow`, `weapon_trail`, `bowstring`, `ranged_flex`, `fishing_line`, `screen_fade`, `camera_shake`, `world_backdrop`) — **~30k total / ~13.5k prod LOC**.
   - **Why grouped together:** `portrait`, `minimap`, and `ui_models` reference each other and `pipe_warm` / `ui_pass`; moving them as one presentation crate avoids cross-crate cycles.

---

## 5. What Stays in `benilla-app` (Do Not Extract)

These modules are the tightly connected core hubs. Moving them below `benilla-app` will **not** speed up live-code hotpatching (edits still replay `benilla-app`), and splitting them horizontally creates trait/seam churn:

| Module | Prod LOC | Total LOC | Inbound Prod Callers | Role |
|---|---:|---:|---:|---|
| `net` | 8,975 | 22,625 | 104 | Session, object store, GUIDs, commands |
| `ui_script` | 16,217 | 105,736 | 92 | Lua VM host, frame lifecycle, input routing |
| `script_calls` (core) | 635 | 8,859 | 65 | Synchronous macro/combat call host |
| `ui_action` | 3,819 | 11,842 | 51 | Action bar, spells catalog, attacks |
| `entities` | 8,919 | 18,439 | 45 | Entity spawn, display, attachments, spell FX |
| `ui_unit` | 2,200 | 9,531 | 38 | Unit feed, health/power/name tables |
| `player` | 8,714 | 24,938 | 36 | Player controller, movement, camera |
| `target` | 6,968 | 21,586 | 33 | Selection, hover, click, interact |
| `creature_anim` | 5,576 | 21,690 | 32 | Rig animation state machine |
| `wrath` | 11,695 | 20,666 | 31 | 3.3.5a session & wire translation bridge |
| `spell` | 7,761 | 27,952 | 28 | Casting, targeting cursor, spell modifiers |
| `ui_items` | 3,012 | 12,490 | 24 | Bags, paperdoll, cursor item use |
| `ui_chat` | 7,705 | 22,830 | 21 | Chat frames, slash commands, emotes |
| `ui_party` | 1,373 | 10,654 | 19 | Group/raid roster & member stats |
| `cvars` | 2,097 | 5,859 | 18 | CVar registry and persistence |

---

## 6. Per-Extraction Verification Checklist

For each extracted crate (`benilla-<leaf>`):

1. **Manifest & features**
   - Workspace member auto-discovered under `crates/benilla-<leaf>`.
   - Depends on `benilla-app = { path = "../benilla-app", default-features = false }`.
   - Forwards `dev = ["benilla-app/dev"]` (and `test-support` only if tests need fixtures).
   - Player build (`--no-default-features`) remains clean with no dev-only leakage.
2. **Narrow seam & CI memory**
   - Add a dedicated `#[doc(hidden)] pub mod <leaf>_seam` in `crates/benilla-app/src/lib.rs` with explicit item re-exports (no wildcard or whole-hub `pub mod`).
   - Verify Linux CI build completes without memory regression.
3. **Schedule & plugin order**
   - Wire plugin registration in `crates/benilla-client/src/lib.rs` preserving exact ordering edges from `game_plugins.rs`.
   - Keep `game_plugins::schedule_tests` / headless client schedule census passing.
4. **Live-code hotpatch check (`-LiveCode`)**
   - Edit a function body inside `crates/benilla-<leaf>/src/` during a `dx serve --hot-patch` session and confirm `dx.log` replays only `benilla-<leaf>`, `benilla-client`, and `benilla` (skipping `benilla-app`).
   - Measure before/after patch wall-clock time on the target Windows workstation.
