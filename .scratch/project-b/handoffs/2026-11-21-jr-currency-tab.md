# Wrath currency tab

Status: complete
Owner: Claude Opus 5.5 / Anthropic
Branch: jr/currency-tab
Issue: none
Next: None. Runtime TokenFrame rendering remains unverified by request.

Implemented source-backed currency support in `crates/benilla-formats/src/currency.rs`, `crates/benilla-ui/src/script/currency.rs`, `crates/benilla-ui/src/script/wrath_natives.rs`, and `crates/benilla-app/src/ui_script/wrath_currency_feed.rs`. The feed loads both Wrath DBC catalogs, projects known/token/honor/arena state, resolves item names/icons through existing caches, persists category expansion, and raises `CURRENCY_DISPLAY_UPDATE`. The natives provide exact list/backpack arities and CVar-backed unused/backpack setters. The old unsupported `GetBackpackCurrencyInfo` registration is removed while the neighboring `GetNumEquipmentSets` removal is preserved.

Reference: preserved wowdecomp checkout at pin `e67b6ffef23655dc7da061cb2d36b7963f6ef71a`.

Reference notes:

- `CurrencyTypes.cpp`: `BuildDisplayList` sorts categories by `(flags & 1)` and case-insensitive name, reserves the final category as the unused bucket, sorts ordinary entries by item name, and emits backpack entries in raw DBC order.
- `CurrencyTypes_Scripts.cpp`: list info returns nine values; backpack info returns five; invalid rows return nils at those exact arities. Setters mutate `currencyTokensUnused1/2` and `currencyTokensBackpack1/2` locally and rebuild the display list.
- `CurrencyTypesRec.h`: `CurrencyTypes.dbc` is ID, item ID, category ID, raw 1-based bit index. `CurrencyCategoryRec.h`: 19 columns, ID, flags, 16 localized name columns, and name flags.

Verification completed: `rustfmt --edition 2021` and `git diff --check`. CI run `44962315146` passed on commit `b7cc2ed373a0bc57826778b8c804256b09cf07ce`: Windows check, test-build, and tests; Linux fmt, clippy, test-build, tests, doc-links, message-catalog, reference, pass-span-lint, player-clippy, player-tests, and live-code-check. Local Cargo execution remained prohibited.

The focused test suite covers raw catalog bit indexes/category fields, row ordering and expansion, unused-mask header behavior, DBC-order backpack rows, and exact Lua native arities. The previously failing unused-header expectation was corrected; both platform test suites now pass.

What is not proven in game: no client, server, login, asset installation, or visible Token/BackpackTokenFrame behavior was performed. The requested `scripts/agent-tasks.sh` workflow was unavailable because PowerShell could not start WSL. Manual workflow dispatch was denied with HTTP 403, so verification used push-triggered CI.
