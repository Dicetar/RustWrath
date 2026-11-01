# Argon integration and human handover

Pulled origin/wotlk 27d3b09d. The dirty OpenCode edits in wrath.rs, tooltip_unit.rs and
wrath_natives.rs were backed up and temporarily stashed. Both authors implemented the same
self-SpellGo cast-slot release; the conflict retains one release and OpenCode's regression
case. Tooltip IsUnit and SpellCanTargetItemID edits are preserved in 0b5682bd.

Build integration repairs: export wmo_group_liquid_mesh_for_root from benilla-formats;
expose the read-only rig billboard predicate (5450a700); replace four private get_u32 calls
with typed optional unit-state getters (df769fba). Argon's requested behaviour is retained.
One focused Wrath movement/posture binding case passed. The final
`cargo build -p benilla --features hotpatch` passed; no broad suite was run.

Binary SHA-256: A974EFBEDCFC7C9BB02DD71CCE8ACB052B0BB3681B9919DE51E0757974128ECD.
Code revision df769fba. Live log: connected-run-20261101-042923-519.stderr.log in the
existing ignored verification directory. Entered world 0 as 0x1522 with 306 entities.
Human session PID 38272 started 04:29 Moscow time, with a 20-minute/5-GiB bound.
No automated movement or combat; Astra sends no controls. Do not open a competing session.

The owner should now check Space jump, dry abbey interiors/posture, repeated abilities,
wandering creatures/circles, merchant flow and camera-dependent magenta. These are fresh
code changes in this executable, not an unchanged rerun. Rendered/gameplay acceptance and
the full quest/loot/reward/relog route remain pending.

Backup task `RustWrath source backup every 8 hours` is installed and its real scheduled
invocation returned result 0. Next scheduled start: 12:14:59 Moscow time. Three roughly
79-MiB source/history copies are retained under D:/Projects/RustWrath-backups; no target,
install, credentials or raw run dumps are copied. Bundle integrity and zip/manifest counts
were checked. Source/working-state capture is best-effort, not a filesystem atomic snapshot.
