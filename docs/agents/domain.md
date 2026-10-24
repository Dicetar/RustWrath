# Domain docs layout

Single-context. `CONTEXT.md` at repo root holds the Project B domain language.
ADRs live in `docs/adr/NNNN-<slug>.md`, numbered from 0001. Consumers: read
`CONTEXT.md` before naming things; an ADR is required for any decision that
changes a seam, an ownership boundary, or a protocol/extension contract.
