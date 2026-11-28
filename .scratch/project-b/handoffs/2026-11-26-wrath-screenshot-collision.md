# Preserve screenshots when the local timestamp repeats

Status: ready to merge
Owner: Claude Opus 5.5
Branch: fix/wrath-screenshot-collision
Issue: none
Next: review and run the focused `benilla-app` screenshot test in the owner's Cargo slot

## Change

The screenshot writer now reserves the preferred PNG path with exclusive creation before encoding. If another file already owns that timestamp, it tries `_2`, `_3`, and later suffixes. This prevents an existing screenshot from being truncated during a repeated local-clock timestamp, including an autumn DST fallback, and also covers concurrent capture workers.

Encoding failures remove the reserved empty file before reporting failure.

## Verification

- `rustfmt --edition 2021 --check crates/benilla-app/src/screenshot.rs`: passed.
- `git diff --check`: passed.
- Cargo tests/builds were not run because the repository reserves the Cargo slot for the owner.
- Client/server were not launched; no game data, credentials, or runtime state was accessed.

## Focused test

`a_taken_screenshot_name_gets_the_next_free_suffix` creates an existing timestamped file, reserves the name, writes a replacement payload to the returned handle, and asserts the original remains intact while `_2.png` receives the new payload.
