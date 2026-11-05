# Independent review of pending U1 combat-log delivery

Owner requested careful assistance on 2026-11-06. Reviewed the dirty integration/autoswarm-waves-2026-11-05 checkout at 5e02bf8f. This review does not establish compilation or runtime acceptance. Cargo and OpenCode were active, so shared implementation files were left with their current owner. AutoSwarm rejected direct coordination with controller main_e71b37115de208ae6ff9c9e13e59dc41 as peer_not_allowed.

## Required corrections before closing U1

1. **P1 incorrect source attribution.** In ui_script/wrath_combat_log_feed.rs::feed, player_name is always resolved from UnitName("player") or world.player_guid, then passed as the source name alongside kill.killer. SMSG_PARTYKILLLOG supplies a killer GUID which can identify another party member. Resolve the source name for kill.killer. A self-only bridge may filter to killer == player_guid, but the general combat log must preserve the actual source identity. Inspect installed bridge consumers before choosing the filter. Check a teammate kill as well as a self kill. Never emit a teammate GUID paired with the local player's name.

2. **P1 overwritten discrete events.** WrathWorldView::apply_party_kill replaces one Option<WrathPartyKill>. Multiple packets can be applied before UiFeed, so only the last is delivered. A kill is an ordered event, not a replaceable snapshot. Retain a bounded scoped sequence until delivery and expose any capacity loss. Clear pending events on scope retirement and define how an attached/rebuilt VM establishes its delivery cursor. Check two kills arriving before a single feed.

3. **P1 name-resolution loss.** CombatLogMemo.revision advances before either endpoint name resolves. Returning on a missing name permanently skips the event, and Ask::Read alone does not establish that a query will ever run. Preserve pending delivery for a bounded lookup/retry period using the existing name-query owner. Advance the delivery cursor after posting or an explicit terminal discard with recorded reason. Check an unknown creature template and a party member whose name arrives later.

4. **P2 first-feed loss.** On VM change, the memo initializes to world.party_kill's latest revision before comparing. A kill between VM creation and its first feed is skipped. Establish a scope/VM baseline at attachment, with an explicit reload replay policy. Check first live kill and reload without replaying an already delivered kill.

5. **Acceptance coverage.** The argument-shape case calls combat_log_args directly, bypassing feed, cursor, name lookup and scope handling. It cannot establish the four behaviors above. When verification is authorized, use the actual feed path for teammate/self, burst, late-name, first-feed and scope-retirement scenarios. These are distinct outcomes; avoid expanding into an unrelated combat-log rewrite.

## Mechanical status

An earlier app-test log showed assert_eq on Vec<SessionEvent>, whose element type lacks PartialEq. The file/log were changing during review. Recheck the final diagnostic and use a variant/field match if still needed. Do not infer the current test result from the earlier error. Existing build and test slots remain with the active coordinator.
