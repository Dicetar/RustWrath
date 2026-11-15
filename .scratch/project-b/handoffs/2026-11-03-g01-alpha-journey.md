# G01 — alpha journey evidence map

Inspected 2026-11-03 at `wotlk@236b1a7aebc017655f9a0fc0dea54e52dae891ef`
plus existing dirty source. No client launched or gameplay automated. Current binary still
has SHA256 `1D853A073F916C1CBD952FA21E36ACB6BA960C29486B919CD2F6A7A13B3B43BC`;
it predates this batch. No current benilla/cargo/rustc process observed at admission.

Historical sources: November 2 completion matrix and Astra/DeepSeek live handoff. Retained
`benilla-config/Logs/Launcher/20261102-222346-059/stderr.log:28` directly records world entry.
Lines 100–101 directly record `WatchFrame.lua:1306` errors. These are historical log facts,
not a new runtime run or proof about the changed source.

| Milestone | Evidence classification | Existing owner / next boundary |
|---|---|---|
| Login controls and authentication | Historical visible report and world-entry prerequisite | glue_session, net/session, wrath login admission |
| Explicit realm selection UI | Source-only controls; historical realm connection report, selection interaction unverified | glue_session/realm, ui_script/glue_mgr, WrathParkedCommand::Realm |
| Roster and selection | Historical roster data reports; switching behavior unknown | glue roster and Wrath parked session |
| Character create/cancel | Source-only; interaction unknown | glue_create and CharRequest; owner controls persistent changes |
| Enter world as Firna | Historical visible report; retained latest log confirms world entry | Wrath session/reducer → view → loading/player owners |
| NPC interaction / greeting | Historical visible report for McBride on an older build | click action → GossipHello/QuestgiverHello → ui_quest/net |
| Quest row → detail | Unverified after dirty instrumentation; historical dead-row symptom | QuestVerbs::select → tap → query packet → detail decoder → on_detail |
| Accept → active journal slot | Source-only, no end-to-end acceptance evidence | QuestVerbs::accept → delivery receipt → server quest fields → QuestLog |
| Combat | Historical owner-driven visible report in Northshire | action/cast and reducer owners; current movement repair acceptance unknown |
| Loot and inventory | Historical partial reports; current route unknown | loot messages → inventory/item owners |
| Objective progression | Source-only; historical WatchFrame error is a known presentation symptom | quest event/slot counters → quest log/watch frame |
| Turn-in and rewards | Source-only for full route | completion plus separate XP/money/item deltas; no reward inferred from panel close |

The first unverified screen interaction in sequence is explicit realm selection; the first
unverified gameplay interaction after NPC greeting is quest row → detail. Do not skip these
when claiming a complete route. G05 addresses a separately proven source defect: channel
send success previously concealed refusal by the 256-entry outbox. It is not established as
the cause of the historical McBride row symptom or WatchFrame error.

## Minimal next reproduction (not executed)

After an explicitly authorized build/session, use the existing launcher with Wrath login UI
and the chosen read-only asset profile; record executable hash, PID, profile, endpoints and
server revision. Owner enters credentials, chooses realm/roster, and verifies Firna in
Northshire. Open McBride, choose one available quest row, record quest/NPC and the last
confirmed boundary: select log, outbox, socket write, detail decode, panel event. Existing
select instrumentation is debug-level; launch configuration must enable it deliberately.

For acceptance, click Accept once and distinguish ChannelAdmitted, OutboxAdmitted and Written
from authoritative quest-slot/journal evidence. Stop at the first missing boundary. Do not
use a timeout alone to blame the server or run combat/character changes unattended.

Deliverable: evidence map and narrow source blocker identified. No new B/T/R/V evidence;
the playable route remains unaccepted.
