# Sweep, thrust and slam telegraphs

Status: landed
Owner: Astra / OpenAI
Branch: feat/astra-strike-shapes
Issue: none
Next: Owner copies integrations/ale/rustwrath_souls.lua and checks each shape.

## What changed
Server entry policy: kobold worker/miner entries 6/257 thrust, Harvest entries
36/114 slam, all others sweep. Sweep: 90-degree half-cone at normal reach;
thrust: 20 degrees and +2 yards; slam: circle and 1.5x normal windup.
STRIKE appends an optional kind after the existing fields. Missing/unknown kinds
retain the client's legacy 60-degree half-cone; old clients still parse the packet.
Client and server fallback judgement use the selected geometry. Bars are orange,
blue or purple by kind, retaining main's under-nameplate position.
Added parser/geometry cases for every kind; updated the roadmap and protocol list.

## CI run id and result
44432258637: Linux app/formats tests pass, including shape cases; inherited UI failures were subsequently fixed on main.
44434902564: Windows workspace check and UI/protocol-wrath tests pass; Linux tests pass.
Only untouched player-clippy debt remains red. No local Cargo/client/server run.

## What is not proven in game
Entry-policy feel, geometry versus rendered positions, colours, mixed-version
judgements and latency. Lua was not deployed or executed; existing victim targeting
is unchanged (the circle is not a new multi-victim damage system).
