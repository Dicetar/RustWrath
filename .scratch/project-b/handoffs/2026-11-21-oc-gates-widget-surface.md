# Gates: allow GameTooltip:AddTexture on the 1.12 surface

Status: ready to merge
Owner: OpenCode / Flint
Branch: oc/gates-widget-surface
Issue: none; restores the gates job
Next: none.

Added one ALLOWED_BEYOND row for AddTexture on GameTooltip citing 12340's Texture1-10 pool line.
The gate's existing test is the check. CI 44612782592: Windows check and the changed-crate tests green; the gates job is red only on the pre-existing `player-clippy` debt in benilla-ui files this ticket did not touch.

The visible tooltip is unchanged; only the gate's exception list grew.
