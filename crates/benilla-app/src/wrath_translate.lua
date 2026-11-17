-- The shipped Wrath -> 1.12 translation script (see wrath_translate.rs).
--
-- Translate(kind) runs once per projected object, after the hand-written projection and the
-- field table. kind is "unit", "player", "gameobject", "dynamicobject" or "corpse".
--
--   Raw(i)                 delivered 3.3.5 descriptor word i, or nil
--   Get(i)                 the 1.12 word i this pass writes so far, or nil
--   Set(i, v)              write 1.12 word i (wins over everything before it)
--   DbcRow(file, id)       every column of that row of DBFilesClient\<file>.dbc, or nothing
--   DbcString(file, off)   a string from that file's string block, or nil
--   Log(text)              an info line in the client log
--
-- An overlay at benilla-config/wrath-translate.lua runs after this file and is reloaded when
-- it changes. To keep this behaviour and add to it:
--
--   local Shipped = Translate
--   function Translate(kind)
--     Shipped(kind)
--     -- your code
--   end

local band, bor, lshift = bit.band, bit.bor, bit.lshift

-- 12340 sends a creature's weapons as item ids (UNIT_VIRTUAL_ITEM_SLOT_ID, 56 + slot) and looks
-- each one up in Item.dbc on create and change. 1.12 carries the resolved display (37 + slot),
-- the class/subclass/material/inventory-type bytes (40 + 2*slot) and the sheath (41 + 2*slot).
-- A missing row is all zeros there, an empty hand. Players arm from their visible-item fields.
-- Item.dbc columns: ID, Class, Subclass, SoundOverrideSubclass, Material, DisplayInfoID,
-- InventoryType, SheatheType.
local function CreatureWeapons()
  for slot = 0, 2 do
    local item = Raw(56 + slot)
    if item then
      local _, class, subclass, _, material, display, inventory, sheath = DbcRow("Item", item)
      Set(37 + slot, display or 0)
      Set(40 + 2 * slot, bor(band(class or 0, 255),
                             lshift(band(subclass or 0, 255), 8),
                             lshift(band(material or 0, 255), 16),
                             lshift(band(inventory or 0, 255), 24)))
      Set(41 + 2 * slot, band(sheath or 0, 255))
    end
  end
end

function Translate(kind)
  if kind == "unit" then
    CreatureWeapons()
  end
end
