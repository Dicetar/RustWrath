# Wrath quest marker refresh after progress

Status: ready to merge
Owner: Codex
Branch: fix/wrath-quest-marker-batch
Issue: none
Next: During Northshire acceptance, check Willem's marker after the required quest item enters the bag.

AzerothCore sends `SMSG_QUESTGIVER_STATUS_MULTIPLE` after quest progress and
item changes. The Wrath decoder ignored it, leaving existing NPC markers stale
until a separate single-giver query. The decoder now validates the batch and
projects its rows through the existing scoped questgiver status owner. Ordinary
per-giver queries still cover objects entering visibility. The existing marker
owner filters non-unit questgivers; game object markers are outside this change.

Evidence: 12340 `CGPlayer_C::OnQuestGiverStatusMultiple` at `0x006D1230` and
AzerothCore `Player::SendQuestGiverStatusMultiple`, called from item and quest
progress handlers. The packet carries a count, then full GUID and u8 status
pairs. Source review and `git diff --check` only; no build, tests or launch.
