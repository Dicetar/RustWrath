# 12 — B-009: installed addon compatibility

**What to build:** port selected WotLK FrameXML/Lua behavior so the installed WM addons load and behave (WMBridge, WM_Multiclass, WMQuestieCompat, Spellwright flow first; owner-picked Bagnon/Bartender4/Questie-335 exact builds second). Audits: API returns, event argument order, load-on-demand, SavedVariables, templates, secure/action semantics, reload lifecycle. Opening without errors is not behavior.

**Blocked by:** 09.

**Status:** ready-for-agent

- [ ] Each target addon: loads + one behavior-level check evidenced
- [ ] API-surface gap list vs 3.3.5 maintained (benilla-lua is an in-house interpreter; extending it is our work)
