# OpenCode checkpoint - pinned route asset manifest (O5)

Date: 2026-10-26 (corrected). Owner: OpenCode (client/gameplay integration lead)
Base HEAD: `23ec4589` (Astra's O5 source review; see
`.scratch/project-b/checkpoints/2026-10-26-astra-o5-source-review.md`)
Work package: `.scratch/project-b/work-packages/2026-10-26-opencode-client.md`, stage O5.
Classification: **static preparation only — not completed route integration**. The graphical
and live gates (G/L) remain unauthorized and unrun.

## Correction: the WMO offset claim is retracted

The earlier version of this checkpoint claimed a 12340 shift of the WMO group header (flags
0x08 -> 0x10, groupLiquid 0x34 -> 0x3C). That was wrong: it compared extractor reads made from
the whole-chunk cursor (after the chunk's 4-byte FourCC + 4-byte size prefix) with the Rust
parsers' payload-relative offsets. Astra's review identified the arithmetic error; both existing
parsers (`benilla-wmo::parse_group`, `benilla-formats::models::wmo::group::wmo_group_header`)
read payload offsets and are consistent with the extractor.

Asset-level check with one consistent origin (read-only, from the active profile):
`WORLD\WMO\AZEROTH\BUILDINGS\NSABBEY\NSABBEY_000.WMO` (root `NSABBEY.WMO`, MVER 17, MOHD 30
textures / 14 groups) has its MOGP payload at the chunk bytes +8; payload+0x08 reads
`0x00002a05` (flags), payload+0x34 reads `0` (groupLiquid), and the first sub-chunk after
payload+68 is `MOPY` with a plausible size. All 14 group files `NSABBEY_000..013.WMO` exist in
`common-2.mpq` with MVER 17. No parser or version branch was or should be added on this basis.
This still does not prove every WMO/asset is compatible; a real defect needs asset evidence at
one origin.

## Install and profile identities

- Client install inspected read-only: `D:/WOW/world of warcraft 3.3.5a hd/data` (build-12340 MPQ
  set; `data/enus/` locale archives; `patch-5/j/u.mpq` owner-disabled).
- `base.mpqlist` SHA-256 `00c7357f2a5599b05ea7b97e4b3529198deb17a458b47199f6567259a1bde1cf`
  (stock `common`, `common-2`, `expansion`, `lichking`, `patch`, `patch-2`, `patch-3` plus the
  six `enus` locale/speech/expansion/lichking/patch-enus{,-2,-3}` archives).
- `wm-hd.mpqlist` SHA-256 `439bdc21d09f5509073cd88cc341adb2eb05854b425f4d9dec1d2bcee5083d59`
  (base plus `patch-6/7/8/S/a/b/c/e/f/k/w/y/z` and `enus/patch-enus-{a,c,f,k,m,q,w}`; a
  manifest entry whose archive is absent or `.disabled` is a hard error, not a silent fallback).

## Pinned route dependencies (from the active profile's bytes, read-only)

- Map 0 = `Azeroth` (`Map.dbc`). Willem `(-8947.64, -132.319, 83.7199)` and Garrick
  `(-9056.48, -460.903, 72.8318)` both fall in ADT tile `(32, 48)`; the central terrain file is
  `World\Maps\Azeroth\Azeroth_32_48.adt`, present in `common-2.mpq` and superseded by
  `patch.mpq` (876,720 compressed / 1,824,447 bytes, chunks MVER/MHDR/MCIN/MCNK/MH2O/MDDF/MODF/
  MWMO/MWID/MMDX/MMID/MTEX). Neighbours 31-33 x 47-49 are the streamer's 3x3 readiness ring;
  `World\Maps\Azeroth\Azeroth.wdt` gates which exist.
- WMO paths used by the tile (`MWMO`, exact internal names): `WORLD\WMO\AZEROTH\BUILDINGS\
  NSABBEY\NSABBEY.WMO` (abbey, 14 groups `NSABBEY_000.WMO`..`NSABBEY_013.WMO`),
  `WORLD\WMO\AZEROTH\COLLIDABLE DOODADS\ELWYNN\ABBEYGATE02\ABBEYGATE02.WMO`,
  `...\ABBEYGATE\ABBEYGATE01.WMO`, `...\WIDEBRIDGE\ELWYNNWIDEBRIDGE.WMO`. The tile also carries
  `STORMWIND.WMO`, `REDRIDGE_STABLE.WMO` and two mine WMOs, and only the MODF-referenced ones load.
- Abbey interior doodads (`NSABBEY.WMO` MODN, `.MDX` refs): `WORLD\GENERIC\HUMAN\PASSIVE
  DOODADS\BOOKSTACKS\GENERALBOOKSTACKTALL01.MDX`, `...\STATUES\NORTHSHIREABBEYBUST01.MDX`,
  `WORLD\GENERIC\PASSIVEDOODADS\LIGHTS\CANDELABRATALLWALL01.MDX`, `...\DRAPERY\DRAPERY01.MDX`,
  `...\HANGINGLANTERNS\GENERALHANGINGLANTERN01.MDX` (first five of thirty textures).
- Route environment doodads (`MDDF`, exact `.M2` paths): `WORLD\AZEROTH\ELWYNN\
  PASSIVEDOODADS\TREES\ELWYNNTREE01\ELWYNNPINE01.M2`, `...\TREES\ELWYNNTREEMID01.M2`,
  `...\ELWYNNTREECANOPY01.M2`, `...\CLIFFROCKS\ELWYNNCLIFFROCK01.M2`, `...\ELWYNNFENCES\
  ELWYNNSTONEFENCE.M2`, `...\ELWYNNWOODFENCE01.M2`, `...\LAMPPOST\LAMPPOST.M2`,
  `...\DETAIL\ELWYNNVINEYARD\ELWYNNVINEYARD01.M2`, `...\WATERBASIN\WATERBASIN.M2`,
  `...\WATERFALL\ELWYNNTALLWATERFALL01.M2`, `WORLD\GENERIC\BUILDINGS\HUMANTENTLARGE\
  HUMANTENTLARGE.M2`.
- Creature displays: Willem display 2072, Garrick display 2073 -> `CreatureModelData` 49 ->
  `Character\Human\Male\HumanMale.mdx` (`.m2` + external `HumanMale00.skin`), no texture
  variations. The `.mdx`/`.mdl`->`.m2` rule and external-skin load already exist, as does M2
  version 264. These pins do **not** identify the selected player; the Auto boot still enters
  the first enumerated character, so the gate must record the actual character identity.
- Items: 182 display 7038 -> icon `INV_Misc_Head_Human_01`; 6076 display 16843 -> icon
  `INV_Pants_11` (`Interface/Icons/...blp` through the same profile). Worn reward appearance is
  outside a narrow receipt proof but remains required replacement behavior.
- Fireball 133 -> `SpellVisual` 67 -> kits and their effect textures (read from
  SpellVisualKit/ModelAttach/EffectName): precast kit 30 -> effect 287 `Spells\Fire_Precast_Hand.mdx`
  (SoundKit 702); cast kit 38 -> effect 288 `Spells\Fire_Cast_Hand.mdx` (SoundKit 1484); impact
  kit 286 -> effect 321 `Spells\MoltenBlast_Impact_Chest.mdx` (SoundKit 1507).

## Pipeline stages verified in source (unchanged)

Profile/manifest reader; `.mdx`->`.m2` and external `.skin`; M2 v264; ADT `MCNK` 12340 offsets
and `MH2O`; WDT gating; `<stem>_NNN.wmo` group loading; terrain and WMO collision accumulation;
DBC catalogs from the active profile.

## Retained open items

- Route integration itself (camera, collision, actual streamed scene) is not proven; G/L remain
  unauthorized and UNKNOWN.
- Movement flags and command admission are being repaired in the source slice after this
  document (falling/root/transport bits; movement must never evict critical commands).
- Aster's inventory reconciliation (`23ec4589`) governs behavior coverage; this document only
  pins assets.

## Evidence level

All facts above are source/asset reads. No build, test, linter, formatter, archive extraction to
disk, renderer, socket, server, WM/receiver or live acceptance ran. Any full-route acceptance
still needs the authorized gate and the run card.
