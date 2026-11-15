# Recover the current world after a refused Wrath transfer

Status: ready to merge
Owner: Codex
Branch: fix/wrath-transfer-recovery
Issue: none
Next: include in the manual build batch and check that a refused instance entry leaves movement and interactions available

The session entered `TransferPending` on a far-transfer announcement and
had no 12340 decoder for `SMSG_TRANSFER_ABORTED`. The worker's scoped
interaction admission requires `InWorld`, so a refusal after the announcement
could leave the old world visible while its interactions were rejected.

The decoder now reads the map, reason and reason-specific argument. A
well-formed refusal for the pending map restores the phase interrupted by
the announcement without changing the world generation, entities, or player
position. A refusal for a different map or after `NEW_WORLD` cannot restore
the old world. A malformed refusal is rejected by the gameplay decoder and
does not release the pending phase.

The app displays the localized transfer refusal in system chat using the
Wrath reason keys. Difficulty refusals use the stock global fallback because
the app does not load the per-map `MapDifficulty.dbc` message. The 1.12
packet and reason table retain their existing behavior.

The preserved 12340 reference is pinned at
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.
`Client.cpp::Packet_SMSG_TRANSFER_ABORTED` at `0x00403910` supplies
the presentation contract. The matching server's `Player::SendTransferAborted`
confirms the conditional argument byte. The earlier local draft was ported
to a new worktree and remains intact.

Source review and `git diff --check` passed. The existing session test file
covers a valid refusal, a different destination, a truncated refusal, and
a refusal after `NEW_WORLD`. No build, tests, or client launch ran.
