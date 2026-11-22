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

Spending charges (random-pool enchants only; the RustWrath window takes a worn item or one from
the bags, the gossip menu worn items only)
- Reroll: a random enchant of the same tier. Cost: the tier value.
- Gamble: 25 % one tier down, 50 % the same tier, 25 % one tier up. Cost: the tier value.
- Empower: the same stat one tier up (same effect types and stats in SpellItemEnchantment.dbc).
  Cost: 2x the new tier's value.
- Awaken: fill an empty slot with a tier rolled from the item's quality window (as the loot
  roller does), lowered to the best tier your charges pay for. Cost: 1.5x the tier value.
- Pour: put the stored enchant into an empty slot. Cost: 2x its tier value.
Tiers go past the item's window up to 5 on any item. Results never use an enchant whose
required level is above the player's level, since the core applies nothing below it.

Opening the bottle: the RustWrath client opens its own window when the bottle item is used (or on
`/bottle`), without sending the use; the window talks to this script over addon whispers to
oneself under the prefix RWBOTTLE (see "The RustWrath window" below). The stock item's spell is a
ground-targeted "set out a bottle", so a stock client shows a placement circle and only reaches
this script once a spot is clicked; it then gets the gossip menu, as does `.bottle` (which gives
you the bottle if you have none). A client that said HELLO gets its window instead. The bottle reuses
stock item 44625 "Bottle of Aged Dalaran Red" (new item_template rows load only at server
start); dummy pool item 800000 is registered too for after the next start.

Storage: characters DB table `rustwrath_enchant_bottle` (created on load if missing).
]]

local BOTTLE_ITEMS = { 44625, 800000 }
local BOTTLE_GIVE = 44625
local MENU_ID = 920700 -- player gossip menu id for the bottle
local NPC_TEXT = 1 -- stock "Greetings" header text

local DISENCHANT = 13262
local PLAYER_EVENT_ON_LOGIN = 3
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

-- The RustWrath client's window channel (`crates/benilla-app/assets/ui/EnchantBottle.xml`).
local PREFIX = "RWBOTTLE"
local ADDON_EVENT_ON_MESSAGE = 30
local CHAT_MSG_WHISPER = 7

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
-- Characters whose client has the RustWrath window (it said HELLO this session): the bottle and
-- `.bottle` open that window for them instead of the gossip menu.
local WINDOW = {}

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

-- One line to the RustWrath window: an addon whisper to oneself (see "The RustWrath window").
local function panel(player, line)
    player:SendAddonMessage(PREFIX, line, CHAT_MSG_WHISPER, player)
end

-- A line for the window: links and colours stripped, cut under the 255-byte chat cap.
local function plain(text)
    return (text:gsub("|H.-|h(.-)|h", "%1"):gsub("|c%x%x%x%x%x%x%x%x", ""):gsub("|r", "")):sub(1, 220)
end

local function say(player, text)
    player:SendBroadcastMessage(TAG .. text)
end

-- Said in chat and to an open window, which refreshes: a result outside the window (a disenchant).
local function notify(player, text)
    say(player, text)
    panel(player, "MSG\t" .. plain(text))
end

local function charges_text(n)
    return string.format("%d charge%s", n, n == 1 and "" or "s")
end

-- The charges an item's random enchants are worth, and its best one.
local function item_value(item)
    local value, best = 0, nil
    for _, slot in ipairs(SLOTS) do
        local id = item:GetEnchantmentId(slot)
        local tier = id and id ~= 0 and tier_of(id)
        if tier then
            value = value + TIER_VALUE[tier]
            if not best or tier > tier_of(best) then
                best = id
            end
        end
    end
    return value, best
end

-- Fills the bottle from an item's random enchants. Returns the charges gained and the line.
local function harvest(player, item)
    local gained, best = item_value(item)
    if gained == 0 then
        return 0, string.format("%s holds no random enchant.", item:GetName())
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
    return gained, string.format("%s: +%d charge%s (%d now)%s.", item:GetName(), gained,
        gained == 1 and "" or "s", s.charges, imprint)
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

-- Positions the bottle works on, in the core's numbering: the worn gear (bag 255, slots 0-18),
-- the backpack (255, 23-38) and the four equipped bags (19-22, from 0). Never the bank.
local function valid_position(bag, slot)
    if bag == INVENTORY_SLOT_BAG_0 then
        return (slot >= 0 and slot <= 18) or (slot >= BACKPACK_FIRST and slot <= BACKPACK_LAST)
    end
    return bag >= BAG_FIRST and bag <= BAG_LAST and slot >= 0 and slot <= 35
end

local function is_worn(bag, slot)
    return bag == INVENTORY_SLOT_BAG_0 and slot <= 18
end

-- A weapon or armor piece of the player's at a valid position, else nil.
local function item_at(player, bag, slot)
    if not valid_position(bag, slot) then
        return nil
    end
    local item = player:GetItemByPos(bag, slot)
    if item and class_name(item) then
        return item
    end
    return nil
end

local function equipped(player, equip_slot)
    return item_at(player, INVENTORY_SLOT_BAG_0, equip_slot)
end

-- Actions ---------------------------------------------------------------------------------

-- One spend on enchant slot `si` (1-3) of `item`, worn or in the bags (the core applies an
-- enchant's stats only while the item is worn, so a bag item takes them when it is put on).
-- Returns whether it took and the line that says what happened.
local function act_on(player, item, action, si)
    local slot = SLOTS[si]
    if not slot then
        return false, "No such enchant slot."
    end
    local s = state(player)
    local current = item:GetEnchantmentId(slot) or 0
    local tier = tier_of(current)

    local function pay(cost)
        if s.charges < cost then
            return false, string.format("Needs %s; the bottle holds %d.", charges_text(cost), s.charges)
        end
        s.charges = s.charges - cost
        return true
    end

    local function apply(id, cost, verb)
        if not item:SetEnchantment(id, slot) then
            s.charges = s.charges + cost
            return false, "The enchant did not take; charges refunded."
        end
        item:SaveToDB()
        save(player, s)
        return true, string.format("%s %s: %s (T%d). %s left.", verb, item:GetItemLink(),
            enchant_name(id), tier_of(id), charges_text(s.charges))
    end

    if action == A_REROLL or action == A_GAMBLE or action == A_EMPOWER then
        if not tier then
            return false, string.format("The %s slot holds no random enchant.", SLOT_NAMES[si]:lower())
        end
        local target, cost, want = tier, TIER_VALUE[tier], nil
        if action == A_GAMBLE then
            local r = math.random(100)
            target = (r <= 25 and math.max(tier - 1, 1)) or (r <= 75 and tier) or math.min(tier + 1, MAX_TIER)
        elseif action == A_EMPOWER then
            if tier >= MAX_TIER then
                return false, string.format("%s is already T%d, the highest tier.", enchant_name(current), tier)
            end
            target, cost, want = tier + 1, 2 * TIER_VALUE[tier + 1], ENCHANTS[current].family
        end
        local id = pick(target, item, player, current, want)
        if not id then
            return false, action == A_EMPOWER
                and string.format("%s has no T%d version for this item.", enchant_name(current), target)
                or string.format("No other T%d enchant fits this item.", target)
        end
        local paid, refusal = pay(cost)
        if not paid then
            return false, refusal
        end
        return apply(id, cost,
            (action == A_REROLL and "Rerolled") or (action == A_GAMBLE and "Gambled") or "Empowered")
    end

    if action ~= A_AWAKEN and action ~= A_POUR then
        return false, "Unknown action."
    end
    if current ~= 0 then
        return false, string.format("The %s slot is not empty.", SLOT_NAMES[si]:lower())
    end

    if action == A_AWAKEN then
        if s.charges < awaken_cost(1) then
            return pay(awaken_cost(1))
        end
        local target = roll_window_tier(item:GetQuality())
        while target > 1 and awaken_cost(target) > s.charges do
            target = target - 1
        end
        local id = pick(target, item, player)
        if not id then
            return false, string.format("No T%d enchant fits this item.", target)
        end
        local cost = awaken_cost(target)
        local paid, refusal = pay(cost)
        if not paid then
            return false, refusal
        end
        return apply(id, cost, "Awakened")
    end

    -- A_POUR
    local id = s.stored
    if id == 0 or not tier_of(id) then
        return false, "The bottle holds no stored enchant."
    end
    if not fits(id, item, player) then
        return false, string.format("%s does not fit %s%s.", enchant_name(id), item:GetName(),
            ENCHANTS[id].level > player:GetLevel()
                and string.format(" (needs level %d)", ENCHANTS[id].level) or "")
    end
    local cost = 2 * TIER_VALUE[tier_of(id)]
    local paid, refusal = pay(cost)
    if not paid then
        return false, refusal
    end
    s.stored = 0
    local ok, text = apply(id, cost, "Poured into")
    if not ok then
        s.stored = id
    end
    return ok, text
end

-- Destroys a bag item for its charges. Returns whether it did and the line.
local function absorb(player, bag, slot)
    if is_worn(bag, slot) then
        return false, "Take the item off first: the bottle absorbs items from your bags."
    end
    local item = item_at(player, bag, slot)
    if not item then
        return false, "Only a weapon or armor piece from your bags can be absorbed."
    end
    local gained, text = harvest(player, item)
    if gained == 0 then
        return false, text
    end
    player:RemoveItem(item, 1)
    return true, text
end

-- Menus (stock clients) -------------------------------------------------------------------

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
        local item = item_at(player, bag, slot)
        if item then
            local value = item_value(item)
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

-- The gossip spend: an equipped item's slot, encoded as action * 256 + equip slot * 8 + 1-3.
local function act(player, intid)
    local action, code = math.floor(intid / 256), intid % 256
    local equip_slot, si = math.floor(code / 8), code % 8
    local item = equipped(player, equip_slot)
    if not item or not SLOTS[si] then
        return "root"
    end
    local ok, text = act_on(player, item, action, si)
    say(player, text)
    if not ok and (action == A_REROLL or action == A_GAMBLE or action == A_EMPOWER) then
        return "slot", code
    end
    return "item", equip_slot
end

-- The RustWrath window --------------------------------------------------------------------
-- The client's own window (`crates/benilla-app/assets/ui/EnchantBottle.xml`), fed over addon
-- whispers to oneself under PREFIX, TAB-separated. Positions are the core's: bag 255 with slots
-- 0-18 (worn) or 23-38 (backpack), bags 19-22 with slots from 0.
-- Client to server:
--   HELLO                                  this client has the window; also answered with STATE
--   OPEN                                   the window opened or wants a refresh: STATE
--   PICK <bag> <slot>                      an item was put in the window's slot: ITEM
--   APPLY <action> <bag> <slot> <1-3>      A_REROLL..A_POUR on one enchant slot of that item, or
--                                          6 (absorb) on a bag item: RESULT, STATE, ITEM
-- Server to client:
--   SHOW                                   open the window (the bottle was used, or `.bottle`)
--   STATE <charges> <stored id> <stored tier> <pour cost> <awaken cost> <stored name>
--   ITEM <bag> <slot> <entry> <usable 0/1> <absorb value> then, for enchant slots 1-3,
--        <id> <tier> <reroll cost> <empower cost> <name> (id 0: empty; tier 0: not a random
--        enchant; cost 0: none). Usable 0: no weapon or armor of yours there; nothing follows.
--   RESULT <ok 0/1> <text>                 what an APPLY did, or why it was refused
--   MSG <text>                             a result outside the window (a disenchant)
-- The server stays authoritative: every field is checked again here.
local A_ABSORB = 6

local function send_state(player)
    local s = state(player)
    local stored_tier = tier_of(s.stored) or 0
    panel(player, table.concat({ "STATE", s.charges, s.stored, stored_tier,
        stored_tier > 0 and 2 * TIER_VALUE[stored_tier] or 0, awaken_cost(1),
        s.stored ~= 0 and enchant_name(s.stored):sub(1, 60) or "" }, "\t"))
end

local function send_item(player, bag, slot)
    local item = item_at(player, bag, slot)
    if not item then
        panel(player, table.concat({ "ITEM", bag, slot, 0, 0, 0 }, "\t"))
        return
    end
    local fields = { "ITEM", bag, slot, item:GetEntry(), 1, (item_value(item)) }
    for _, eslot in ipairs(SLOTS) do
        local id = item:GetEnchantmentId(eslot) or 0
        local tier = tier_of(id) or 0
        fields[#fields + 1] = id
        fields[#fields + 1] = tier
        fields[#fields + 1] = tier > 0 and TIER_VALUE[tier] or 0
        fields[#fields + 1] = (tier > 0 and tier < MAX_TIER) and 2 * TIER_VALUE[tier + 1] or 0
        fields[#fields + 1] = id ~= 0 and enchant_name(id):sub(1, 40) or ""
    end
    panel(player, table.concat(fields, "\t"))
end

local function split(text)
    local fields = {}
    for field in (text .. "\t"):gmatch("([^\t]*)\t") do
        fields[#fields + 1] = field
    end
    return fields
end

local function on_addon(_, sender, msg_type, prefix, msg, target)
    if prefix ~= PREFIX then
        return
    end
    -- Only a whisper to oneself is ours; anything else with the prefix is dropped.
    if msg_type ~= CHAT_MSG_WHISPER or not target or not sender
        or target:GetGUIDLow() ~= sender:GetGUIDLow() then
        return false
    end
    WINDOW[sender:GetGUIDLow()] = true
    local fields = split(msg or "")
    local verb = fields[1]
    if verb == "HELLO" or verb == "OPEN" then
        send_state(sender)
    elseif verb == "PICK" then
        local bag, slot = tonumber(fields[2]), tonumber(fields[3])
        if bag and slot then
            send_item(sender, bag, slot)
        end
    elseif verb == "APPLY" then
        local action, bag, slot, si = tonumber(fields[2]), tonumber(fields[3]), tonumber(fields[4]),
            tonumber(fields[5])
        if action and bag and slot then
            local ok, text
            if action == A_ABSORB then
                ok, text = absorb(sender, bag, slot)
            elseif action >= A_REROLL and action <= A_POUR and si then
                local item = item_at(sender, bag, slot)
                if item then
                    ok, text = act_on(sender, item, action, si)
                else
                    ok, text = false, "That item is no longer there."
                end
            else
                ok, text = false, "Unknown action."
            end
            if ok then
                say(sender, text)
            end
            panel(sender, "RESULT\t" .. (ok and 1 or 0) .. "\t" .. plain(text))
            send_state(sender)
            send_item(sender, bag, slot)
        end
    end
    -- Our own requests never come back to the client as whispers.
    return false
end

-- Opens the RustWrath window if this client has one, else the gossip menu.
local function open_bottle(player)
    if WINDOW[player:GetGUIDLow()] then
        panel(player, "SHOW")
        send_state(player)
    else
        show_root(player)
    end
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
        local view, arg = act(player, intid)
        if view == "slot" then
            show_slot(player, arg)
        elseif view == "item" then
            show_item(player, arg)
        else
            show_root(player)
        end
    elseif sender == S_ABSORB_LIST then
        show_absorb_list(player)
    elseif sender == S_ABSORB then
        local _, text = absorb(player, math.floor(intid / 256), intid % 256)
        say(player, text)
        show_absorb_list(player)
    else
        show_root(player)
    end
end

local function on_bottle_use(_, player)
    open_bottle(player)
    return false -- never cast the stock spell
end

local function on_spell_cast(_, player, spell)
    if spell:GetEntry() ~= DISENCHANT then
        return
    end
    local item = spell:GetTarget()
    if item and item:GetObjectType() == "Item" then
        local gained, text = harvest(player, item)
        if gained > 0 then
            notify(player, text)
        end
    end
end

local function on_command(_, player, command)
    if not player or tostring(command):lower() ~= "bottle" then
        return
    end
    if player:GetItemCount(BOTTLE_GIVE, true) == 0 and not player:AddItem(BOTTLE_GIVE, 1) then
        say(player, "No bag space for the bottle. The bottle opens anyway.")
    end
    open_bottle(player)
    return false
end

-- A new session says HELLO again if its client has the window.
local function on_login(_, player)
    WINDOW[player:GetGUIDLow()] = nil
end

if ENCHANTS then
    for _, entry in ipairs(BOTTLE_ITEMS) do
        -- ALE refuses entries the server has not loaded (the dummy pool until its next start).
        pcall(RegisterItemGossipEvent, entry, GOSSIP_EVENT_ON_HELLO, on_bottle_use)
    end
    RegisterPlayerGossipEvent(MENU_ID, GOSSIP_EVENT_ON_SELECT, on_select)
    RegisterPlayerEvent(PLAYER_EVENT_ON_LOGIN, on_login)
    RegisterPlayerEvent(PLAYER_EVENT_ON_SPELL_CAST, on_spell_cast)
    RegisterPlayerEvent(PLAYER_EVENT_ON_COMMAND, on_command)
    RegisterServerEvent(ADDON_EVENT_ON_MESSAGE, on_addon)
end

local pooled = 0
for t = 1, MAX_TIER do
    pooled = pooled + #(BY_TIER[t] or {})
end
print(string.format("[rustwrath_enchant_bottle] %s; %d pool enchants",
    ENCHANTS and "loaded" or "disabled (no SpellItemEnchantment.dbc)", pooled))
