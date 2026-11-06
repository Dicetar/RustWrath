# Wrath mailbox: list, text and single-attachment compose

This source continuation is based on `a83a4d0439781dcd9df75f23c38cd8086e171870`.
It addresses part of D11 in the [replacement inventory](../../.scratch/project-b/issues/18-stock-replacement-inventory.md).

- `wrath_tap_commands` now routes mailbox list, mark-read, create-text-item, next-mail-time
  and the existing compose request to the 12340 socket. List/read/create-text carry the
  mailbox and mail id required by the local `MailHandler.cpp`; create-text omits the donor's
  extra template word. The normal reply path remains the authority for the compose and
  create-text outcomes. No automatic resend or local success is added.
- `gameplay::economy::send_mail` encodes the 12340 attachment-count and slot/GUID records;
  the existing compose owner supplies at most one attached item. Empty recipient/subject,
  NUL and oversized string constraints are checked at the UI before taking the pending-send lock. This does
  not implement 12-item composition.
- `wrath_mail_list` retains the inline letter body in `MailListEntry`; `resolve_row` makes it
  available to the existing `GetInboxText` consumer and auction invoice parser. Inline text
  is passed through the normal read-time text filter without inventing a text-query id.
  Body-copy uses the server's result and the existing copied flag.

Local evidence: `D:/WOW/WM_BridgeLab/src/azerothcore/src/server/game/Handlers/MailHandler.cpp`
`HandleSendMail`, `HandleGetMailList`, `HandleMailMarkAsRead`, `HandleMailCreateTextItem` and
`HandleQueryNextMailTime`; the client reference at pin
`e67b6ffef23655dc7da061cb2d36b7963f6ef71a`,
`WoW/Source/Ui/MailInfo_Scripts.cpp::Script_GetInboxText` (OFFSET `0x00570BD0`) reads the
inline body and marks the row read. `Script_GetInboxItem` takes both message and attachment
indices: the current donor-derived UI does not yet implement that contract.

**Still missing:** the UI projection only exposes the first attachment and its count is
narrowed; item retrieval needs the selected attachment's low GUID in `CMSG_MAIL_TAKE_ITEM`.
Money, item take, return, delete and multi-item send remain unconnected on the Wrath command
path. Do not apply the donor's auto-delete behavior to a multi-attachment mail until an owner
preserves every attachment and reconciles the server receipt. `GetInboxItem`, item links,
tooltips and take operations must agree on the same attachment index/GUID. Persisted mail
and inventory remain server-owned; reconnect must re-enumerate them.

No build, test, client, server or DB operation was run. Follow-up acceptance requires a
two-character session for text, template and auction invoice, profanity setting and reload;
send with zero/one attachment and server refusal; read/copy text and relog; exact wire bodies
and length limits; then a separately implemented two-attachment take/return/delete scenario.
Never infer successful delivery from a queued command.
