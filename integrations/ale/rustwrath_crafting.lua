--[[
RustWrath: crafting keeps going while the player moves (owner request 2026-11-20). ALE (Eluna)
Lua, live: no core rebuild, no restart.

Install: copy this file into the server's `run\lua_scripts` folder. ALE auto-reloads it within a
second; `.reload ALE` does the same.

AzerothCore stops a cast on the move in two places, both on the spell's `InterruptFlags`
movement bit (`SPELL_INTERRUPT_FLAG_MOVEMENT`, 0x1): the start refusal (`Spell::prepare`,
`SPELL_FAILED_MOVING`) and the in-flight cancel (`Spell::update`). On load this script clears
that bit for every tradeskill spell (`SPELL_ATTR0_IS_TRADESKILL`, 0x20, the recipe-list flag), so
one rule covers every profession, enchanting included. `SpellEntry:SetInterruptFlags` writes the
`Spell.dbc` store and the live `SpellInfo` together. Aura and channel interrupts are untouched.

The RustWrath client applies the same rule in its own gates (`spell::movement_interrupts`). Stock
3.3.5a clients still cancel locally when they move.

Removing the file does not restore the bits; a server restart does.
]]

local SPELL_ATTR0_IS_TRADESKILL = 0x20
local SPELL_INTERRUPT_FLAG_MOVEMENT = 0x1
local MAX_SPELL_ID = 100000 -- above the highest 3.3.5a Spell.dbc id (80864) and custom rows

local cleared, checked = 0, 0
for id = 1, MAX_SPELL_ID do
    local entry = LookupEntry("Spell", id)
    if entry then
        checked = checked + 1
        if bit32.band(entry:GetAttributes(), SPELL_ATTR0_IS_TRADESKILL) ~= 0 then
            local flags = entry:GetInterruptFlags()
            if bit32.band(flags, SPELL_INTERRUPT_FLAG_MOVEMENT) ~= 0 then
                entry:SetInterruptFlags(bit32.band(flags, bit32.bnot(SPELL_INTERRUPT_FLAG_MOVEMENT)))
                cleared = cleared + 1
            end
        end
    end
end

print(string.format("[rustwrath_crafting] %d spells read; movement interrupt cleared on %d tradeskill spells",
    checked, cleared))
