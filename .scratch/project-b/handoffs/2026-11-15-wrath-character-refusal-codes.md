# Wrath character login refusal and rename responses

Status: ready to merge
Owner: Codex
Branch: fix/wrath-character-refusal-codes
Issue: none
Next: Include in the owner build and check return to character selection after a refused character login.

The character-login wire byte is AzerothCore's `LoginFailureReason`, not a
`ResponseCodes` value. `SendCharLoginFailed` writes it directly; the 12340
`ClientConnection::HandleCharacterLoginFailed` handler (`0x006B2070`)
confirms indices 1–6 and the two additional Wrath lock reasons at 7 and 8.
The first version of this branch confused internal response codes with wire
values. The published follow-up preserves wire indices and shows the two
Wrath lock reasons without changing the donor dialogs. Unsupported failures
use the generic character-login dialog. The Wrath rename path distinguishes a taken name
from a generic refusal, and validates a successful reply's GUID and terminated UTF-8
name before changing the roster. A malformed rename reply keeps selection available.

This change is already on `integration/wrath-alpha`. It is presented here alone so
the mainline integrator can take it without merging the integration branch. Source
review and `git diff --check` passed; compilation and live validation remain open.
