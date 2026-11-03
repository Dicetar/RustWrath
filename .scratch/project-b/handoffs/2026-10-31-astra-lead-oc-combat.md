# OC prompt — continue playable combat, Astra leads graphics/modules

Continue your existing playable-combat task in D:/Projects/project-b-client, branch wotlk.
Read AGENTS.md, CONTEXT.md, docs/agents/active-ownership.md and the relevant checkpoints first;
revalidate HEAD, dirty files and live processes. Last observed HEAD was 6c44f83f; do not assume
it is still current. Astra preserves your pending finite-local guard and writeback in pose.rs.

The owner transferred graphics and hotpatch leadership to Astra, including rig scheduling seams.
Do not edit graphics/M2/rig/palette/attachments/shaders/profile code, the new hotpatch crates,
Cargo/lockfile, or app plugin registration. Keep progressing on playable combat and its actual
UI/protocol/state blockers. Do not replace that task with a general API audit.

Use Firna (GUID 0x1522), human warrior, Northshire, same dedicated account. Never use the old mage
Fireball instructions for her. Do not print or modify credentials. No server, DB, WM or install
changes, no bots/GM shortcuts and no automated attacks/kills. The human drives combat.

Immediate source lead if it blocks your route: the connected trace
connected-run-20261031-145356-794.stderr.log records UnitName(target)=Unknown for
0xF130000337001DCD just before GOSSIP_SHOW; screenshots show that live questgiver as Unknown/Dead.
Trace the actual received health/entry/name query, descriptor projection, cache landing and held
unit snapshot. Correct the missing/wrong producer or translation; never force alive health or
invent a name. Claim names.rs, ui_unit/combat/input/target owners and any necessary wrath.rs or
protocol changes before editing. Report graphics-owned dependencies to Astra.

Then follow your existing route toward target -> attack/cast -> authoritative outcome -> visible
UI, quest counters, corpse loot, turn-in/reward and relog persistence. Use the preserved 12340
reference at D:/Projects/wowdecomp-reference, pin e67b6ffef23655dc7da061cb2d36b7963f6ef71a;
record symbol/address and corroborate with selected assets or matching AzerothCore wire source.
Inspect behavior, do not copy decompiled implementation into the project.

The earlier Astra-only Cargo/client-run reservation is released: the owner confirms OC is working
toward playable combat. Revalidate the live client (PID 45472 when last observed); do not stop or
replace an existing session without coordinating its ownership. Do not start a competing login
or build that interferes with that session. Use only
the most necessary focused checks (maximum 20 tests per task), one bounded symptom rerun and
human combat acceptance. No full-suite cycling, duplicate worktrees or target backups.

Commit only your claimed files. Report source changes, compilation, focused checks and actual
connected/rendered evidence separately. A0/A1 and playable alpha remain unproven until the full
route and persistence are observed. Keep the next blocker concrete and evidence-backed.
