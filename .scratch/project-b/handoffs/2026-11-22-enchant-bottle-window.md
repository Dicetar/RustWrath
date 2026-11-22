# Enchant Bottle: a real window instead of the gossip menu

Status: landed
Owner: Claude Code / Opus 5.5
Branch: claude/enchant-bottle-window
Issue: none
Next: land on main, copy the server script to run\lua_scripts, owner retests in game

## Owner report (2026-11-22)

Using the bottle opened the gossip window ("Charges: 1. Stored enchant: +36 Attack Power (T4)",
then one line per equipped slot) and drew a green ground-target circle in the world. The owner
sketched the window they want: Options on the left, "Drop item here" with the selected option's
effect and Cancel / Apply in the centre, the bags on the right.

## Why the old panel never showed

`EnchantBottle.xml` (7605dca29, 23200dc09) opened its panel by replacing the Lua globals
`UseContainerItem` and `UseAction`. The owner's use still reached the native item use: the
interaction diagnostics show spell 61050 (the stock item 44625's ground-targeted "set out a
bottle") requested. That spell is the green circle: the client entered ground targeting, the
placement click sent `CMSG_USE_ITEM`, and the server's item-gossip hook answered with the menu.
The exact path that bypassed the Lua wrappers (Bagnon, Bartender4, a keybind) was not isolated;
the fix no longer depends on it.

## What exists now

- Client, `crates/benilla-app/src/ui_items/mod.rs`: `send_item_use`, the one function every item
  use passes through (bags, bag addons, the doll, action bars, keybinds, macros), returns early
  for item 44625 / 800000 and calls `RustWrathBottle_Toggle()` in the UI VM. No cast, no
  targeting circle, nothing sent.
- Client, `crates/benilla-app/assets/ui/EnchantBottle.xml`: rewritten to the owner's sketch.
  Left: six option rows (Reroll, Gamble, Empower, Awaken, Pour stored enchant, Absorb) with
  icon, name and the cost for the item in the slot, greyed when they do not apply. Centre: the
  item slot (drag from the bags, a bag addon or the character frame, or click an item in the
  grid; right-click empties it), the item's three enchant slots (the option's target slot is
  chosen there), the effect text, Cancel / Apply, and the server's result. Right: the bags as an
  8-wide grid (mouse wheel scrolls), a Worn tab for the gear; non-weapon/armor items are dimmed.
  Absorb asks for confirmation. The Lua `UseContainerItem` / `UseAction` replacements are gone.
- Server, `integrations/ale/rustwrath_enchant_bottle.lua`: new window protocol (HELLO, OPEN,
  PICK, APPLY in; SHOW, STATE, ITEM, RESULT, MSG out; written out in the script). Actions work on
  a position (worn gear, backpack, equipped bags), so bag items can be rerolled etc. too. A
  client that said HELLO gets SHOW instead of the gossip menu on item use and `.bottle`; stock
  clients keep the gossip menu. `act_on` returns its line instead of saying it; Pour restores the
  stored enchant when the enchant does not take.

## Checks

- Lua syntax: both chunks (the window's script and the server script) compile in the Wrath VM
  (a throwaway benilla-ui test, run once and not committed): pass.
- `cargo clippy -p benilla-app` in this worktree's own target dir: no error, no new lint.
- rustfmt on `ui_items/mod.rs`: clean.
- Not run: the client, the window in game, the server round trip. CI: not run yet.

## Not done / not proven

- Whether the owner's Bagnon / Bartender path reaches `send_item_use` with the entry set; it is
  the documented single funnel (`0x5d8d00`), but not observed in game.
- The live-code client needs a restart? No new system, resource or struct: the Rust change is a
  branch in an existing function and the XML is read from `assets/ui` in a dev build, so dx
  patch + `/reload` should do. If the window does not appear after `/reload`, restart the client.
