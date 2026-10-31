# Owner steering: Firna is the alpha acceptance character

The owner created a new human character **Firna**, on the same dedicated account, and explicitly
instructed Astra to use her for testing and pass this to OC. Use Firna for the Northshire alpha
route. Do not use or reset Dicetar's existing camp quests. Do not change credentials or expose
the identity file. Verify the selected roster name before any world actions.

Both tracks share one account: inspect active client/build processes before launching. Do not
disconnect or terminate another owner's session. Astra owns spellbook/UI/gameplay integration;
OC owns quest markers and graphics. Astra's current source claims are spellbook.rs in benilla-ui
and wrath_ui_slice_tests.rs in benilla-app. She is verifying the six-return Wrath spell-tab API
and highest-rank slot mapping, with source reference and evidence in the UI-host checkpoint.

Astra also owns wrath.rs boot-name admission: old binaries ignore WOW_CHAR and choose the
first record. Wait for the fixed binary before using Firna. The existing identity wrapper now
accepts `-CharacterName Firna` to override only the character for that run; credentials and the
identity file remain unchanged. This still uses the real returned roster, not a fabricated GUID.

This file communicates the owner's instruction; it does not prove Firna was selected, entered
Northshire or completed any acceptance step. Combat remains human-driven.

Gameplay update: Astra also repaired the Wrath quest-log title ABI (suggested group in return
4, completion in 7, daily/id tail), preserving suggested_players from the actual query packet.
Claims additionally cover benilla-ui/script/quest_log.rs, benilla-app/ui_quest_log.rs, shared
protocol messages/quest/log.rs and Wrath gameplay.rs; only mechanical fixture fields in
capture/fixtures.rs and ui_quest/lines.rs. No graphics source was edited.

Verification: 88 spellbook/quest-log UI tests pass and the nonzero suggested-player wire case
passes. Source is frozen for client build/preflight. Named-character unit passed; final live
Firna entry remains pending. Avoid launching a competing client/account session. Human owner
is available for the combat leg once Astra reports the preflight ready.

Live update: Firna is confirmed race 1/class 1 (human warrior), GUID 0x1522, level 1 in Northshire
Valley. The second bounded run connected-run-20261031-134958-868 confirms a populated stock
spellbook in astra-firna-20261031-134958.png; 89 focused tests and client build pass. Peak private
3.48 GiB; normal deadline exit. No owned client remains running. OC marker commit 36fa08f7 is
preserved as source parent. Combat/quest acceptance is still pending: Astra found the next
quest-detail/reward native API gaps and is closing their real wire-to-state consumption next.
