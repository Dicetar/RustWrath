# CVar backend review — Sound_EnableMusic and textureFilteringMode

Read-only review requested by `.scratch/project-b/handoffs/2026-11-02-deepseek-support.md`.
No Cargo, test run, client launch, commit or production edit. The two names are the newest
sound/video/CVar edits (also recorded in `verification/2026-11-02-opencode-batch.md` and
AutoSwarm call `call_5380cc6ac3844b74a45cdf5465d64056`). Wrath behavior taken from
`D:/Projects/wowdecomp-reference` at `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`; read for
behavior only, no code copied.

## What was checked

- Registration: name, default, target, latched/checked flags, reader.
- Initialization: fresh install (no file), saved Wrath value, saved donor spelling.
- Change application: the observer/knob the value reaches, and whether the reference applies
  live or at restart.
- Donor compatibility: whether the 1.12 spelling/behavior changed.
- Persisted interaction: `benilla-config/config.toml` fold, default elision, alias precedence.
- Reference anchors: `Client.cpp:247-250`, `:1108`, `:1124-1126`;
  `WorldParam_CVar.cpp:114-126`; `SoundInterface2_CVar.cpp:176-192`, `:225-240`, `:248-252`.

## Source locations

| Item | Location |
|---|---|
| `Sound_EnableMusic` row, default `"1"` | `crates/benilla-app/src/cvars/table.rs:279` |
| donor `EnableMusic` row, default `"1"` | `crates/benilla-app/src/cvars/table.rs:212` |
| `Sound_EnableSFX` / `Sound_EnableAllSound` rows | `crates/benilla-app/src/cvars/table.rs:273-274` |
| `textureFilteringMode` row, default `"1"` | `crates/benilla-app/src/cvars/table.rs:294` |
| donor `trilinear` / `anisotropic` rows | `crates/benilla-app/src/cvars/table.rs:752-762` |
| sound observer | `crates/benilla-app/src/sound/mod.rs:326-350` (music alias `:337`) |
| music gate | `crates/benilla-app/src/sound/mod.rs:141-153` (`category_amp`) |
| video observer | `crates/benilla-app/src/video.rs:317-398` (mode arm `:391-398`) |
| texture policy | `crates/benilla-assets/src/tex_filter.rs:39-103`; publish at `crates/benilla-app/src/cvars/mod.rs:1227-1240` |
| boot fold / observers | `crates/benilla-app/src/cvars/mod.rs:949-992`; `load_file` `:570-612`; `compose` `:717-745` |

## Verified correct

1. **Fresh install.** `Sound_EnableMusic` registers `"1"` and `SoundConfig` starts
   `music_enabled: true` (`sound/mod.rs:165`); `textureFilteringMode` registers `"1"` and
   `TexFilterSetting::default()` is `trilinear: true, aniso: 1` = the same mode
   (`tex_filter.rs:47-65`). No file means no `CvarChanged`, and the resources already stand at
   the reference default. Correct.
2. **Saved Wrath value applies at boot.** `load_config` (Startup, `CvarLoad`) folds the file and
   triggers the collected events before anything after `CvarLoad` runs (`cvars/mod.rs:949-992`),
   and `publish_filter_policy` is chained after `load_config` in the same set
   (`cvars/mod.rs:855-860`). So a saved mode is in `TexFilterSetting` before the sampler policy is
   frozen. Correct.
3. **Mapping.** 0→bilinear (`trilinear=false, aniso=1`), 1→trilinear, 2..5→aniso 2/4/8/16 matches
   `s_textureFilterModes`/`s_textureMaxAnisotropy` (`Client.cpp:247-250`) and the reference's
   one-time application at Client init (`Client.cpp:1124-1126`). The arm's `1 << (n-1)` produces
   exactly 2/4/8/16 and stays inside `ANISO_RANGE` (`tex_filter.rs:34`). Correct.
4. **No live filter swap.** `textureFilteringMode` intentionally only writes the knob; the policy
   publishes at `CvarLoad`. The reference also applies at Client init only, not in the callback
   (`WorldParam_CVar.cpp:114-126` returns true after validating). Correct and consistent with the
   panel's restart-required semantics. (The donor `trilinear`/`anisotropic` arms have the same
   behavior; `video.rs:374-376` says so.)
5. **Music reaches a real consumer.** `sound::on_cvar:337` writes `SoundConfig::music_enabled`,
   which `category_amp` uses to gate the Music category (`sound/mod.rs:149`). This is a real
   backend gate, not a registered-only row. The value/default come from the reference register
   site (`SoundInterface2_CVar.cpp:251`, default `"1"`).
6. **Donor rows/defaults untouched.** The change adds `ours(...)` rows and one observer arm; it does
   not edit `EnableMusic`, `EnableAmbience`, `SoundReverb`, `SoundZoneMusicNoDelay` or their
   defaults. `compose` still preserves unknown keys and removes only values equal to their own
   default (`cvars/mod.rs:717-745`).

## Concrete issues

### I1 (High) — `Sound_EnableMusic` cannot durably override a persisted donor `EnableMusic`

Both rows are live on the same registry (there is no target filter in `all_rows()`,
`table.rs:171-173`), and the observer maps both spellings to one knob (`sound/mod.rs:337`).
Neither write mirrors the other; the new test even asserts the donor row is left at `"0"`
after `Sound_EnableMusic` is set to `"1"` (`cvars/mod.rs:1744-1748`).

Failure sequence on a Wrath launch with a 1.12-era config:

1. `config.toml` holds `EnableMusic = "0"` (off its `"1"` default, so the donor save kept it).
2. `load_file` fires `CvarChanged(EnableMusic, 1→0)` → `music_enabled = false`. There is no
   `Sound_EnableMusic` entry (its default is `"1"`), so no second event.
3. The Wrath Audio panel reads `Sound_EnableMusic = "1"` and shows music checked, while the mixer
   is muted — the checkbox and the sound disagree.
4. The owner unchecks/rechecks it: `SetCVar("Sound_EnableMusic","1")` fires and audio returns for
   the session, but `compose` drops `"1"` because it equals that row's default
   (`cvars/mod.rs:736-739`). `EnableMusic = "0"` remains in the file.
5. Next launch repeats step 2: music is muted again, panel still checked.

The same shape exists for every aliased volume row (`Sound_MusicVolume` vs `MusicVolume`, etc.;
`table.rs:531-535`, `sound/mod.rs:330-332`): setting the Wrath slider back to the Wrath default
is not persisted, and a stale donor value wins on the next boot while the panel displays the
Wrath default.

Latent today: the current `benilla-config/config.toml` has only the `Sound_*` spellings and no
donor `EnableMusic`/`MusicVolume`, so this does not reproduce with the present file. It
reproduces for any player carrying a 1.12 config, which is exactly the donor-compatibility case.

**Minimal repair (one rule, not a refactor).** On the Wrath target, keep the two rows equal:
in the sound observer, after applying an arm, mirror the paired spelling through
`Cvars::mirror` (it moves the row, persists it and reaches the VM without a second event — the
existing mechanism for `WorldDetail`/`frillDensity`, `cvars/mod.rs:466-484`). That requires
`mut cvars: ResMut<crate::cvars::Cvars>` on the observer; mirror only when
`crate::wrath::selected_from_env()` so a 1.12 launch keeps donor-only behavior. The same applies
to the new video alias (I2). A registry-level alias table would be cleaner long-term but is a
larger change; the observer mirror is the bounded repair.

**Test to add.** Boot a registry from `EnableMusic = "0"`, run `load_config`, assert
`music_enabled == false`; then apply `Sound_EnableMusic = "1"`, save, reload, and assert music
stays enabled. That is the failure the current tests do not cover.

### I2 (High) — a persisted donor filter row overrides `textureFilteringMode` at boot

`load_file` iterates the `BTreeMap` in key order (`cvars/mod.rs:570-612`). With the registered
spellings, `textureFilteringMode` (`t-e…`) sorts **before** `trilinear` (`t-r…`). So a saved
`trilinear = "0"` is applied after the Wrath mode. Sequence:

1. `config.toml`: `trilinear = "0"` (off its `"1"` default), no `textureFilteringMode`.
2. Boot: the `trilinear` event sets `tex_filter.trilinear = false`; the Wrath row never fires.
3. `GetCVar("textureFilteringMode")` answers `"1"` (Trilinear) and the Wrath dropdown shows
   Trilinear; `publish_filter_policy` logs mode 3 (bilinear + nearest mip). The setting lies.
4. `trilinear = "1"` plus saved `textureFilteringMode = "0"` inverts it the other way: the
   dropdown shows Bilinear while mode 4 (trilinear) is published.

`anisotropic` sorts before the Wrath row and is therefore overridden by it, so the asymmetry is
specific to `trilinear` (and to a live `SetCVar("trilinear", …)` / `SetCVar("anisotropic", …)`,
which also wins after boot).

**Minimal repair.** On the Wrath target, make the Wrath spelling authoritative when the file
provides it, and otherwise seed it from the donor pair so the panel tells the truth. The bounded
place is a Wrath-only normalization after `load_config` and before `publish_filter_policy`
(e.g. a new system in the `CvarLoad` chain gated by `wrath::selected_from_env()`): if the
`textureFilteringMode` row's `reset` is `Some` (the file had the key), recompute
`TexFilterSetting` from it; else write the mode equivalent of `trilinear`/`anisotropic` into the
row with `Cvars::mirror` so `GetCVar` matches the policy. Do not reorder `load_file` globally —
that would change 1.12 fold order.

**Test to add.** Two boot cases with both keys present: Wrath mode 0 + `trilinear "1"` must
publish mode 3 and answer `"0"`; Wrath mode 1 + `trilinear "0"` must publish mode 4 and answer
`"1"`.

### I3 (Medium) — `textureFilteringMode` accepts writes the reference refuses

Reference callback (`WorldParam_CVar.cpp:114-126`): `SStrToInt(value)`, accept only
`0 <= mode < 6`; otherwise print `"Texture filtering mode must be in range 0 to 6."` (the
reference's own `%d`/6 text bug) and return false, so the old value stands. benilla's row has no
`CvarCheck` (`table.rs:294`) and the arm clamps (`video.rs:392`):

- `SetCVar("textureFilteringMode","9")` stores `"9"` and applies 16× (reference: refused, old
  value kept).
- `SetCVar("textureFilteringMode","-1")` stores `"-1"` and applies bilinear (reference: refused).
- `"2.9"` is equivalent in both (truncates to 2), and non-numeric text is 0 in both.

**Minimal repair.** Give the row `.checked(...)` with a small checker alongside `check_farclip`
(same `s_str_to_int` parse, accept `0..=5`, emit the range line on refusal). Keep the arm's clamp
as the defensive path for a hand-edited file, whose stored value is kept but whose observer does
not fire under `load_file`'s refusal rule (`cvars/mod.rs:581-604`). Document that the reference
would index its six-entry table with the kept value at Client init (undefined); benilla does not
ape that, so the file-loaded out-of-range value shows the default filter. That is a deliberate,
safe deviation and should be one line in the row comment.

### I4 (Medium) — the Wrath master/SFX/emote rows reach no consumer, and music ignores master

`Sound_EnableAllSound` and `Sound_EnableSFX` are registered (`table.rs:273-274`) but
`sound::on_cvar` has no arm for either. The reference:

- `EnableAllSoundHandler` (`SoundInterface2_CVar.cpp:176-192`) mutes the SFX, MUSIC and AMBIENCE
  groups when any of the three is 0, and all three when the master is 0.
- `EnableMusicHandler` (`:225-240`) computes the music mute as
  `!SStrToInt(value) || !enableAllSound->m_intValue || CGGameUI::m_inMovie`, then stops all music
  streams when muted.

benilla's music gate only consults `music_enabled` (`sound/mod.rs:149`); `enabled` is written only
by the donor `mastersoundeffects` (`:333`). Net effect: in the Wrath panel, unchecking
"Enable All Sound" leaves music and SFX playing, and the music checkbox cannot reflect the master
state. `Sound_EnableEmoteSounds` is similarly inert (only donor `emotesounds` is read, `:346`).
The in-game movie case is already covered by `music_suppressed`, so it is not part of this gap.

**Minimal repair.** Add `"sound_enableallsound" => sound.enabled = v != 0.0` beside the donor
arm. `Sound_EnableSFX` needs a new `sfx_enabled` field (or reuse of `enabled` is wrong: the
reference has independent SFX and master groups), so either add the field and gate the SFX
category or state it as unsupported in the row's `why`. `Sound_EnableEmoteSounds` is one alias arm
(`"sound_enableemotesounds" => sound.emote_sounds = …`). These are bounded; no panel rewrite.

### I5 (Medium) — `$WOW_TRILINEAR`/`$WOW_ANISO` no longer win over a saved Wrath mode

`TexFilterSetting::default()` documents the env levers as session-only overrides
(`tex_filter.rs:48-63`), and `session_values` marks only `trilinear` and `anisotropic` as
session-owned (`cvars/mod.rs:909-915`). `load_file` skips session-owned rows (`:577-580`) but
still applies a saved non-default `textureFilteringMode`, whose event overwrites the env-derived
setting. An A/B run with `WOW_ANISO=8` and a config holding `textureFilteringMode = "2"` therefore
runs 2×, not 8×, and is not hermetically controlled. (A saved `"1"` is the row's default and is
elided from the file; a hand-edited `"1"` fires nothing because the value already matches. The
current config has no such key, so this is latent.)

**Minimal repair.** In `session_values`, when either lever is set, also
`own_for_session("textureFilteringMode", Some(derived_mode))` — derived from the same
`TexFilterSetting` the lever seeded, as `uiScale`/`farclip` are marked with their knob value.
The file entry stays untouched.

### I6 (Low) — Wrath-only names are observable/active on the donor target

`on_cvar`'s new arms (`sound_enablemusic`, `sound_enableambience`,
`sound_enablereverb`, `sound_listeneratcharacter`, `sound_zonemusicnodelay`,
`texturefilteringmode`) run on every target. The rows are also registered on the donor target
because `all_rows()` has no target filter. A 1.12 launch that writes a Wrath name (console/addon)
changes donor playback, which the fork's "preserve donor 1.12 behavior behind its version
boundary" rule does not intend. Nothing in stock 1.12 writes these names, so impact is low.

**Minimal repair.** Gate the new alias arms on `crate::wrath::selected_from_env()` (the same
switch `selected_ui_target` uses, `ui_script/lifecycle.rs:21-30`). Do not remove the rows from the
shared registry here; that is a separate, larger table-targeting decision and would touch donor
behavior.

### I7 (Low, documentation) — the Wrath handler's stream-stop behavior is not recorded at the alias

The donor field comment (`sound/mod.rs:81-84`) documents benilla's deviation: it holds the track
at zero and resumes mid-track, where 1.12's `EnableMusic` callback stops and re-picks. The 3.3.5
handler has the same stop behavior plus `StopAllMusic`/`StopAIMusic`/`StopLuaMusic`
(`SoundInterface2_CVar.cpp:232-236`). The new alias comment (`sound/mod.rs:334-336`) only says the
names alias. Under METHOD, the inherited deviation should be named where the Wrath spelling lives,
so a later reader does not treat the Wrath behavior as fully replicated. One comment line; no
code change.

## Test coverage gaps (for the owner to schedule, not run here)

- Boot-from-file for `Sound_EnableMusic` and the volume aliases; the existing case uses `apply`
  (`cvars/mod.rs:1741-1748`), not `load_config`.
- Refusal of `textureFilteringMode` outside 0..5 (I3).
- Boot precedence between `textureFilteringMode` and the donor pair (I2).
- Env lever precedence over a saved Wrath mode (I5).
- No test currently shows the Wrath master/SFX aliases reaching `SoundConfig` (I4).

## Evidence limits

- No build, test, client launch or live listen/render was performed. Runtime status for these
  values comes from the completion matrix: `15CC91…` loads both Options FrameXML files, but no
  owner observation of an opened panel or a toggled music/texture setting is cited, and 15CC91
  has no retained build log and predates the current `wrath.rs`.
- "The row exists" and "a unit test applies it" are source evidence only; the reviewed code does
  reach real consumers for music and the sampler policy, but that was judged by reading the
  consumer, not by observing it.
