# Land the ready queue

Status: landed
Owner: Astra / OpenAI
Branch: astra/overnight-2026-11-19
Issue: none
Next: Owner runs the existing gameplay checklists.

## What changed
Merged all six ready branches in three pairs, preserving their notes and intent.
Talent preview: e373e179; diagnostics: 4691447e.
Spell-slot fallback: 590fc627; chat: 7ee1efa3.
Action bars: d94b5252; alpha documentation: 7b03254c.
Fixed merge formatting, static macro usage lifetimes, Wrath-only locale metadata,
and two UI test-contract failures; repairs through 33dc2bc7.
Preserved the concurrent HD-rig, rooting and nameplate-bar changes from main.
Existing notes are marked landed; landed remote task branches are retired.

## CI run id and result
44432140749: Windows workspace check and app tests pass.
44433243628: Windows check and protocol-wrath tests pass.
44434664718: Linux UI/protocol-wrath tests pass (Windows still running).
44432259456: Linux app/formats tests pass; its two UI failures are fixed by 33dc2bc7.
Player-clippy remains red on untouched combat-core lint debt.

## What is not proven in game
Talent learning, pet learning, chat, paging/macros, vehicle/possess UI and the
Northshire route were not played. No local Cargo, client or server was started.
