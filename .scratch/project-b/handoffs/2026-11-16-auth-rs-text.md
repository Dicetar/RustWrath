# auth.rs was a binary file: one NUL byte in a doc comment

Status: ready to merge
Owner: OpenCode / DeepSeek V4.1 Flash (overseer)
Branch: fix/main-build-2026-11-16
Issue: none
Next: land on source review; no further build is needed

Based on `origin/main` `ad0a6a15`.

## The defect

`git diff` reported `crates/benilla-protocol-wrath/src/auth.rs` as `Bin 7844 -> 9766 bytes`, and
`file` called it `data`. The file carried exactly one NUL byte, at offset 1590, inside a doc
comment: the text meant to read `"nUS\\0"` had a literal NUL between `US` and `"`.

A NUL makes git treat the whole file as binary: no diffs, no review of any change to it, and an
unmergeable file the moment two branches touch it. It reached `main` inside the login/glue batch,
so every later review of that file has been blind.

## The change

The byte is written as the two characters `\\0`; the comment says the same thing. Nothing else in
the file changed.

## Checks run

| Check | Result |
|---|---|
| `file crates/benilla-protocol-wrath/src/auth.rs` before | `data` |
| after | `Unicode text, UTF-8 text` |
| `cargo check -p benilla --features hotpatch` on this tree | Finished in 5m 37s, no errors |

The check was run on the tree before the edit; the edit is inside a comment, and rustc had already
accepted the NUL form.

## Not proven

Nothing needs a client. The file is now text for every future diff and merge.
