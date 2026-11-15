# Preserve Lua caller frames in launcher errors

Status: ready to merge
Owner: Codex
Branch: fix/lua-error-traceback
Issue: none
Next: include in the owner's build batch and check stderr for Lua traceback on a reproduced UI error

The Nov 12 run recorded 63 MoneyFrame errors at the same stock Lua line.
The launcher retained the failing expression's location. The caller that
supplied the nil amount was absent even in the original stderr log.

The Rust host captures up to 16 active Lua file and line locations before
the protected call unwinds. The diagnostic includes a second line labeled
`Lua traceback`. The Lua error handler still receives the raised value,
and Lua `pcall` and `xpcall` keep their own error semantics. Locations come
from the frames still active at capture time.

The existing error-handler test now raises a nil money amount across two
named chunks and checks that the host retains both locations while Lua
hears the bare error. Source review traced the event and click dispatches
through the protected VM call to the launcher's stderr. The live build,
test execution and MoneyFrame reproduction are pending the owner's batch.

The diagnostics collector associates the continuation with the preceding
error in the same log stream. Its report and repair tickets retain the
caller chain. Errors from `RunScript` retain the same caller information
and their original headline.
