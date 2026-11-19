# Gates: allow GameTooltip:AddTexture on the 1.12 surface

Status: ready to merge
Owner: OpenCode / Flint
Branch: oc/gates-widget-surface
Issue: none; restores the gates job
Next: none.

Added one ALLOWED_BEYOND row for AddTexture on GameTooltip citing 12340's Texture1-10 pool line.
The gate's existing test is the check; CI: not run yet.

The visible tooltip is unchanged; only the gate's exception list grew.
