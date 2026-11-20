# Authorized focused Wrath host smoke

Explicit human grant: one existing corrected Wrath UI smoke test at a pinned commit, wm-hd,
BENILLA_REQUIRE_DATA=1, captured output, maximum 20 minutes including compilation. Before the
test the human requested review of OC's work; no test had started at that point. Astra inspected
5b4bcdc3 and the staged aura parser: the monotonic arrival instant reaches seed_initial, the
1.12 producer retains its None fallback, and complete aura parsing precedes state mutation.
OC's reported checks/test counts were not rerun or adopted as Astra execution.

Pinned source: `404cea295228d023257fec946b44bfc6f9c368cf`, clean detached checkout at
`F:/AI/tmp/project-b-astra-wrath-smoke-20261027`. This includes OC's fixes and Astra's corrected
fixture/committed aura consumers.

Selected data: `D:/WOW/world of warcraft 3.3.5a hd/data`.
Profile: `wm-hd`, pinned checkout's `docs/profiles/wm-hd.mpqlist`.
Manifest SHA-256: `439bdc21d09f5509073cd88cc341adb2eb05854b425f4d9dec1d2bcee5083d59`.
Cargo used the existing shared target directory with locked/offline dependency resolution and
Rustup update checks disabled. Credential/capture entries were removed without printing values;
the native WM token was unset. No game/window, socket, WM, server or DB operation was launched.

Command, exactly once:

```text
cargo test --locked --offline -p benilla-app --lib ui_script::wrath_ui_slice_tests::stock_player_frame_host_smoke_test -- --exact --nocapture
```

Started 2026-10-27 15:16:37.702 Europe/Moscow (12:16:37.702 UTC).
Finished 15:27:58.463 Europe/Moscow (12:27:58.463 UTC).
Total 680.8 seconds, approximately 11m21s, below the 20-minute deadline. No timeout.

Result: exit 101. The app library test binary compiled with warnings (`Finished test profile`
in 11m07s). Exactly one test ran: 0 passed, 1 failed, 0 ignored, 6,601 filtered out; test runtime
7.40s. The explicit admission diagnostic confirms Wrath335, interface 30300 and the selected
wm-hd manifest/data root. This was not a no-data skip or vanilla-mode invocation.

First observed failure:

```text
Interface/FrameXML/Constants.lua:87: attempt to call global `FillLocalizedClassList' (a nil value)
```

The fixture stops while loading Constants.lua, before PlayerFrame construction, manual health
event/query assertions or CVar readback. No success is claimed for those later stages. This is
direct evidence of a missing host API on the admitted profile; no stub or fixture weakening was
added. Existing ChrClasses catalog ownership is a source lead for the next repair, not a proven
implementation of this missing API.

Retained logs: `run.log`, `test.stdout.log`, `test.stderr.log`, including backtrace and compiler
warnings. The clean temporary checkout was removed without force after checking its exact
resolved path; source and unrelated untracked owner files were preserved. The execution session
is terminal, the one-time grant is consumed, and no rerun/new test case/fix execution followed.

Evidence boundary: compilation of this snapshot's app test target and one failed, data-admitted
Wrath host initialization. No dedicated aura/cooldown timing cases, session-driven UI behavior,
installed-mod loading, rendered/live Northshire loop, WM behavior, playable alpha or full
stock-client replacement is established.
