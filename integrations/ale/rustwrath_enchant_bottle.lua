--[[
RustWrath: the Enchant Bottle. ALE (Eluna) Lua, live: no core rebuild, no restart.

Random enchants are no longer lost when an item is disenchanted. They fill a per-character
bottle with charges, and the charges work on the random enchants of the gear you wear. Owner
design, approved 2026-11-21 (`.scratch/project-b/prompts/brainstorm-enchant-bottle-2026-11-20.md`).

Getting charges
- Disenchant (spell 13262) an item: every random enchant on it (slots PERM 0, TEMP 1, BONUS 5)
  adds its tier value, T1 1, T2 2, T3 4, T4 8, T5 16.
- Absorb (bottle menu): destroy a weapon or armor piece from your bags for the same charges. It
  works on any quality, white included: the core only disenchants green to epic items with a
  DisenchantID, and that check is C++.
- Both also imprint the item's best enchant into the bottle. One enchant is stored; a newer
  imprint replaces it.

Spending charges (equipped items only, random-pool enchants only)
- Reroll: a random enchant of the same tier. Cost: the tier value.
- Gamble: 25 % one tier down, 50 % the same tier, 25 % one tier up. Cost: the tier value.
- Empower: the same stat one tier up (same effect types and stats in SpellItemEnchantment.dbc).
  Cost: 2x the new tier's value.
- Awaken: fill an empty slot with a tier rolled from the item's quality window (as the loot
  roller does), lowered to the best tier your charges pay for. Cost: 1.5x the tier value.
- Pour: put the stored enchant into an empty slot. Cost: 2x its tier value.
Tiers go past the item's window up to 5 on any item. Results never use an enchant whose
required level is above the player's level, since the core applies nothing below it.

Opening the bottle: use the bottle item, or type `.bottle` (gives you the bottle if you have
none, then opens the menu). The bottle reuses stock item 44625 "Bottle of Aged Dalaran Red"
(new item_template rows load only at server start); dummy pool item 800000 is registered too for
after the next start. Using it opens the menu instead of drinking it, so it is never consumed.
Out of combat only (the stock drink spell is not usable in combat).

Storage: characters DB table `rustwrath_enchant_bottle` (created on load if missing).
]]

local BOTTLE_ITEMS = { 44625, 800000 }
local BOTTLE_GIVE = 44625
local MENU_ID = 920700 -- player gossip menu id for the bottle
local NPC_TEXT = 1 -- stock "Greetings" header text

local DISENCHANT = 13262
local PLAYER_EVENT_ON_SPELL_CAST = 5
local PLAYER_EVENT_ON_COMMAND = 42
local GOSSIP_EVENT_ON_HELLO = 1
local GOSSIP_EVENT_ON_SELECT = 2

local ITEM_CLASS_WEAPON = 2
local ITEM_CLASS_ARMOR = 4
local SLOTS = { 0, 1, 5 } -- PERM_ENCHANTMENT_SLOT, TEMP_ENCHANTMENT_SLOT, BONUS_ENCHANTMENT_SLOT
local SLOT_NAMES = { "Primary", "Secondary", "Bonus" }
local EQUIP_NAMES = {
    [0] = "Head", "Neck", "Shoulders", "Shirt", "Chest", "Waist", "Legs", "Feet", "Wrists",
    "Hands", "Finger 1", "Finger 2", "Trinket 1", "Trinket 2", "Back", "Main hand", "Off hand",
    "Ranged", "Tabard",
}
local INVENTORY_SLOT_BAG_0 = 255
local BACKPACK_FIRST, BACKPACK_LAST = 23, 38
local BAG_FIRST, BAG_LAST = 19, 22

local MAX_TIER = 5
local TIER_VALUE = { 1, 2, 4, 8, 16 }
local TIER_STEPS = { { 0, 50 }, { 1, 30 }, { 2, 15 }, { 3, 5 } } -- same window as the loot roller

local TAG = "|cff66ccff[Bottle]|r "

-- Gossip senders and actions.
local S_ROOT, S_ITEM, S_SLOT, S_ACT, S_ABSORB_LIST, S_ABSORB, S_CLOSE = 1, 2, 3, 4, 5, 6, 7
local A_REROLL, A_GAMBLE, A_EMPOWER, A_AWAKEN, A_POUR = 1, 2, 3, 4, 5

-- SpellItemEnchantment.dbc (3.3.5a): 38 uint32 columns. 0 id, 2-4 effect types, 5-7 amounts,
-- 11-13 effect args (stat, spell, school), 14 enUS name (string offset), 37 RequiredLevel.
local function u32(data, at)
    local a, b, c, d = data:byte(at + 1, at + 4)
    return a + b * 0x100 + c * 0x10000 + d * 0x1000000
end

local function load_enchants()
    local dir = tostring(GetConfigValue("DataDir") or ""):gsub("[\\/]+$", "")
    local path = dir .. "/dbc/SpellItemEnchantment.dbc"
    local file = io.open(path, "rb")
    if not file then
        print("[rustwrath_enchant_bottle] cannot open " .. path .. "; bottle disabled")
        return nil
    end
    local data = file:read("*a")
    file:close()
    if data:sub(1, 4) ~= "WDBC" then
        print("[rustwrath_enchant_bottle] " .. path .. " is not a WDBC file; bottle disabled")
        return nil
    end
    local records, fields, size = u32(data, 4), u32(data, 8), u32(data, 12)
    if fields ~= 38 or size ~= 152 then
        print(string.format("[rustwrath_enchant_bottle] unexpected layout %d x %d; bottle disabled",
            fields, size))
        return nil
    end
    local strings = 20 + records * size
    local enchants = {}
    for i = 0, records - 1 do
        local at = 20 + i * size
        local col = function(n) return u32(data, at + n * 4) end
        local family = {}
        for e = 0, 2 do
            local kind = col(2 + e)
            if kind ~= 0 then
                family[#family + 1] = kind .. ":" .. col(11 + e)
            end
        end
        local name_at = strings + col(14)
        local name_end = data:find("\0", name_at + 1, true)
        enchants[col(0)] = {
            name = name_end and data:sub(name_at + 1, name_end - 1) or "?",
            family = table.concat(family, ","),
            level = col(37),
        }
    end
    return enchants
end

local ENCHANTS = load_enchants()

-- The random pool: POOL[id] = {tier, class, sub}; BY_TIER[tier] = {ids}.
local POOL, BY_TIER = {}, {}
local function load_pool()
    for t = 1, MAX_TIER do
        BY_TIER[t] = {}
    end
    local query = WorldDBQuery("SELECT `enchantID`, `tier`, `class`, IFNULL(`exclusiveSubClass`, -1)"
        .. " FROM `item_enchantment_random_tiers`")
    if not query then
        return
    end
    repeat
        local id, tier = query:GetUInt32(0), query:GetUInt32(1)
        if ENCHANTS and ENCHANTS[id] and BY_TIER[tier] then
            POOL[id] = { tier = tier, class = query:GetString(2), sub = query:GetInt32(3) }
            table.insert(BY_TIER[tier], id)
        end
    until not query:NextRow()
end
if ENCHANTS then
    load_pool()
end

local function enchant_name(id)
    return ENCHANTS and ENCHANTS[id] and ENCHANTS[id].name or ("enchant " .. id)
end

local function tier_of(id)
    return POOL[id] and POOL[id].tier
end

local function class_name(item)
    local class = item:GetClass()
    return (class == ITEM_CLASS_WEAPON and "WEAPON") or (class == ITEM_CLASS_ARMOR and "ARMOR")
end

local function fits(id, item, player)
    local entry = POOL[id]
    if not entry then
        return false
    end
    if entry.class ~= "ANY" and entry.class ~= class_name(item) then
        return false
    end
    if entry.sub ~= -1 and entry.sub ~= item:GetSubClass() then
        return false
    end
    return ENCHANTS[id].level <= player:GetLevel()
end

-- A random enchant of `tier` that fits the item; `want_family` keeps the same stat.
local function pick(tier, item, player, except, want_family)
    local choices = {}
    for _, id in ipairs(BY_TIER[tier] or {}) do
        if id ~= except and fits(id, item, player)
            and (not want_family or ENCHANTS[id].family == want_family) then
            choices[#choices + 1] = id
        end
    end
    if #choices == 0 then
        return nil
    end
    return choices[math.random(#choices)]
end

-- Charges and the stored enchant, cached per character and written through.
CharDBQuery("CREATE TABLE IF NOT EXISTS `rustwrath_enchant_bottle` ("
    .. "`guid` INT UNSIGNED NOT NULL PRIMARY KEY,"
    .. " `charges` INT UNSIGNED NOT NULL DEFAULT 0,"
    .. " `stored_enchant` INT UNSIGNED NOT NULL DEFAULT 0,"
    .. " `updated_at` TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP"
    .. ") ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='RustWrath Enchant Bottle'")

local STATE = {}

local function state(player)
    local guid = player:GetGUIDLow()
    local s = STATE[guid]
    if not s then
        s = { charges = 0, stored = 0 }
        local query = CharDBQuery(string.format(
            "SELECT `charges`, `stored_enchant` FROM `rustwrath_enchant_bottle` WHERE `guid` = %d", guid))
        if query then
            s.charges, s.stored = query:GetUInt32(0), query:GetUInt32(1)
        end
        STATE[guid] = s
    end
    return s
end

local function save(player, s)
    CharDBExecute(string.format(
        "REPLACE INTO `rustwrath_enchant_bottle` (`guid`, `charges`, `stored_enchant`) VALUES (%d, %d, %d)",
        player:GetGUIDLow(), s.charges, s.stored))
end

local function say(player, text)
    player:SendBroadcastMessage(TAG .. text)
end

-- Fills the bottle from an item's random enchants. Returns the charges gained.
local function harvest(player, item)
    local gained, best = 0, nil
    for _, slot in ipairs(SLOTS) do
        local id = item:GetEnchantmentId(slot)
        local tier = id and id ~= 0 and tier_of(id)
        if tier then
            gained = gained + TIER_VALUE[tier]
            if not best or tier > tier_of(best) then
                best = id
            end
        end
    end
    if gained == 0 then
        return 0
    end
    local s = state(player)
    s.charges = s.charges + gained
    local imprint = ""
    if best then
        imprint = string.format(", imprinted %s (T%d)", enchant_name(best), tier_of(best))
        if s.stored ~= 0 and s.stored ~= best then
            imprint = imprint .. string.format(", replacing %s", enchant_name(s.stored))
        end
        s.stored = best
    end
    save(player, s)
    say(player, string.format("%s: +%d charge%s (%d now)%s.", item:GetName(), gained,
        gained == 1 and "" or "s", s.charges, imprint))
    return gained
end

local function roll_window_tier(quality)
    local base = math.max(quality, 1)
    local r = math.random() * 100
    for _, step in ipairs(TIER_STEPS) do
        r = r - step[2]
        if r < 0 then
            return math.min(base + step[1], MAX_TIER)
        end
    end
    return math.min(base, MAX_TIER)
end

local function awaken_cost(tier)
    return math.ceil(TIER_VALUE[tier] * 1.5)
end

local function equipped(player, equip_slot)
    local item = player:GetEquippedItemBySlot(equip_slot)
    if item and class_name(item) then
        return item
    end
    return nil
end

-- Menus -----------------------------------------------------------------------------------

local function header(player)
    local s = state(player)
    local stored = s.stored ~= 0
        and string.format("%s (T%d)", enchant_name(s.stored), tier_of(s.stored) or 0) or "nothing"
    player:GossipMenuAddItem(0, string.format("Charges: %d. Stored enchant: %s.", s.charges, stored),
        S_ROOT, 0)
end

local function send(player)
    player:GossipSendMenu(NPC_TEXT, player, MENU_ID)
end

local function show_root(player)
    player:GossipClearMenu()
    header(player)
    for equip_slot = 0, 18 do
        local item = equipped(player, equip_slot)
        if item then
            local count = 0
            for _, slot in ipairs(SLOTS) do
                if tier_of(item:GetEnchantmentId(slot) or 0) then
                    count = count + 1
                end
            end
            player:GossipMenuAddItem(3, string.format("%s: %s (%d random enchant%s)",
                EQUIP_NAMES[equip_slot], item:GetName(), count, count == 1 and "" or "s"),
                S_ITEM, equip_slot)
        end
    end
    player:GossipMenuAddItem(6, "Absorb an item from your bags", S_ABSORB_LIST, 0)
    player:GossipMenuAddItem(0, "Close", S_CLOSE, 0)
    send(player)
end

local function show_item(player, equip_slot)
    local item = equipped(player, equip_slot)
    if not item then
        return show_root(player)
    end
    local s = state(player)
    player:GossipClearMenu()
    header(player)
    for si, slot in ipairs(SLOTS) do
        local id = item:GetEnchantmentId(slot) or 0
        local code = equip_slot * 8 + si
        if id == 0 then
            player:GossipMenuAddItem(4, string.format("%s slot empty: Awaken (from %d charges)",
                SLOT_NAMES[si], awaken_cost(1)), S_ACT, A_AWAKEN * 256 + code)
            if s.stored ~= 0 then
                local tier = tier_of(s.stored) or 1
                player:GossipMenuAddItem(4, string.format("%s slot: Pour %s (%d charges)",
                    SLOT_NAMES[si], enchant_name(s.stored), 2 * TIER_VALUE[tier]),
                    S_ACT, A_POUR * 256 + code)
            end
        elseif tier_of(id) then
            player:GossipMenuAddItem(3, string.format("%s: %s (T%d)", SLOT_NAMES[si],
                enchant_name(id), tier_of(id)), S_SLOT, code)
        else
            player:GossipMenuAddItem(0, string.format("%s: %s (not a random enchant)",
                SLOT_NAMES[si], enchant_name(id)), S_ITEM, equip_slot)
        end
    end
    player:GossipMenuAddItem(0, "Back", S_ROOT, 0)
    send(player)
end

local function show_slot(player, code)
    local equip_slot, si = math.floor(code / 8), code % 8
    local item = equipped(player, equip_slot)
    local id = item and SLOTS[si] and item:GetEnchantmentId(SLOTS[si]) or 0
    local tier = tier_of(id)
    if not tier then
        return show_item(player, equip_slot)
    end
    player:GossipClearMenu()
    header(player)
    local value = TIER_VALUE[tier]
    player:GossipMenuAddItem(4, string.format("Reroll %s at T%d (%d charges)",
        enchant_name(id), tier, value), S_ACT, A_REROLL * 256 + code)
    player:GossipMenuAddItem(4, string.format("Gamble: T%d-T%d (%d charges)",
        math.max(tier - 1, 1), math.min(tier + 1, MAX_TIER), value), S_ACT, A_GAMBLE * 256 + code)
    if tier < MAX_TIER then
        player:GossipMenuAddItem(4, string.format("Empower to T%d, same stat (%d charges)",
            tier + 1, 2 * TIER_VALUE[tier + 1]), S_ACT, A_EMPOWER * 256 + code)
    end
    player:GossipMenuAddItem(0, "Back", S_ITEM, equip_slot)
    send(player)
end

local function bag_items(player)
    local items = {}
    local function consider(bag, slot)
        local item = player:GetItemByPos(bag, slot)
        if item and class_name(item) then
            local value = 0
            for _, eslot in ipairs(SLOTS) do
                local tier = tier_of(item:GetEnchantmentId(eslot) or 0)
                if tier then
                    value = value + TIER_VALUE[tier]
                end
            end
            if value > 0 then
                items[#items + 1] = { item = item, bag = bag, slot = slot, value = value }
            end
        end
    end
    for slot = BACKPACK_FIRST, BACKPACK_LAST do
        consider(INVENTORY_SLOT_BAG_0, slot)
    end
    for bag = BAG_FIRST, BAG_LAST do
        for slot = 0, 35 do
            consider(bag, slot)
        end
    end
    return items
end

local function show_absorb_list(player)
    player:GossipClearMenu()
    header(player)
    local items = bag_items(player)
    if #items == 0 then
        player:GossipMenuAddItem(0, "No weapon or armor with random enchants in your bags.",
            S_ABSORB_LIST, 0)
    end
    for i, entry in ipairs(items) do
        if i > 25 then
            break
        end
        local name = entry.item:GetName()
        player:GossipMenuAddItem(6, string.format("Absorb %s (+%d charges)", name, entry.value),
            S_ABSORB, entry.bag * 256 + entry.slot, false,
            string.format("Destroy %s for %d charges?", name, entry.value))
    end
    player:GossipMenuAddItem(0, "Back", S_ROOT, 0)
    send(player)
end

-- Actions ---------------------------------------------------------------------------------

local function pay(player, s, cost)
    if s.charges < cost then
        say(player, string.format("Needs %d charges; the bottle holds %d.", cost, s.charges))
        return false
    end
    s.charges = s.charges - cost
    return true
end

local function apply(player, item, slot, id, s, cost, verb)
    if not item:SetEnchantment(id, slot) then
        s.charges = s.charges + cost
        say(player, "The enchant did not take; charges refunded.")
        return
    end
    item:SaveToDB()
    save(player, s)
    say(player, string.format("%s %s: %s (T%d). %d charge%s left.", verb, item:GetItemLink(),
        enchant_name(id), tier_of(id), s.charges, s.charges == 1 and "" or "s"))
end

local function act(player, intid)
    local action, code = math.floor(intid / 256), intid % 256
    local equip_slot, si = math.floor(code / 8), code % 8
    local item, slot = equipped(player, equip_slot), SLOTS[si]
    if not item or not slot then
        return show_root(player)
    end
    local s = state(player)
    local current = item:GetEnchantmentId(slot) or 0
    local tier = tier_of(current)

    if action == A_REROLL or action == A_GAMBLE or action == A_EMPOWER then
        if not tier then
            return show_item(player, equip_slot)
        end
        local target, cost, want = tier, TIER_VALUE[tier], nil
        if action == A_GAMBLE then
            local r = math.random(100)
            target = (r <= 25 and math.max(tier - 1, 1)) or (r <= 75 and tier) or math.min(tier + 1, MAX_TIER)
        elseif action == A_EMPOWER then
            target, cost, want = tier + 1, 2 * TIER_VALUE[math.min(tier + 1, MAX_TIER)], ENCHANTS[current].family
            if target > MAX_TIER then
                return show_slot(player, code)
            end
        end
        local id = pick(target, item, player, current, want)
        if not id then
            say(player, action == A_EMPOWER
                and string.format("%s has no T%d version for this item.", enchant_name(current), target)
                or string.format("No other T%d enchant fits this item.", target))
            return show_slot(player, code)
        end
        if pay(player, s, cost) then
            apply(player, item, slot, id, s, cost,
                (action == A_REROLL and "Rerolled") or (action == A_GAMBLE and "Gambled") or "Empowered")
        end
        return show_item(player, equip_slot)
    end

    if current ~= 0 then
        say(player, "That slot is no longer empty.")
        return show_item(player, equip_slot)
    end

    if action == A_AWAKEN then
        if s.charges < awaken_cost(1) then
            pay(player, s, awaken_cost(1))
            return show_item(player, equip_slot)
        end
        local target = roll_window_tier(item:GetQuality())
        while target > 1 and awaken_cost(target) > s.charges do
            target = target - 1
        end
        local id = pick(target, item, player)
        if not id then
            say(player, string.format("No T%d enchant fits this item.", target))
            return show_item(player, equip_slot)
        end
        local cost = awaken_cost(target)
        if pay(player, s, cost) then
            apply(player, item, slot, id, s, cost, "Awakened")
        end
        return show_item(player, equip_slot)
    end

    if action == A_POUR then
        local id = s.stored
        if id == 0 or not tier_of(id) then
            return show_item(player, equip_slot)
        end
        if not fits(id, item, player) then
            say(player, string.format("%s does not fit %s%s.", enchant_name(id), item:GetName(),
                ENCHANTS[id].level > player:GetLevel()
                    and string.format(" (needs level %d)", ENCHANTS[id].level) or ""))
            return show_item(player, equip_slot)
        end
        local cost = 2 * TIER_VALUE[tier_of(id)]
        if pay(player, s, cost) then
            s.stored = 0
            apply(player, item, slot, id, s, cost, "Poured into")
        end
        return show_item(player, equip_slot)
    end
    return show_root(player)
end

local function absorb(player, intid)
    local bag, slot = math.floor(intid / 256), intid % 256
    local item = player:GetItemByPos(bag, slot)
    if item and class_name(item) and not item:IsEquipped() then
        if harvest(player, item) > 0 then
            player:RemoveItem(item, 1)
        end
    end
    show_absorb_list(player)
end

-- Hooks -----------------------------------------------------------------------------------

local function on_select(_, player, _, sender, intid)
    if sender == S_CLOSE then
        player:GossipComplete()
    elseif sender == S_ITEM then
        show_item(player, intid)
    elseif sender == S_SLOT then
        show_slot(player, intid)
    elseif sender == S_ACT then
        act(player, intid)
    elseif sender == S_ABSORB_LIST then
        show_absorb_list(player)
    elseif sender == S_ABSORB then
        absorb(player, intid)
    else
        show_root(player)
    end
end

local function on_bottle_use(_, player)
    show_root(player)
    return false -- never cast the stock drink spell
end

local function on_spell_cast(_, player, spell)
    if spell:GetEntry() ~= DISENCHANT then
        return
    end
    local item = spell:GetTarget()
    if item and item:GetObjectType() == "Item" then
        harvest(player, item)
    end
end

local function on_command(_, player, command)
    if not player or tostring(command):lower() ~= "bottle" then
        return
    end
    if player:GetItemCount(BOTTLE_GIVE, true) == 0 and not player:AddItem(BOTTLE_GIVE, 1) then
        say(player, "No bag space for the bottle. The menu opens anyway.")
    end
    show_root(player)
    return false
end

if ENCHANTS then
    for _, entry in ipairs(BOTTLE_ITEMS) do
        -- ALE refuses entries the server has not loaded (the dummy pool until its next start).
        pcall(RegisterItemGossipEvent, entry, GOSSIP_EVENT_ON_HELLO, on_bottle_use)
    end
    RegisterPlayerGossipEvent(MENU_ID, GOSSIP_EVENT_ON_SELECT, on_select)
    RegisterPlayerEvent(PLAYER_EVENT_ON_SPELL_CAST, on_spell_cast)
    RegisterPlayerEvent(PLAYER_EVENT_ON_COMMAND, on_command)
end

local pooled = 0
for t = 1, MAX_TIER do
    pooled = pooled + #(BY_TIER[t] or {})
end
print(string.format("[rustwrath_enchant_bottle] %s; %d pool enchants",
    ENCHANTS and "loaded" or "disabled (no SpellItemEnchantment.dbc)", pooled))
