# Wrath SpamMessages.dbc 2-field schema & ChatProfanity.dbc RegexSet recovery

Status: ready to merge
Branch: fix/wrath-spam-and-profanity-dbc
Owner: AI Studio Build
Base: origin/main (9dc36e48)

## Cause
- `crates/benilla-formats/src/text_filter_lists.rs`: `schema()` hardcoded 3 fields (`ID`, `Pattern`, `Language`), while Wrath 3.3.5a's `SpamMessages.dbc` dropped `Language` and has 2 fields (`ID`, `Pattern`), failing with `applying SpamMessages schema (field-count mismatch?): schema has 3 fields but file has 2`.
- `crates/benilla-app/src/text_filter.rs`:
  1. `Matcher::compile` validated `fancy_regex::Expr::parse_tree(&t.exact)` alone when `t.bounded == true`, never validating `&t.relaxed` against `regex::Regex::new`. Any bounded pattern with an escape rejected by `regex-syntax` passed `Matcher::compile` and caused `RegexSet::new(&patterns)` to fail (`unrecognized escape sequence`) and disable the entire profanity filter.
  2. `translate()` did not mirror PCRE's literal treatment of non-quantifier `{...}` braces (such as `{性}交` in record ID 11957) or redundant `\` before non-alphanumeric non-metacharacters (`\/`, `\_`, `\'`, etc.).

## Fix
- `crates/benilla-formats/src/text_filter_lists.rs`: inspect `crate::dbc::field_count(bytes)` so 2-field tables use `[ID, Pattern]` while 3-field tables retain `[ID, Pattern, Language]`; gate hardcoded 1.12 donor counts on `crate::is_donor_install(&data)` and add a synthetic 2-field + 3-field DBC unit test.
- `crates/benilla-app/src/text_filter.rs`:
  - Validate `Regex::new(&t.relaxed)` alongside `fancy_regex::Expr::parse_tree(&t.exact)` in `Matcher::compile` when `t.bounded` is true.
  - Escape literal non-quantifier `{...}` / stray `}` outside `[...]` and unwrap redundant `\` before non-alphanumeric non-metacharacters in `translate()`.
  - Add 2 focused unit tests (`literal_braces_and_punctuation_escapes_translate_like_pcre`, `bad_bounded_relaxed_row_is_skipped_without_disabling_the_set`) and verify `RegexSet` size matches compiled row count on Wrath data.

## Verification
- Compilation, unit tests, and client launch: not run (`AGENTS.md`: owner runs Cargo and client in batch).

## Next
- Source review and batch `cargo test -p benilla-formats text_filter_lists` and `cargo test -p benilla-app text_filter`, then manual Wrath startup check that `chat filters:` reports non-zero profanity and spam expressions without `the pattern set would not build`.
