# Live code: patch every workspace crate the client links

Status: landed on `main`; the patched dx built and ran, the batch fix is not compiled
Owner: Claude Code (Opus 5.5)
Branch: fix/live-code-dx-workspace (landed `fa707857`), fix/live-code-dx-chkstk (landed
`84ef7600`), then live-dx3
Issue: none. Owner session 2026-11-17: the trainer/game-object fix did not patch into the
running client, and the first patch that did land crashed it.
Next: the owner reruns `tools/install-live-dx.ps1` and relaunches `-LiveCode`. Then the
trainer/game-object fix is applied in the live worktree as the acceptance patch.

Date 2026-11-17. Worktree `D:/Projects/wow/worktrees/live-dx`, cut from `origin/main` `7114694f`.

## What the owner's session showed

The owner ran `launch-client.ps1 -LiveCode` with stock dx 0.7.10 in `D:/Projects/wow/worktrees/live`.

- The flags parsed and the first build finished through the sccache wrapper.
- The client started under dx.

Applying `eb410dd7` (the trainer/game-object fix) in that worktree failed the patch build with
`unresolved import benilla_formats::LOCK_KEY_SPELL`. Reading dx 0.7.10's source showed two causes:

- **Watch scope.** `serve/runner.rs` `local_dependencies` lists only the tip crate's direct
  dependencies (`benilla` → `benilla-app`). The `benilla-formats` edit was never seen, so only
  `benilla-app` was rebuilt, against the old formats.
- **Replay set.** `build/builder.rs` `patch_rebuild` adds every workspace crate that depends on a
  changed crate, across the whole workspace. That includes `benilla-worldview` and
  `benilla-wm-driver`, which the client does not link. `build/link.rs`
  `compile_workspace_hotpatch` has no fat-build rustc arguments for them and fails with
  `Missing rustc args for replay`. Fixing the watch scope alone would hit this.

## The crash after the first landed patch

Removing the fix files again in the live worktree (22:16, still stock dx) produced a patch
that compiled. The client logged "live code: patch 1 landed", re-derived the spell catalog,
and about a second later printed "thread 'main' has overflowed its stack". It exited with
`0xc000041d`.

dx 0.7.10 `build/patch.rs` `create_undefined_symbol_stub` builds Windows x64 stubs as
`movabs rax, addr; jmp rax`. A patched function reaches `__chkstk` in the original exe
through such a stub, and `__chkstk` reads the frame size from RAX. Any patched function
with a frame over 4 KiB therefore overflows on entry. This is upstream DioxusLabs/dioxus#5966;
the fix is in PR #5967, which was open when checked. The second commit adds that fix to the
dx patch (`jmp [rip+0]` plus the 64-bit address, the stub dx already uses on other x86_64
targets). The stub is 14 bytes instead of 12; its size is taken from the byte length.

## The patched dx still built benilla-app alone

The owner installed the patched dx (marker matched) and relaunched. Applying `eb410dd7` at
22:36 failed again at 22:37:05 with the same `unresolved import benilla_formats::LOCK_KEY_SPELL`.
The captured arguments for `benilla_formats` existed. Its rlib was still the 22:31 fat build's,
so it was never replayed.

dx 0.7.10 has no debounce. `serve/runner.rs` `wait` hands the first watcher event to
`handle_file_change`, which starts a patch at once. The cherry-pick writes files one at a time:
`crates/benilla-app/src/target/lock.rs` came first, and the formats and protocol files arrived
during that build. Changes that arrive during a build are queued. `serve/mod.rs` processes the
queue only after `BuildReady`, never after `BuildFailed`. So the app-only patch failed, and the
formats half of the change was dropped. The stock-dx attempt failed the same way, on top of the
watch scope.

The third commit adds two hunks to the dx patch:

- After the first event, `wait` keeps collecting events until the watcher has been quiet for
  300 ms, with a 2 s cap.
- The `BuildFailed` arm processes the queued changes like `BuildReady` does. A failed patch
  followed by its missing half then patches again. `modified_crates` is cumulative, so the
  retry replays both crates, in dependency order.

`launch-client.ps1` now passes `--log-to-file <run>\dx.log`. dx traces its own modules for
`serve`, so the log shows each patch's `changed_crates` and `modified_crates`.

Also read, not a cause here: krates 0.17.5 `direct_dependents` shadows `nid` in its loop, so it
misses a dependent reached only through a chain of feature nodes. `benilla-app` has a plain
edge to `benilla-formats`, so the ordering edge exists.

## What changed

- `tools/live-code/dx-0.7.10-workspace-crates.patch` (the Windows stub fix is above):
  - The watch list is a breadth-first walk over the dependency graph. It skips registry crates
    (manifests under `.cargo`).
  - The replay keeps only crates the fat build captured arguments for.
  - The patch applies cleanly to a fresh extraction of the crate.
- `tools/install-live-dx.ps1`:
  - takes the `.crate` from Cargo's cache, or from crates.io, and checks its SHA-256;
  - unpacks it under `%LOCALAPPDATA%\RustWrath\dx-src` and applies the patch;
  - runs `cargo install --path … --locked --force` with the wrapper bypassed;
  - writes `dx.rustwrath-patch` (the patch hash) beside `dx.exe`.
- `tools/launch-client.ps1 -LiveCode`:
  - points a missing or mismatched dx at the install script (stock install for other versions);
  - warns when the marker is missing or stale;
  - records `dx_patch=` in `launch.txt`.
- ADR-0002 "Live code" and the reload-class row, and `docs/agents/local-launch.md`.

## Unverified

- The settle and failed-build hunks have not been compiled. The rest of the patch built and ran.
- No cross-crate patch has yet landed in a running client.
- `benilla-app`'s hotpatched build pulls in optional dependencies (`bevy/hotpatching`). If the
  krates graph from `cargo metadata` lacks an edge the build uses, that crate stays unwatched.
