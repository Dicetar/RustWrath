--[[
RustWrath: random enchants that respect the item's tier and the enchant's level. ALE (Eluna) Lua,
live: no core rebuild, no restart.

Install: copy this file into the server's `run\lua_scripts` folder (ALE auto-reloads it), then
turn the C++ roller off: `RandomEnchants.Enable = 0` in `configs\modules\random_enchants.conf`
and `.reload config`. The module reads that switch on every roll, so the change is live. While
the module is still on, this script does nothing, so an item is never rolled twice.

Why the C++ roller is replaced: `mod-random-enchants` `getRandEnchantment` queries
  WHERE tier=? AND exclusiveSubClass=NULL AND exclusiveSubClass=? OR class=? OR class='ANY'
`= NULL` is never true and AND binds before OR, so the tier is ignored and every item draws from
the whole table, level-60 enchants included (owner report 2026-11-20: a crafted Brown Linen Shirt
rolled Fortitude of the Scourge 2716, which applies nothing below level 60). Nothing checked the
enchant's level either.

This roller keeps the module's behaviour (events, chances, quality-to-tier roll, slots PERM 0,
TEMP 1, BONUS 5, the chat line) with the query fixed:
  WHERE tier=? AND (exclusiveSubClass IS NULL OR exclusiveSubClass=?)
    AND (class=? OR class='ANY') ORDER BY RAND()
and takes the first enchant whose `SpellItemEnchantment.RequiredLevel` is at most the higher of
the player's level and the item's required level. ALE's DBC registry has no SpellItemEnchantment,
so the levels are read once from the server's own `<DataDir>\dbc\SpellItemEnchantment.dbc`.
Items rolled before keep their enchants.
]]

local PLAYER_EVENT_ON_LOOT_ITEM = 32
local PLAYER_EVENT_ON_QUEST_REWARD_ITEM = 51
local PLAYER_EVENT_ON_CREATE_ITEM = 52
local PLAYER_EVENT_ON_GROUP_ROLL_REWARD_ITEM = 56

local ITEM_CLASS_WEAPON = 2
local ITEM_CLASS_ARMOR = 4
local QUALITY_NORMAL, QUALITY_LEGENDARY = 1, 5
local SLOTS = { 0, 1, 5 } -- PERM_ENCHANTMENT_SLOT, TEMP_ENCHANTMENT_SLOT, BONUS_ENCHANTMENT_SLOT

-- SpellItemEnchantment.dbc (3.3.5a): 38 uint32 columns; column 0 is the id, 37 RequiredLevel.
local function u32(data, at)
    local a, b, c, d = data:byte(at + 1, at + 4)
    return a + b * 0x100 + c * 0x10000 + d * 0x1000000
end

local function load_enchant_levels()
    local dir = tostring(GetConfigValue("DataDir") or ""):gsub("[\\/]+$", "")
    local path = dir .. "/dbc/SpellItemEnchantment.dbc"
    local file = io.open(path, "rb")
    if not file then
        print("[rustwrath_random_enchants] cannot open " .. path .. "; roller disabled")
        return nil
    end
    local data = file:read("*a")
    file:close()
    if data:sub(1, 4) ~= "WDBC" then
        print("[rustwrath_random_enchants] " .. path .. " is not a WDBC file; roller disabled")
        return nil
    end
    local records, fields, size = u32(data, 4), u32(data, 8), u32(data, 12)
    if fields ~= 38 or size ~= 152 then
        print(string.format("[rustwrath_random_enchants] unexpected layout %d x %d; roller disabled",
            fields, size))
        return nil
    end
    local levels = {}
    for i = 0, records - 1 do
        local at = 20 + i * size
        levels[u32(data, at)] = u32(data, at + 37 * 4)
    end
    return levels
end

local ENCHANT_LEVEL = load_enchant_levels()

local function config_on(name, default)
    local value = GetConfigValue(name)
    if value == nil or value == "" then
        return default
    end
    return value ~= false and value ~= 0 and value ~= "0"
end

local function chance(name, default)
    return tonumber(GetConfigValue(name)) or default
end

-- The module's quality roll (`getRandEnchantment`): a number on a 0-93 scale, then a tier.
local function roll_tier(quality)
    local r
    if quality == 1 then
        r = math.floor(math.random() * 50)
    elseif quality == 2 then
        r = math.floor(45 + math.random() * 20)
    elseif quality == 3 then
        r = math.floor(65 + math.random() * 15)
    elseif quality == 4 then
        r = math.floor(80 + math.random() * 14)
    elseif quality == 5 then
        r = 93
    else
        return nil
    end
    if r <= 44 then return 1 end
    if r <= 64 then return 2 end
    if r <= 79 then return 3 end
    if r <= 92 then return 4 end
    return 5
end

local function pick_enchant(item, class_name, max_level)
    local tier = roll_tier(item:GetQuality())
    if not tier then
        return nil
    end
    local query = WorldDBQuery(string.format(
        "SELECT `enchantID` FROM `item_enchantment_random_tiers` WHERE `tier` = %d"
            .. " AND (`exclusiveSubClass` IS NULL OR `exclusiveSubClass` = %d)"
            .. " AND (`class` = '%s' OR `class` = 'ANY') ORDER BY RAND()",
        tier, item:GetSubClass(), class_name))
    if not query then
        return nil
    end
    repeat
        local id = query:GetUInt32(0)
        local level = ENCHANT_LEVEL[id]
        if level and level <= max_level then
            return id
        end
    until not query:NextRow()
    return nil
end

local function roll(player, item)
    if not ENCHANT_LEVEL or config_on("RandomEnchants.Enable", true) then
        return
    end
    local quality, class = item:GetQuality(), item:GetClass()
    if quality < QUALITY_NORMAL or quality > QUALITY_LEGENDARY then
        return
    end
    local class_name = (class == ITEM_CLASS_WEAPON and "WEAPON") or (class == ITEM_CLASS_ARMOR and "ARMOR")
    if not class_name then
        return
    end
    local max_level = math.max(player:GetLevel(), item:GetRequiredLevel())
    local chances = {
        chance("RandomEnchants.EnchantChance1", 70),
        chance("RandomEnchants.EnchantChance2", 65),
        chance("RandomEnchants.EnchantChance3", 60),
    }
    local applied = 0
    for i, slot in ipairs(SLOTS) do
        if math.random() * 100 >= chances[i] then
            break
        end
        local id = pick_enchant(item, class_name, max_level)
        if not id or not item:SetEnchantment(id, slot) then
            break
        end
        applied = applied + 1
    end
    if applied > 0 then
        player:SendBroadcastMessage(string.format(
            "Newly Acquired |cffFF0000 %s |rhas received|cffFF0000 %d |rrandom enchantment%s!",
            item:GetName(), applied, applied > 1 and "s" or ""))
    end
end

local function on_event(setting)
    return function(_, player, item)
        if player and item and config_on(setting, true) then
            roll(player, item)
        end
    end
end

RegisterPlayerEvent(PLAYER_EVENT_ON_LOOT_ITEM, on_event("RandomEnchants.OnLoot"))
RegisterPlayerEvent(PLAYER_EVENT_ON_CREATE_ITEM, on_event("RandomEnchants.OnCreate"))
RegisterPlayerEvent(PLAYER_EVENT_ON_QUEST_REWARD_ITEM, on_event("RandomEnchants.OnQuestReward"))
RegisterPlayerEvent(PLAYER_EVENT_ON_GROUP_ROLL_REWARD_ITEM, on_event("RandomEnchants.OnGroupRoll"))

print(string.format("[rustwrath_random_enchants] loaded; %s",
    not ENCHANT_LEVEL and "disabled (no enchant levels)"
        or config_on("RandomEnchants.Enable", true)
            and "idle until RandomEnchants.Enable = 0 and .reload config"
        or "rolling"))
