# Integration record, 2026-11-06

Integrated the remaining published Windows release assembly, opt-in multiclass host, mail/action-bar and whois branches onto main at 4a23750c. Reconciled the preserved recorder/pose work and roadmap history with the newer implementations already on main.

Conflict decisions preserve the typed ActionButtonState decoder and ActionButtonsUpdate dispatch already used by main, while incorporating mail composition and inline text. The launcher supports assembled releases, versioned recovery and the optional multiclass glue. Session shutdown retires scoped delivery and suspends multiclass before worker teardown. The obsolete boot-VM helper stays removed.

Older dirty checkouts were committed and pushed as source archives before publication:

- archive/client-local-2026-11-06 retains the old diagnostics checkout's edits, primarily trailing blank lines and earlier cast fixtures, plus the XP notification patch already present in main.
- archive/multiclass-xp-local-2026-11-06 retains the earlier XP patch, already present in main.
- archive/assets-local-2026-11-06 retains older format/profile edits based on the historical asset checkout.
- archive/opencode-local-2026-11-06 retains earlier profile, combat-core and WM bridge source and handoffs.

The archives are preserved source snapshots. Their stale implementations are not substituted wholesale for the later integrated owners. Asset and older combat/WM snapshots still require semantic comparison before any additional behavior can be promoted. Local logs, agent configuration and runtime evidence remain local.

Validation in this integration was source conflict inspection and git whitespace/conflict checks. No build, tests or client sessions were run. Existing historical test results apply to their recorded source revisions; this combined revision still needs compilation and gameplay acceptance.
