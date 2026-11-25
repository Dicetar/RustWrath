# ER combat: guard, stamina and the poise bar

Status: ready for a build and a server script reload
Owner: DeepSeek V4.1 Flash (opencode-go)
Branch: main (uncommitted integration tree)
Next: rebuild the client; shield Int still has no spell-block path (see "Blocked")

## What changed

- **Guard, server** (`integrations/ale/rustwrath_souls.lua`): a guarded blow's share that gets
  through now shrinks with the defender's Stamina and armour instead of the old binary
  shield/weapon split. Shield: `0.55*120/(120+Sta) + 0.20*3000/(3000+armour)`, clamped 0.10-0.60;
  weapon: `0.80*120/(120+Sta)`, clamped 0.35-0.80. A `guard_break` blow never drops below 0.40,
  so the adaptation chips through a shield.
- **Guard, client** (`crates/benilla-souls/src/input.rs`): a guarded hit costs
  `25 * kit scale * 120/(120+Stamina)`, so the Stamina stat deepens the bar without changing the
  engine's maximum.
- **Poise bar**: the server now sends `POISE <guid> <cur> <max>` whenever a creature's stance
  moves, refills or breaks; the client parses it and draws a slim pale-blue bar over that
  creature's plate, fresh for 5 s and hidden at full. `HudPart::Poise/PoiseFill`.

## Checks

- `cargo test -p benilla-souls`: 33 passed, 0 failed.
- `cargo fmt --all`: clean.
- ALE deployed to `WM_BridgeLab\run\lua_scripts\rustwrath_souls.lua`; watcher reloaded it
  ("1 compiled") with no Lua error.

## Blocked (and why)

- **Mana ward/spell parry**: the server exposes no player-took-damage hook in this ALE build, so a
  ward cannot absorb server-applied spell damage without a core change.
- **Combo lingering**: the client still does not decode `SMSG_UPDATE_COMBO_POINTS` (the known
  gap), and the server cannot read combo points to apply a lingering crit.
- **Shield Int spell-block**: `Unit:GetStat(3)` is available, but the only damage path routed
  through this script is melee; magic damage never reaches `resolve_strike`.

## Next

- Light/heavy motion-value spread: the kit's hit rows are generated, so this is a kit/importer
  tuning pass, not a code constant.
- Adaptation feedback: the client still ignores `ADAPT`; showing it needs a HUD text element and
  a font handle the HUD does not load today.
