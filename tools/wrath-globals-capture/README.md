# 3.3.5a globals capture

The addon harness compares our `_G` with the real client's. The checked-in
`reference/lua_globals.tsv` is a 1.12 capture; this addon produces the 3.3.5a one.

1. Copy the `RustWrathGlobalsCapture` folder into the **stock 3.3.5a client's**
   `Interface\AddOns` (not into the RustWrath install; the stock client is the oracle).
2. Log in, enter the world once, and read the printed name count.
3. `/reload` or log out. The client writes
   `WTF\Account\<ACCOUNT>\SavedVariables\RustWrathGlobalsCapture.lua`.
4. Convert it:

   ```powershell
   powershell -NoProfile -File tools/import-globals-capture.ps1 -File '<the SavedVariables file>'
   ```

   The converter writes `reference/lua_globals_30300.tsv` (name, type, origin `capture`).
5. Commit that file. `WOW_WRATH=1 cargo run -q -p benilla-app --example addon_harness -- --surface`
   then reports the real 3.3.5a diff instead of skipping it.
