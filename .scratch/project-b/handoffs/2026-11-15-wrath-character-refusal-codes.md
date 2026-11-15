# Wrath character login refusal and rename responses

Status: ready to merge
Owner: Codex
Branch: fix/wrath-character-refusal-codes
Issue: none
Next: Include in the owner build and check return to character selection after a refused character login.

The original integration used character-creation bytes for character-login failures.
The 12340 values in `WowSvcsClient/Types.h` and AzerothCore `ResponseCodes` map
to the existing glue result indices: no world, duplicate character, no instances,
disabled, no character, and locked for transfer. Unsupported failures still use the
generic character-login dialog. The Wrath rename path also distinguishes a taken name
from a generic refusal, and validates a successful reply's GUID and terminated UTF-8
name before changing the roster. A malformed rename reply keeps selection available.

This change is already on `integration/wrath-alpha`. It is presented here alone so
the mainline integrator can take it without merging the integration branch. Source
review and `git diff --check` passed; compilation and live validation remain open.
