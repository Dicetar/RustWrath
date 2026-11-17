# Live code: patch every workspace crate the client links

Status: landed on `main`. The sixth dx commit (Windows TLS and exe-static reach) is not
compiled or run.
Owner: Claude Code (Opus 5.5)
Branch: fix/live-code-dx-workspace (landed `fa707857`), fix/live-code-dx-chkstk (landed
`84ef7600`), then live-dx3, live-dx4, live-dx5, fix/live-code-dx-windows-tls
Issue: none. Owner session 2026-11-17: the trainer/game-object fix did not patch into the
running client, and the first patch that did land crashed it.
Next: the owner reinstalls dx (`tools/install-live-dx.ps1`) and relaunches with `-LiveCode`;
apply `eb410dd7` live and check that the patch lands and the client keeps running. dx.log
records `patch image base: 0x…` for each patch.

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

## The replay put benilla_app before its optional dependency

With the settle and failed-build hunks installed, the owner relaunched and the fix was
applied. dx.log showed one patch with all three changed crates, so the batching worked. The
replay then compiled `benilla_app` before `benilla_hotpatch_bevy` and failed with `E0463 can't
find crate for benilla_hotpatch_bevy` (`crates/benilla-app/src/hotpatch.rs:3`).

`benilla-app` reaches that crate only through its own feature, `hotpatch =
["dep:benilla-hotpatch-bevy"]`. In the krates graph that is the edge
`Feature(benilla-app, hotpatch)` → `benilla-hotpatch-bevy`. krates 0.17.5 `direct_dependents`
follows feature nodes only when they belong to the target crate, so it never reports
`benilla-app`. dx builds both the cascade and the replay order from that call, and with no
edge the lexicographic tie-break put `benilla_app` first.

The fifth commit changes two things in the dx patch:

- `workspace_dependents_of` (`build/request.rs`) walks the incoming graph edges itself. A
  crate node, or a feature node of another crate, counts as a dependent; the target's own
  feature nodes are followed further.
- `compile_workspace_hotpatch` (`build/link.rs`) re-sorts the replay by each crate's captured
  `--extern` arguments, which are what the fat build actually linked. Ties keep the earlier
  order.

## Thread-locals in the patch DLL crash the client

The relaunch at 23:15 used the patched dx with the replay-order fix (marker `44C145B4…`). The fix
was applied at 23:19:23. dx replayed nine crates in dependency order: formats, assets, lua,
mlua, protocol_wrath, ui, world, hotpatch_bevy, app. The replay took 288 s, and the client
logged "live code: patch 1 landed" at 23:24:27. Eleven seconds later it died: WER reports
`c0000005` in `libbenilla-patch-1794954251251.dll` at RVA `0x2163e50`, then exit `0xc000041d`.

`dumpbin /disasm` on the patch DLL puts that RVA at the first instruction of
`benilla_mlua::RUNNING`'s thread-local accessor, `mov eax, dword ptr [_tls_index]`. Three
things break here:

- The patch objects leave `_tls_index` undefined. `create_undefined_symbol_stub` defines it as
  an absolute symbol at the exe's runtime address. The access is a 32-bit RIP-relative load,
  and the exe is mapped far more than 2 GB from the DLL, so the displacement wraps (the
  disassembly shows the target as `0x17B35A2B8`) and the load faults.
- Even with a valid index, the SECREL offsets (`[block+0xE8]`) come from the patch DLL's own
  `.tls` layout. They would read the wrong variables in the exe's TLS block.
- The patch DLL has no TLS directory, so the loader never allocates its `.tls` section.

dx 0.7.10's TLS stub path is for ELF and Mach-O. Upstream added a `tls_fixups` jump-table field
for "future runtime TLS sharing" (DioxusLabs/dioxus#5291), but Windows TLS sharing is not
implemented.

`benilla_mlua` was in the patch only because the cascade replays every dependent of
`benilla_formats`; its source did not change. `benilla-app` also has `thread_local!` (sound
mixer, lanes, quest trace, loot UI), so a patch can reach this crash through the app too.

The trainer/game-object fix stays applied in the live worktree. A full rebuild (`r` in dx, or a
relaunch) includes it without a live patch.

## Patch DLLs get their own TLS and a base near the exe

The wrapped `_tls_index` load is one case of a wider fault. `dumpbin /disasm` of the crashed DLL
(image `0x180000000`–`0x18564C000`) finds 71 RIP-relative loads whose target is outside the
image and below the exe: 29 are `_tls_index` and 42 are other exe statics. Every exe data symbol
the stub resolves is an absolute address. link.exe computes the 32-bit displacement against the
DLL's preferred base, the DLL is linked with ASLR, and the loader never relocates a
RIP-relative displacement. Upstream's comment on `/HIGHENTROPYVA:NO` ("prevent aslr from
overflowing 32 bits") assumes the exe stays low. This exe ran at `0x7FF7…`.

The sixth commit changes two things in the dx patch:

- `create_undefined_symbol_stub` (`build/patch.rs`) no longer resolves `_tls_index`, `_tls_used`,
  `_tls_start`, `_tls_end`, `__xl_a` and `__xl_z` from the exe. link.exe takes them from
  `tlssup.obj` in `msvcrt.lib`; `dumpbin /linkermember` shows the MSVC 14.36 `msvcrt.lib`
  defines all six. The link adds `/INCLUDE:_tls_used`, so the DLL has a TLS directory. Windows
  allocates a `LoadLibrary`'d DLL's TLS for threads that already exist. The patch's
  thread-locals start from their initial values and are separate from the exe's copies. This
  is how dx already treats TLS on Linux and macOS ("each patch gets its own TLS copy").
- `compile_workspace_hotpatch` (`build/link.rs`) links each Windows patch with
  `/BASE:<addr> /DYNAMICBASE:NO`.
  - `windows_patch_image_base` puts the first patch 16 MiB past the exe image. It reads
    `SizeOfImage` from the exe's PE header and finds the runtime base from the ASLR reference
    minus `main`'s RVA.
  - Each later patch goes past the one before, because subsecond keeps them loaded.
  - A patch whose image would end more than 2 GiB past the exe base fails with a message to
    restart. At this patch's size (86 MiB) and this exe's (194 MiB), that leaves about 20 patches
    per client run.
  - The placement resets when the exe's base or modification time changes.

Not covered:

- If something else occupies the chosen base, the loader relocates the DLL and the exe-static
  loads fault again. dx cannot see the client's address space to check.
- A `thread_local!` that patched and unpatched code must share (for example `benilla_mlua`'s
  `RUNNING` flag) has two copies while a patch is loaded. That gives wrong values, not a crash.

## What changed

- `tools/live-code/dx-0.7.10-workspace-crates.patch` (the Windows stub fix is above):
  - The watch list is a breadth-first walk over the dependency graph. It skips registry crates
    (manifests under `.cargo`).
  - The replay keeps only crates the fat build captured arguments for.
  - The patch applies cleanly to a fresh extraction of the crate.
- `tools/install-live-dx.ps1`:
  - takes the `.crate` from Cargo's cache, or from crates.io, and checks its SHA-256;
  - unpacks it under `%LOCALAPPDATA%\RustWrath\dx-src` and applies the patch;
  - runs `cargo install --path … --locked --force` with the wrapper bypassed, into
    `dx-src\target-0.7.10` beside the source tree, so a reinstall rebuilds dx and not its
    ~800 dependencies (the first install built inside the tree it then deletes; the script
    moves that output over once);
  - writes `dx.rustwrath-patch` (the patch hash) beside `dx.exe`.
- `tools/launch-client.ps1 -LiveCode`:
  - points a missing or mismatched dx at the install script (stock install for other versions);
  - warns when the marker is missing or stale;
  - records `dx_patch=` in `launch.txt`.
- ADR-0002 "Live code" and the reload-class row, and `docs/agents/local-launch.md`.

## Unverified

- A cross-crate patch landed (23:24:27) but crashed 11 s later on Windows TLS. No patch has
  yet run to completion.
- The sixth dx commit is not compiled. Whether the loader honours the chosen base in the
  client, and whether link.exe accepts the TLS objects without warnings, are unverified.
- The trainer, chest and ore-vein behaviour of `eb410dd7` has not been checked in game.
