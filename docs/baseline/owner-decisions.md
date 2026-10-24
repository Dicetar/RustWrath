# Owner decisions — adopted defaults (vetoable)

Recorded 2026-10-24 after owner granted execution authority ("you can build, code and do anything you need").
These defaults were recommended by GLM, unopposed by Astra, and adopted in the owner's absence of objection.
The owner may override any row; later documents must not treat them as owner-confirmed until then.

| # | Question | Adopted default | Basis |
|---|---|---|---|
| 1 | Maintenance boundary | Routine WM definitions/behaviors update live (proven lane). Rare *new native capabilities* (new C++ primitives, new wire framing) may use a declared, planned maintenance restart; goal is rarity, each preceded by a recorded gap. | GLM recommendation, response 02; Astra flagged as open owner question |
| 2 | Addon compatibility targets | First: installed WM addons (WMBridge, WM_Multiclass, WMQuestieCompat, Spellwright flow). Then: owner to pick exact Bagnon/Bartender4/Questie-335 builds. | Plan rev 3 P5 |
| 3 | First route | Northshire → Elwynn tile boundary (Hogger stretch optional). | GLM R4 default |
| 4 | Souls combat sequencing | Optional parallel work, never a gate prerequisite. | Astra correction, accepted |
| 5 | Stock-client coexistence | Not a product goal. Fallback deployment retained; baseline-protocol invariant maintained as regression check. | Response 01 R8 / plan rev 3 |

## Long-term goals added by owner

| # | Goal | Added | Staged interpretation (proposal, vetoable) |
|---|---|---|---|
| 6 | **Client-side hot-patching** — live client changes without client restart/rebuild, mirroring the server-side live-change rule | 2026-10-24, owner message | **C1:** hot-reload of client data — assets via a file-watch/re-import path on the manifest loader (ticket 05 must leave a seam), tuning/config files, and Lua/XML (the addon host already has a reload lifecycle; stock `/reload` semantics). **C2:** WM-pushed client-side definitions — the native client channel (ticket 11) carries versioned client-content publications (UI tweaks, shell visuals, tuning) to connected clients. **C3, optional/last:** hot-swappable native presentation modules (dylib with stable ABI, swap at state boundaries) — explicitly not promised by the base plan's extension model; only if C1+C2 prove insufficient. |
