# OpenCode lead adoption — 2026-10-25

- Authoritative client: `D:/Projects/project-b-client`, branch `wotlk`, clean at
  `4c718693d4dad2d83467a4cc62a7a8e7436aa91e`.
- This checkout already contains the integrated v2.1 bridge, profile source, v264 actor and MH2O
  liquid increments. The former asset worktree hashes match the client for every integrated asset
  file; that worktree is preserved frozen and is not a second authority.
- WM package boundary: `D:/WOW/wm-project/src/wm_native` is byte-identical to the preserved
  `D:/Projects/project-b-opencode/integrations/wm-native/src/wm_native` for all ten modules.
- Preserved, not used as authority: `D:/Projects/project-b-opencode` uncommitted bridge/docs/WM
  work, `D:/Projects/project-b-opencode-assets` dirty asset worktree, WM glue edits and untracked
  WM files.
- Ownership: OpenCode is the sole implementation worker and integration lead. Former GLM files are
  now in scope.
- Execution: source inspection/adoption performed. No live service, game, DB, install or visual
  probe run. Build/test permissions remain those already granted in this session; this checkpoint
  records no new permission.
- Next: O1 bounded repair R8, R9, R10, R13, S1; then the first rendered connected world.
