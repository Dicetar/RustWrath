# B-004 findings — route assets, profiles, and first WotLK format facts

2026-10-24. All facts sampled first-hand via `profile-probe` (benilla-profile crate) and `benilla-extract` against the owner's install. Companion code: `crates/benilla-profile` (+ `profile-probe` CLI), manifests `docs/profiles/{base,wm-hd}.mpqlist`.

## Profile loader (delivered)

- Manifest-defined chains (`stock|wm <name.mpq>`, ascending precedence); **no auto-discovery, no writes to Data**; a manifest entry that is `.disabled` on disk is a **hard error naming both files** (the owner's toggles are law).
- Per-file provenance (`status`, `classify`, `read`, `list` commands); `reload()` is the C1 hot-swap seam for client-side patching.
- Failure classification per plan rev 3: profile-dependent = override/interaction hypothesis; both-profile = parser/dependency hypothesis. Never auto-blame.
- 3 unit tests green; builds clean under the workspace lints.

## Provenance results (acceptance demos)

| File | base | wm-hd | Class / meaning |
|---|---|---|---|
| `Interface\AddOns\WMBridge\WMBridge.lua` | missing | **patch-z.mpq (wm)**, 20,086 B | **WmOnly** — the deliberately introduced custom asset, classified as HD-profile-caused ✓ |
| `World\Maps\Azeroth\Azeroth.wdt` | patch-3.mpq, 32,839 B | **patch-6.mpq (wm)**, 32,839 B | Shared-readable but **archive-overridden**: WM's patch-6 replaces the world tile table (custom terrain registration) |
| `World\Maps\Azeroth\Azeroth_30..33_48.adt` | patch.mpq / patch-2.mpq | same stock archives | Shared — route terrain is stock in both profiles |
| `Creature\Wolf\Wolf.m2` | common-2.mpq, 431,152 B | **patch-f.mpq (wm)**, 625,352 B | Shared-readable, HD model swap (real HD-pack evidence) |

## First WotLK format facts (sample-verified)

1. **Terrain layout**: `World\Maps\<Map>\<Map>_<col>_<row>.adt` (not 1.12's `Maps\...`); route tiles confirmed at 30..33 × 48 (Northshire→Elwynn). ADT header: **version 18**, chunk IDs appear byte-reversed on disk in this install (`REVM` + size 4 + `18` + `RDHM`…); readers that compare chunk magic as u32 are unaffected — verify per file family when parsing.
2. **Elwynn liquid = MH2O only**: `Azeroth_31_48.adt` has 1 × `MH2O`, 256 × `MCNK`, **zero `MCLQ`**. The route needs the modern liquid reader; legacy MCLQ is not required for this route (per-tile check for others).
3. **M2 corpus mixes model versions**: stock `Creature\Boar\Boar.m2` = `MD20` **version 264** with **external `.skin` companions** (`Boar00/01.skin`). 264 is already outside benilla's `256..=263` gate — the retarget's M2 work is: implement external-`.skin` loading (upstream documents embedded vanilla profiles only — new work, not preserved behavior), add WotLK header/animation blocks per layout, and raise the version gate only per supported layout — never to the corpus max wholesale. Full corpus version census = first parser task (B-004 implementation).
4. **benilla-mpq decompresses this install's WotLK archives correctly** (ADT/M2 bytes verified by structure) — the MPQ reader needs no retarget work for read/list/extract.

## Route dependency list (v1 — closed for the base profile)

- WDT `World\Maps\Azeroth\Azeroth.wdt` (+ wm-hd override via patch-6)
- ADTs 30..33 × 48 (+1 neighbor row/col to be pinned when the exact walk path is fixed)
- Liquid: MH2O (this route)
- Models: player race M2+skins, `Creature\Boar\Boar.m2` + skins (sample verified), route doodads/WMOs (Northshire abbey WMO id pending pinning), BLP textures via MTEX/MMDX references
- DBCs (resolve from locale patches last): Map, AreaTable, Light*, CreatureDisplayInfo(+Extra)/CreatureModelData, ChrRaces/ChrClasses/CharSections, minimal Item display chain, SpellVisual* only when P4 spell is pinned

## Open items carried

- [ ] M2 version census across both profiles (max version, count per version) — first parser ticket
- [ ] Northshire WMO + doodad set pinned to exact internal paths
- [ ] Nonstandard `enus/backup-enus.mpq`, `enus/base-enus.mpq`: excluded from manifests pending empirical load-order verification
- [ ] Chunk byte-order convention surveyed across file families (ADT confirmed reversed here)
