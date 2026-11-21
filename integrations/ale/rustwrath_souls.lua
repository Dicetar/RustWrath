--[[
RustWrath Souls combat: the server half, as ALE (Eluna) Lua. No core rebuild.

Install: copy this file into the server's `run\lua_scripts` folder. ALE auto-reloads it within a
second; `.reload ALE` does the same. Remove the file to turn the whole module off.

The client decides its own hits and its own defence (owner decisions 2026-11-14 §3, items 11-16);
this script applies the results through stock damage and turns enemy melee into telegraphed
strikes. Everything here acts only for players whose client said `MODE 1` (F10 on), and a
creature is changed only while it fights one of them. Turning F10 off, logging out or reloading
the script hands every creature back to stock melee.

Channel: addon whispers to oneself (LANG_ADDON, prefix RWSOULS); the server swallows them, and
answers with addon whispers from the player to the player. Fields are tab-separated.

Client -> server
  HELLO                       -> HELLO <version>
  MODE <1|0>                  -> MODE <1|0>   (also the client's keepalive)
  SPRINT <1|0>                   raise / restore the run speed
  CROUCH <1|0>                   lower / restore the run speed; the crouch pose for others; out of
                                combat, crouch stealth (see "Crouch stealth" below)
  ROLL <rate>                    run (forward and back) at <rate> times the run for a roll; 0 ends it
  HIT <mv%> <guid,guid,...> [hands]   one hit window's struck units, nearest first; Elden Ring
                                damage with cleave (hands 2: a one-handed weapon held in both)
  SWING <action>                 relayed to Souls players nearby as SWING <guid> <action>
  VERDICT <id> <hit|guard|parry|dodge|miss>   the client's judgement of strike <id>

Server -> client
  STRIKE <id> <creature guid> <windup ms> <reach yd, centre to centre> <1 if you judge it|0>
  RESOLVE <id> <outcome>
  PARRIED <creature guid> <riposte ms>
  BROKEN <creature guid> <riposte ms>   its stance broke (to every Souls player nearby)
  SWING <player guid> <action>
  EYE <0 standing|1 hidden|2 crouched, seen> <suspicion 0-100> <creature guid|0>   the eye meter
  SNEAK <creature guid>          a sneak attack landed on it

ESTIMATE values are marked; tune them in play.
]]

local PREFIX = "RWSOULS"
local VERSION = 1
local CHAT_MSG_WHISPER = 7
local CMSG_ATTACKSWING = 0x141
local PACKET_EVENT_ON_PACKET_RECEIVE = 5
local ADDON_EVENT_ON_MESSAGE = 30
local ALE_EVENT_ON_LUA_STATE_CLOSE = 16
local PLAYER_EVENT_ON_LOGOUT = 4

-- Update fields (AzerothCore UpdateFields.h, OBJECT_END = 6).
local UNIT_FIELD_FLAGS = 6 + 0x35
local UNIT_FIELD_BASEATTACKTIME = 6 + 0x38
local UNIT_FIELD_COMBATREACH = 6 + 0x3C
local UNIT_FIELD_MINDAMAGE = 6 + 0x40
local UNIT_FIELD_MAXDAMAGE = 6 + 0x41
local UNIT_FIELD_BYTES_1 = 6 + 0x44
local UNIT_FIELD_RESISTANCES = 6 + 0x5D  -- the first is armour
local UNIT_FLAG_PACIFIED = 0x00020000
local BYTES_1_VIS_FLAG = 2          -- UNIT_BYTES_1_OFFSET_VIS_FLAG
local STAND_FLAG_CREEP = 0x02       -- UNIT_STAND_FLAGS_CREEP (the stealth crouch pose)

local MOVE_RUN = 1
local MOVE_RUN_BACK = 2
local BASE_RUN_SPEED = 7.0
local BASE_RUN_BACK_SPEED = 4.5
local SLOT_MAINHAND = 15
local SLOT_OFFHAND = 16
local ITEM_CLASS_WEAPON = 2
local ITEM_CLASS_ARMOR = 4
local INVTYPE_2HWEAPON = 17
local ARMOR_SUBCLASS_SHIELD = 6
local SLOT_RANGED = 17
-- The equipment load the client picks its roll tier from: head, shoulders, chest, waist, legs,
-- feet, wrists, hands score their armour type (cloth 1, leather 2, mail 3, plate 4)...
local LOAD_ARMOR_SLOTS = { 0, 2, 4, 5, 6, 7, 8, 9 }
local LOAD_BY_ARMOR = { [1] = 1, [2] = 2, [3] = 3, [4] = 4 }
-- ...each hand its weapon (mace 2, axe 1, a two-hander 1 more; a shield 1), the ranged slot a gun 1.
local LOAD_BY_WEAPON = { [0] = 1, [1] = 1, [4] = 2, [5] = 2 }
local WEAPON_SUBCLASS_GUN = 3
local EMOTE_ONESHOT_WOUND = 33
local EMOTE_ONESHOT_WOUND_CRITICAL = 34
local EMOTE_STATE_NONE = 0
local EMOTE_STATE_STUN = 64

local TICK_MS = 100
local SPRINT_RATE = 1.4             -- ESTIMATE: ER's sprint over its run
local CROUCH_RATE = 0.5             -- ESTIMATE: ER's crouch walk over its run
local ROLL_RATE_MIN, ROLL_RATE_MAX = 0.5, 3.0   -- the client's roll rate, kept sane
local WINDUP_SHARE = 0.4            -- decision 16: 40 % of the attack time...
local WINDUP_MIN_MS = 600           -- ...clamped 0.6-1.5 s
local WINDUP_MAX_MS = 1500
local START_SLACK_YARDS = 0.3       -- a strike starts (and reaches) this far past the melee range
local VERDICT_GRACE_MS = 500        -- wait this long past the windup for the client's verdict
local CONE_HALF_RADIANS = math.pi / 3   -- decision 16: a 120 degree frontal cone
local GUARD_TAKEN_NO_SHIELD = 0.4   -- ESTIMATE: a weapon guard lets 40 % through; a shield none
local PARRY_STAGGER_MS = 2500       -- ESTIMATE: a parried creature's next strike waits this long
local RIPOSTE_MS = 3000             -- ESTIMATE: a hit this soon after a parry is a riposte...
local RIPOSTE_MULTIPLIER = 3.0      -- ...for this many times the damage
-- Elden Ring damage per hit (see `defended` and on_hit): the weapon's roll times the attack's
-- motion value, the grip and counter bonuses, then the defense curve.
local TWO_HAND_BONUS = 1.15         -- ESTIMATE: ER's two-handed Str x1.5 on a Str weapon's AR
local COUNTER_BONUS = 1.15          -- ESTIMATE: ER's counter hit, landed during a windup
local ARMOR_CAP = 0.75              -- WoW's armour reduction cap
-- Stance (ER's enemy poise): each hit costs the creature stance; at zero it breaks, staggering it
-- and opening a critical (riposte) window. It refills a while after the last hit.
local STANCE_BASE = 30              -- ESTIMATE: a creature's stance is this...
local STANCE_PER_LEVEL = 2          -- ...plus this a level...
local STANCE_ELITE_MULTIPLIER = 2   -- ...times this for an elite
local STANCE_PER_MOTION = 10        -- ESTIMATE: stance damage of a hit at 100 % motion value
local STANCE_REFILL_MS = 8000       -- ESTIMATE: stance refills this long after the last hit
local STANCE_BREAK_MS = 3000        -- ESTIMATE: a broken stance staggers and opens a riposte this long
local HIT_REACH_YARDS = 12         -- a reported unit farther than this is refused
local HITS_PER_SECOND = 12          -- a client sending more is throttled
local MAX_TARGETS_REPORTED = 8
local SWING_RELAY_YARDS = 60
local MANAGE_RANGE_YARDS = 40
-- Trace every managed creature's turn into the server log (PrintInfo: Server.log). On while the
-- enemy strike cadence is being tuned; set false to quiet it.
local TRACE = true
local TRACE_HELD_MS = 2000          -- a creature held from striking is traced at most this often

-- Decision 6: cleave falloff c and the target cap per main-hand weapon subclass.
local CLEAVE = {
    [15] = { 0.25, 2 },  -- dagger
    [13] = { 0.30, 2 },  -- fist
    [4]  = { 0.40, 3 },  -- 1H mace
    [7]  = { 0.50, 3 },  -- 1H sword
    [0]  = { 0.50, 3 },  -- 1H axe
    [10] = { 0.50, 4 },  -- staff
    [6]  = { 0.60, 4 },  -- polearm
    [5]  = { 0.65, 4 },  -- 2H mace
    [8]  = { 0.75, 5 },  -- 2H sword
    [1]  = { 0.75, 5 },  -- 2H axe
    [3]  = { 0.35, 3 },  -- gun
}
local SINGLE_TARGET = { [2] = true, [16] = true, [18] = true, [19] = true }  -- bow, thrown, crossbow, wand
local UNARMED = { 0.30, 2 }

-- Players in Souls mode, by guid string: { guid, sprint_rate, hit_second, hits }.
local souls = {}
-- Creatures this script drives, by guid string: { guid, map, instance, next_at, strike, parried_until }.
local managed = {}
local next_strike_id = 1

local function key(guid)
    return tostring(guid)
end

local function send(player, text)
    player:SendAddonMessage(PREFIX, text, CHAT_MSG_WHISPER, player)
end

local function split(text, separator)
    local out = {}
    for part in string.gmatch(text or "", "[^" .. separator .. "]+") do
        out[#out + 1] = part
    end
    return out
end

local function now_ms()
    return GetCurrTime()
end

local function trace(...)
    if TRACE then
        PrintInfo("[souls] " .. string.format(...))
    end
end

local function angle_diff(a, b)
    local d = (a - b) % (2 * math.pi)
    if d > math.pi then
        d = d - 2 * math.pi
    end
    return math.abs(d)
end

-- A unit's stock weapon hit: its main-hand roll, attack power included.
local function weapon_roll(unit)
    local lo = unit:GetFloatValue(UNIT_FIELD_MINDAMAGE) or 0
    local hi = unit:GetFloatValue(UNIT_FIELD_MAXDAMAGE) or lo
    if hi < lo then
        hi = lo
    end
    return lo + math.random() * (hi - lo)
end

-- A Souls swing replaces the stock auto-attack, so it earns the auto-attack's rage: AC's
-- `Unit::RewardRage(damage, weaponSpeedHitFactor, true)` (Unit.cpp), with the speed factor
-- `DealMeleeDamage` gives a main-hand hit (attack time in s x 3.5, doubled on a critical).
-- `Unit:DealDamage` passes no CleanDamage, so AC itself grants none for these hits.
local POWER_RAGE = 1
local RAGE_INCOME_RATE = 2           -- worldserver.conf Rate.Rage.Income
local ENDLESS_RAGE = 29623           -- the one SPELL_AURA_MOD_RAGE_FROM_DAMAGE_DEALT talent, +25 %
local function reward_rage(player, damage, critical)
    if damage <= 0 or player:GetPowerType() ~= POWER_RAGE then
        return
    end
    local level = player:GetLevel()
    local conversion = 0.0091107836 * level * level + 3.225598133 * level + 4.2652911
    if level > 70 then
        conversion = conversion + 13.27 * (level - 70)
    end
    local speed = math.floor((player:GetUInt32Value(UNIT_FIELD_BASEATTACKTIME) or 2000) / 1000 * 3.5)
    if critical then
        speed = speed * 2
    end
    local from_damage = damage / conversion * 7.5
    local rage = math.min((from_damage + speed) / 2, from_damage * 2)
    if player:HasAura(ENDLESS_RAGE) then
        rage = rage * 1.25
    end
    player:ModifyPower(math.floor(rage * RAGE_INCOME_RATE * 10), POWER_RAGE)
end

-- A queued on-next-swing strike (Heroic Strike, Cleave, Maul, Raptor Strike) replaces the swing,
-- as in `Unit::AttackerStateUpdate` (Unit.cpp). Souls mode swallows CMSG_ATTACKSWING, so the
-- server's own swing never comes to fire it. `Spell:Cast(false)` runs the same `CheckCast` and the
-- same failure handling (cast result, interrupt, finish). With too little power the strike drops
-- and the swing lands as a plain hit (`SPELL_FAILED_NO_POWER`); any other failure takes the swing
-- with it. Returns true when the swing was replaced.
local CURRENT_MELEE_SPELL = 0
local function queued_strike_replaces_swing(player)
    local spell = player:GetCurrentSpell(CURRENT_MELEE_SPELL)
    if not spell then
        return false
    end
    local id = spell:GetEntry()
    local entry = LookupEntry("Spell", id)
    local power = entry and entry:GetPowerType() or player:GetPowerType()
    local short = player:GetPower(power) < spell:GetPowerCost()
    spell:Cast(false)
    trace("queued strike %d: %s", id, short and "no power, plain swing" or "cast")
    return not short
end

-- The unit's main-hand roll range midpoint: the size of a 100 % motion value hit.
local function weapon_mean(unit)
    local lo = unit:GetFloatValue(UNIT_FIELD_MINDAMAGE) or 0
    local hi = unit:GetFloatValue(UNIT_FIELD_MAXDAMAGE) or lo
    return math.max(1, (lo + math.max(lo, hi)) / 2)
end

-- Elden Ring's defense curve: the share of an attack of `ratio` times the defense that lands.
-- 90 % from eight times the defense up, 10 % below an eighth, quadratic between.
local function er_defense_share(ratio)
    if ratio >= 8 then
        return 0.9
    elseif ratio >= 2.5 then
        return 0.9 - 0.8 / 121 * (ratio - 8) ^ 2
    elseif ratio >= 1 then
        return 0.7 - 0.4 / 3 * (ratio - 2.5) ^ 2
    elseif ratio >= 0.125 then
        return 0.1 + 0.3 * ((ratio - 0.125) / 0.875) ^ 2
    end
    return 0.1
end

-- The curve's inverse on [0.1, 0.9]: the attack-to-defense ratio that lands `share`.
local function er_defense_ratio(share)
    if share >= 0.9 then
        return 8
    elseif share >= 0.7 then
        return 8 - math.sqrt((0.9 - share) * 121 / 0.8)
    elseif share >= 0.4 then
        return 2.5 - math.sqrt((0.7 - share) * 3 / 0.4)
    elseif share > 0.1 then
        return 0.125 + 0.875 * math.sqrt((share - 0.1) / 0.3)
    end
    return 0.125
end

-- WoW's armour reduction of the victim's armour against the attacker's level.
local function armor_reduction(attacker, victim)
    local armor = victim:GetUInt32Value(UNIT_FIELD_RESISTANCES) or 0
    if armor <= 0 or armor > 1000000 then
        return 0
    end
    local level = attacker:GetLevel() or 1
    local k = level < 60 and (400 + 85 * level) or (467.5 * level - 22167.5)
    return math.min(ARMOR_CAP, armor / (armor + k))
end

-- How much of a `damage` blow lands through the victim's armour, Elden Ring's way. The victim's
-- armour sets its defense so that the attacker's average 100 % hit loses exactly WoW's armour
-- reduction; on Elden Ring's curve a heavier hit then loses less of itself and a lighter one more
-- (a dagger poke barely scratches plate, a greataxe slam goes through). No armour: all of it.
-- Returns the damage that lands and the share of it.
local function defended(attacker, victim, damage)
    local reduction = armor_reduction(attacker, victim)
    if reduction <= 0 then
        return damage, 1
    end
    -- The reference hit lands (1 - reduction); scaled by 0.9 onto the curve, whose top is 90 %.
    local reference = er_defense_ratio(0.9 * (1 - reduction))
    local defense = weapon_mean(attacker) / reference
    local share = er_defense_share(damage / defense) / 0.9
    return damage * share, share
end

-- Unit::GetMeleeRange: centre to centre, both bodies' reach already in it. Measure it with
-- centre_distance, not GetDistance or IsWithinDist, which add both bodies' sizes again.
local function melee_reach(attacker, victim)
    local a = attacker:GetFloatValue(UNIT_FIELD_COMBATREACH) or 1.5
    local b = victim:GetFloatValue(UNIT_FIELD_COMBATREACH) or 1.5
    return math.max(5.0, a + b + 4.0 / 3.0)
end

local function centre_distance(a, b)
    local dx, dy = a:GetX() - b:GetX(), a:GetY() - b:GetY()
    return math.sqrt(dx * dx + dy * dy)
end

local function has_shield(player)
    local item = player:GetEquippedItemBySlot(SLOT_OFFHAND)
    return item ~= nil and item:GetClass() == ITEM_CLASS_ARMOR and item:GetSubClass() == ARMOR_SUBCLASS_SHIELD
end

-- Elden Ring's two-handing: only a one-handed weapon gains (a two-handed one always is held so).
local function two_handing_one_hander(player)
    local item = player:GetEquippedItemBySlot(SLOT_MAINHAND)
    return item ~= nil and item:GetClass() == ITEM_CLASS_WEAPON
        and item:GetInventoryType() ~= INVTYPE_2HWEAPON
end

local function cleave_of(player)
    local item = player:GetEquippedItemBySlot(SLOT_MAINHAND)
    if not item or item:GetClass() ~= ITEM_CLASS_WEAPON then
        return UNARMED[1], UNARMED[2]
    end
    local sub = item:GetSubClass()
    if SINGLE_TARGET[sub] then
        return 0, 1
    end
    local row = CLEAVE[sub] or { 0.50, 3 }
    return row[1], row[2]
end

-- Decisions 6-7: target k (nearest first, from 0) takes c^k of the hit up to the cap; units whose
-- circles touch form a blob, and each blob member then takes avg + (c / 2) * (1 - avg).
local function shares(units, c)
    local n = #units
    local share, parent = {}, {}
    for i = 1, n do
        share[i] = c ^ (i - 1)
        parent[i] = i
    end
    local function root(i)
        while parent[i] ~= i do
            i = parent[i]
        end
        return i
    end
    for i = 1, n do
        for j = i + 1, n do
            -- GetDistance already subtracts both units' sizes: circles touch within 0.5 yd.
            if units[i]:GetDistance(units[j]) <= 0.5 then
                parent[root(j)] = root(i)
            end
        end
    end
    local sum, count = {}, {}
    for i = 1, n do
        local r = root(i)
        sum[r] = (sum[r] or 0) + share[i]
        count[r] = (count[r] or 0) + 1
    end
    local beta = c / 2
    for i = 1, n do
        local r = root(i)
        if count[r] > 1 then
            local avg = sum[r] / count[r]
            share[i] = avg + beta * (1 - avg)
        end
    end
    return share
end

-- A rogue's Stealth, highest rank first. Its 30 % slow never applies in Souls mode (owner,
-- 2026-11-21): the run rate seen without it (`free_rate`, kept by the tick) is held instead.
local ROGUE_STEALTH = { 1787, 1786, 1785, 1784 }
local CLASS_ROGUE = 4

local function rogue_stealth_aura(player)
    for _, id in ipairs(ROGUE_STEALTH) do
        if player:HasAura(id) then
            return id
        end
    end
    return nil
end

-- The run rate to scale a stance from: the current one, or under Stealth the one without its slow.
local function natural_rate(player, state)
    if state.free_rate and rogue_stealth_aura(player) then
        return state.free_rate
    end
    return player:GetSpeed(MOVE_RUN) / BASE_RUN_SPEED
end

local function restore_back(player, state)
    if state.back_rate then
        player:SetSpeed(MOVE_RUN_BACK, state.back_rate, true)
        state.back_rate = nil
    end
end

local function restore_speed(player, state)
    if state.sprint_rate then
        player:SetSpeed(MOVE_RUN, state.sprint_rate, true)
        state.sprint_rate = nil
    end
    restore_back(player, state)
end

-- The run speed for the client's stance: a roll runs at its own rate, sprinting raises it,
-- crouching lowers it.
local function apply_speed(player, state)
    local factor = state.roll or (state.sprint and SPRINT_RATE) or (state.crouch and CROUCH_RATE) or 1
    if factor == 1 and not state.roll then
        restore_speed(player, state)
        return
    end
    if not state.sprint_rate then
        state.sprint_rate = natural_rate(player, state)
    end
    player:SetSpeed(MOVE_RUN, state.sprint_rate * factor, true)
    -- A roll back carries as fast as one forward.
    if state.roll then
        if not state.back_rate then
            state.back_rate = player:GetSpeed(MOVE_RUN_BACK) / BASE_RUN_BACK_SPEED
        end
        local back = state.sprint_rate * factor * BASE_RUN_SPEED / BASE_RUN_BACK_SPEED
        player:SetSpeed(MOVE_RUN_BACK, back, true)
    else
        restore_back(player, state)
    end
end

-- The crouch pose for everyone else: the stealth CREEP stand flag, only if we set it (a real
-- stealth aura owns it otherwise).
local function set_creep(player, state, on)
    local flags = player:GetByteValue(UNIT_FIELD_BYTES_1, BYTES_1_VIS_FLAG) or 0
    local has = flags % (2 * STAND_FLAG_CREEP) >= STAND_FLAG_CREEP
    if on and not has then
        player:SetByteValue(UNIT_FIELD_BYTES_1, BYTES_1_VIS_FLAG, flags + STAND_FLAG_CREEP)
        state.creep_set = true
    elseif not on and has and state.creep_set then
        player:SetByteValue(UNIT_FIELD_BYTES_1, BYTES_1_VIS_FLAG, flags - STAND_FLAG_CREEP)
    end
    if not on then
        state.creep_set = nil
    end
end

-- Crouch stealth (owner decisions 2026-11-21) ----------------------------------------------------
-- A crouched Souls player out of combat hides: stock stealth (HIDE_SPELL, refreshed while they
-- stay hidden, so it lapses by itself if this script stops) keeps the core from seeing them past
-- a creature's own combat reach. Whether a creature notices them is decided here. Each hostile
-- creature nearby builds suspicion from sight (its view cone, in line of sight, the range shrunk
-- by darkness, a roof and holding still, stretched by plate) and from noise (all round, from
-- moving, more in armour heavier than leather). At NOTICE it stops and looks, at INVESTIGATE it
-- walks to where it last placed the player, at SPOTTED it attacks and the stealth breaks.
-- The stealth also breaks when one of the player's attacks lands, on damage taken, in combat,
-- sprinting, rolling or mounted, and comes back STEALTH_COOLDOWN_MS later if they still crouch.
-- A rogue's own Stealth rides along (its openers work) without its slow, and creatures notice a
-- rogue at half: half the view cone, half the noise, half the suspicion from what they see.
-- The first hit landed from hiding on an unaware creature is a sneak attack.
local HIDE_SPELL = 16122            -- "Phasing": aura 16 at 99999, 4 s, broken by damage taken
local HIDE_REFRESH_MS = 2500        -- refresh the aura's 4 s once it has less than this left
local STEALTH_COOLDOWN_MS = 4000    -- owner: about 4 s after a break
local SNEAK_MULTIPLIER = 2.5        -- ESTIMATE: a sneak attack's damage before armour
local STEALTH_EVERY_MS = 200        -- suspicion is worked this often
local SUSPECT_SCAN_YARDS = 30
local SIGHT_YARDS = 20              -- ESTIMATE: a level-matched creature sees a crouched player moving in daylight this far
local SIGHT_HALF_ARC = math.pi / 3  -- ESTIMATE: a 120 degree view cone
local SIGHT_STILL = 0.55            -- ESTIMATE: holding still shrinks the sight range to this share
local SIGHT_RATE = 90               -- ESTIMATE: suspicion a second at point-blank sight...
local SIGHT_FLOOR = 0.25            -- ...falling to this share of it at the range's edge
local NOISE_MOVE_YARDS = 5          -- ESTIMATE: moving is heard this far in cloth or leather...
local NOISE_ARMOR_YARDS = 10        -- ...and this much farther in full plate (full mail half)
local NOISE_RATE = 45               -- ESTIMATE: suspicion a second from noise at point blank
local ARMOR_SHINE = 0.25            -- ESTIMATE: full plate is seen this much farther
local SUSPICION_DECAY = 12          -- a second, while nothing is seen or heard
local NOTICE, INVESTIGATE, SPOTTED = 30, 65, 100
local LEVEL_STEP = 0.06             -- each level the creature has over the player (five at most) stretches its ranges this much
local NIGHT_SIGHT, DUSK_SIGHT, INDOOR_SIGHT = 0.6, 0.8, 0.75
local ROOF_PROBE_YARDS = 30         -- no line of sight this far straight up: under a roof
local ROGUE_SHARE = 0.5             -- owner: rogues are noticed at half
local INVESTIGATE_REPATH_MS = 1500
local INVESTIGATE_POINT_ID = 920800
local EMOTE_ONESHOT_QUESTION = 6
local EYE_STEP = 5                  -- the eye meter is sent in steps of this much suspicion
-- Armour weight past leather per armour slot (the load's eight; weapons, shields, back, jewellery
-- do not count): mail 1, plate 2. Full leather weighs nothing; full plate weighs 1.
local WEIGHT_BY_ARMOR = { [3] = 1, [4] = 2 }
local WEIGHT_MAX = 2 * #LOAD_ARMOR_SLOTS

local function armor_weight(player)
    local weight = 0
    for _, slot in ipairs(LOAD_ARMOR_SLOTS) do
        local item = player:GetEquippedItemBySlot(slot)
        if item and item:GetClass() == ITEM_CLASS_ARMOR then
            weight = weight + (WEIGHT_BY_ARMOR[item:GetSubClass()] or 0)
        end
    end
    return weight / WEIGHT_MAX
end

-- How far creatures see by the light: night and dusk outdoors (server time), any dungeon or a roof
-- overhead. Worked once a second.
local function light_share(player, state, now)
    if state.light and now < state.light_at then
        return state.light
    end
    local share = 1
    local map = player:GetMap()
    if (map and map:IsDungeon())
        or not player:IsWithinLoS(player:GetX(), player:GetY(), player:GetZ() + ROOF_PROBE_YARDS) then
        share = INDOOR_SIGHT
    else
        -- GetGameTime() is a 64-bit userdata that os.date refuses; the server's own clock is
        -- the game clock here.
        local hour = tonumber(os.date("%H"))
        if hour and (hour >= 22 or hour < 5) then
            share = NIGHT_SIGHT
        elseif hour and (hour >= 20 or hour < 7) then
            share = DUSK_SIGHT
        end
    end
    state.light, state.light_at = share, now + 1000
    return share
end

local function rogue_rank(player)
    if player:GetClass() ~= CLASS_ROGUE then
        return nil
    end
    for _, id in ipairs(ROGUE_STEALTH) do
        if player:HasSpell(id) then
            return id
        end
    end
    return nil
end

-- The eye meter: 0 standing, 1 hidden, 2 crouched but seen (cooldown or combat); the highest
-- suspicion around and the creature holding it. Sent when any of it changes.
local function send_eye(player, state, mode, level, ck)
    level = math.floor(level / EYE_STEP + 0.5) * EYE_STEP
    local text = string.format("EYE\t%d\t%d\t%s", mode, level, ck or "0")
    if state.eye_sent ~= text then
        state.eye_sent = text
        send(player, text)
    end
end

local function hide(player, state)
    local aura = player:AddAura(HIDE_SPELL, player)
    if not aura then
        return
    end
    state.hidden = true
    local rank = rogue_rank(player)
    if rank and not rogue_stealth_aura(player) and player:AddAura(rank, player) then
        state.rogue_added = rank
    end
    -- Stealth's speed aura reset the run speed: the crouch rate again, from the unslowed one.
    apply_speed(player, state)
    set_creep(player, state, state.crouch == true)
end

-- Leave hiding; `cooldown` holds the next hide off.
local function unhide(player, state, cooldown)
    local now = now_ms()
    if state.hidden then
        state.hidden = nil
        state.unhid_at = now
        player:RemoveAura(HIDE_SPELL)
        if state.rogue_added then
            player:RemoveAura(state.rogue_added)
            state.rogue_added = nil
        end
        -- The last stealth aura took the crouch pose and the crouch speed with it.
        set_creep(player, state, state.crouch == true)
        apply_speed(player, state)
    end
    if cooldown then
        state.hide_after = now + STEALTH_COOLDOWN_MS
    end
end

-- A creature that was walked off to investigate goes home.
local function calm(player, s)
    if s.moved then
        local map = player:GetMap()
        local creature = map and map:GetWorldObject(s.guid)
        if creature and creature:IsAlive() and not creature:IsInCombat() then
            creature:MoveHome()
        end
    end
end

local function forget_suspects(player, state)
    for _, s in pairs(state.suspects or {}) do
        calm(player, s)
    end
    state.suspects = nil
end

-- A creature's reaction to its suspicion; returns true when it spotted the player.
local function react(creature, s, player, now)
    if s.v >= SPOTTED then
        return true
    end
    if s.v >= INVESTIGATE then
        if now >= (s.path_at or 0) and not creature:IsRooted() then
            s.path_at = now + INVESTIGATE_REPATH_MS
            s.moved = true
            creature:MoveTo(INVESTIGATE_POINT_ID, s.x, s.y, s.z)
        end
    elseif s.v >= NOTICE and not s.moved then
        if not s.noticed then
            s.noticed = true
            creature:PerformEmote(EMOTE_ONESHOT_QUESTION)
        end
        if now >= (s.face_at or 0) then
            s.face_at = now + 1000
            creature:SetFacingToObject(player)
        end
    end
    return false
end

-- A creature has not caught on: out of combat and not yet investigating.
local function unaware(state, unit)
    local s = state.suspects and state.suspects[key(unit:GetGUID())]
    return not unit:IsInCombat() and (not s or s.v < INVESTIGATE)
end

-- The suspicion of every hostile creature around a hidden player; decay for the rest.
local function work_suspicion(player, state, now)
    local dt = (now - (state.suspect_at or now)) / 1000
    if dt <= 0 or dt > 1 then
        dt = STEALTH_EVERY_MS / 1000
    end
    state.suspect_at = now
    state.suspects = state.suspects or {}
    local suspects = state.suspects
    local heard_or_seen = {}
    if state.hidden then
        local share = player:GetClass() == CLASS_ROGUE and ROGUE_SHARE or 1
        local moving = player:IsMoving()
        local weight = armor_weight(player)
        local sight_base = SIGHT_YARDS * light_share(player, state, now) * (moving and 1 or SIGHT_STILL)
            * (1 + ARMOR_SHINE * weight)
        local noise_base = moving and (NOISE_MOVE_YARDS + NOISE_ARMOR_YARDS * weight) * share or 0
        local arc = 2 * SIGHT_HALF_ARC * share
        local level = player:GetLevel()
        for _, creature in ipairs(player:GetCreaturesInRange(SUSPECT_SCAN_YARDS, 0, 1, 1) or {}) do
            if not creature:IsInCombat() then
                local lv = math.max(-5, math.min(5, creature:GetLevel() - level))
                local stretch = 1 + LEVEL_STEP * lv
                local d = creature:GetDistance(player)
                local gain = 0
                local sight = sight_base * stretch
                if d < sight and creature:IsInFront(player, arc) and creature:IsWithinLoS(player) then
                    gain = gain + SIGHT_RATE * share * (SIGHT_FLOOR + (1 - SIGHT_FLOOR) * (1 - d / sight))
                end
                local heard = noise_base * stretch
                if d < heard then
                    gain = gain + NOISE_RATE * (1 - d / heard)
                end
                if gain > 0 then
                    local ck = key(creature:GetGUID())
                    heard_or_seen[ck] = true
                    local s = suspects[ck] or { guid = creature:GetGUID(), v = 0 }
                    suspects[ck] = s
                    s.v = math.min(SPOTTED, s.v + gain * dt)
                    s.x, s.y, s.z = player:GetX(), player:GetY(), player:GetZ()
                    if react(creature, s, player, now) then
                        suspects[ck] = nil
                        trace("spotted by %s (entry %d)", ck, creature:GetEntry())
                        unhide(player, state, true)
                        creature:AttackStart(player)
                        send_eye(player, state, 2, SPOTTED, ck)
                        return
                    end
                end
            end
        end
    end
    local top, top_key = 0, nil
    for ck, s in pairs(suspects) do
        if not heard_or_seen[ck] then
            s.v = s.v - SUSPICION_DECAY * dt
        end
        if s.v <= 0 then
            calm(player, s)
            suspects[ck] = nil
        elseif s.v > top then
            top, top_key = s.v, ck
        end
    end
    send_eye(player, state, state.hidden and 1 or (state.crouch and 2 or 0), top, top_key)
end

-- Hide, refresh or break, then the suspicion; every tick, worked every STEALTH_EVERY_MS.
local function stealth_tick(player, state, now)
    local want = state.crouch and not state.sprint and not state.roll and player:IsAlive()
        and not player:IsInCombat() and not player:IsMounted()
    if state.hidden then
        local aura = player:GetAura(HIDE_SPELL)
        if not want or not aura or (state.rogue_added and not player:HasAura(state.rogue_added)) then
            -- Damage taken, combat, a cast that ended the rogue's Stealth, sprint, roll, mount.
            -- Standing up alone (CROUCH 0) unhides before this, with no cooldown.
            unhide(player, state, true)
        elseif aura:GetDuration() < HIDE_REFRESH_MS then
            aura:SetDuration(4000)
        end
    elseif want and now >= (state.hide_after or 0) then
        hide(player, state)
    end
    if now >= (state.stealth_at or 0) then
        state.stealth_at = now + STEALTH_EVERY_MS
        if state.hidden or state.suspects then
            work_suspicion(player, state, now)
        else
            send_eye(player, state, state.crouch and 2 or 0, 0, nil)
        end
    end
end

-- Hold a rogue's run speed against Stealth's slow, and learn the unslowed rate while not stealthed.
local function keep_speed(player, state)
    if not rogue_stealth_aura(player) then
        if not state.sprint_rate then
            state.free_rate = player:GetSpeed(MOVE_RUN) / BASE_RUN_SPEED
        end
        return
    end
    local want
    if state.sprint_rate then
        local factor = state.roll or (state.sprint and SPRINT_RATE) or (state.crouch and CROUCH_RATE) or 1
        want = state.sprint_rate * factor
    else
        want = state.free_rate
    end
    if want and math.abs(player:GetSpeed(MOVE_RUN) - want * BASE_RUN_SPEED) > 0.05 then
        player:SetSpeed(MOVE_RUN, want, true)
    end
end

local function restore_stance(player, state)
    state.sprint, state.crouch, state.roll = nil, nil, nil
    unhide(player, state, false)
    forget_suspects(player, state)
    restore_speed(player, state)
    set_creep(player, state, false)
end

local function leave(player, k)
    local state = souls[k]
    if state then
        if player then
            restore_stance(player, state)
        end
        souls[k] = nil
    end
end

-- Handing a creature back: stock melee resumes, and it chases its victim again.
local function release(creature, ck)
    local m = managed[ck]
    managed[ck] = nil
    if not creature or not m then
        return
    end
    creature:RemoveFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PACIFIED)
    if m.broken_until then
        creature:EmoteState(EMOTE_STATE_NONE)
    end
    if m.strike or m.broken_until then
        creature:SetRooted(false)
    end
end

local function managed_creature(m)
    local map = GetMapById(m.map, m.instance)
    return map and map:GetWorldObject(m.guid)
end

-- Defined below, beside the strike code; the hit report calls it.
local break_stance, react_to_hit

-- The player's own hit report.
local function on_hit(player, state, fields)
    local now = now_ms()
    local second = math.floor(now / 1000)
    if state.hit_second ~= second then
        state.hit_second, state.hits = second, 0
    end
    state.hits = state.hits + 1
    if state.hits > HITS_PER_SECOND then
        return
    end
    local mv = math.max(0, math.min(tonumber(fields[2]) or 100, 500)) / 100
    local grip = (fields[4] == "2" and two_handing_one_hander(player)) and TWO_HAND_BONUS or 1
    local hostile = {}
    for _, unit in ipairs(player:GetUnfriendlyUnitsInRange(HIT_REACH_YARDS) or {}) do
        hostile[key(unit:GetGUID())] = unit
    end
    local units, seen = {}, {}
    for _, text in ipairs(split(fields[3], ",")) do
        local unit = hostile[text]
        if unit and not seen[text] and unit:IsAlive() and #units < MAX_TARGETS_REPORTED then
            seen[text] = true
            units[#units + 1] = unit
        end
    end
    if #units == 0 then
        return
    end
    if queued_strike_replaces_swing(player) then
        return
    end
    table.sort(units, function(a, b)
        return player:GetDistance(a) < player:GetDistance(b)
    end)
    local c, cap = cleave_of(player)
    while #units > cap do
        table.remove(units)
    end
    local share = shares(units, c)
    -- Decisions 8 and 10: AR is the weapon's DPS over the 2.0 s reference and MV = w = speed /
    -- 2.0, so a light swing deals the stock hit; the kit's motion value scales it from there.
    -- Elden Ring's per-hit damage on top: the grip, a counter hit on a creature mid-windup, a
    -- riposte, then the victim's armour through the defense curve (heavy hits lose less of it).
    local roll = weapon_roll(player)
    local dealt = 0
    local hidden = state.hidden
    for i, unit in ipairs(units) do
        local m = managed[key(unit:GetGUID())]
        local riposte = m and m.parried_until and now < m.parried_until
        local counter = (m and m.strike) and COUNTER_BONUS or 1
        -- A sneak attack: landed from hiding on a creature that has not caught on.
        local sneak = hidden and unaware(state, unit)
        local attack = roll * mv * grip * counter * (riposte and RIPOSTE_MULTIPLIER or 1)
            * (sneak and SNEAK_MULTIPLIER or 1)
        if sneak then
            send(player, "SNEAK\t" .. key(unit:GetGUID()))
            trace("sneak attack on %s", key(unit:GetGUID()))
        end
        local landed, through = defended(player, unit, attack)
        local damage = math.max(1, math.floor(landed * share[i] + 0.5))
        if riposte then
            m.parried_until = nil
        end
        trace("hit %s: roll %.1f x mv %.2f x grip %.2f x counter %.2f%s = %.1f; armour lets %.2f, cleave %.2f -> %d",
            key(unit:GetGUID()), roll, mv, grip, counter, riposte and " x riposte" or "", attack, through,
            share[i], damage)
        player:DealDamage(unit, damage, true)
        dealt = dealt + damage
        if m and not m.broken_until and unit:IsAlive() then
            local cost = STANCE_PER_MOTION * mv * share[i] * (riposte and RIPOSTE_MULTIPLIER or 1)
            if m.stance_refill_at and now >= m.stance_refill_at then
                m.stance = m.stance_max
            end
            m.stance = m.stance - cost
            m.stance_refill_at = now + STANCE_REFILL_MS
            if m.stance <= 0 then
                break_stance(unit, key(unit:GetGUID()), m, now)
            elseif cost > 0 then
                react_to_hit(unit, m, cost, now)
            end
        end
    end
    -- One swing, one auto-attack's rage, on all it dealt (a cleave is still one swing).
    reward_rage(player, dealt, false)
    -- The attack landed: out of hiding (a swing that struck nothing never gets here).
    unhide(player, state, true)
end

local function on_swing(player, fields)
    local name = fields[2]
    if not name or #name > 48 then
        return
    end
    local text = "SWING\t" .. key(player:GetGUID()) .. "\t" .. name
    local me = key(player:GetGUID())
    for _, other in ipairs(player:GetPlayersInRange(SWING_RELAY_YARDS) or {}) do
        local ok = key(other:GetGUID())
        if ok ~= me and souls[ok] then
            send(other, text)
        end
    end
end

local function on_verdict(player, fields)
    local id = tonumber(fields[2])
    local outcome = fields[3]
    local k = key(player:GetGUID())
    for _, m in pairs(managed) do
        local strike = m.strike
        if strike and strike.id == id and strike.victim_key == k then
            if outcome == "hit" or outcome == "guard" or outcome == "parry" or outcome == "dodge" or outcome == "miss" then
                strike.verdict = outcome
            end
            return
        end
    end
end

-- Existing MODE keepalives refresh nearby clients' remote pose leases.
local function announce_peer(player, state)
    local me = key(player:GetGUID())
    local text = "PEER\t" .. me .. "\t" .. (state.sprint and "1" or "0")
    for _, other in ipairs(player:GetPlayersInRange(SWING_RELAY_YARDS) or {}) do
        local other_key = key(other:GetGUID())
        if other_key ~= me and souls[other_key] then
            send(other, text)
        end
    end
end

local on_critical

-- The equipment load score (0-44): the client's roll tier and distance.
local function load_of(player)
    local load = 0
    for _, slot in ipairs(LOAD_ARMOR_SLOTS) do
        local item = player:GetEquippedItemBySlot(slot)
        if item and item:GetClass() == ITEM_CLASS_ARMOR then
            load = load + (LOAD_BY_ARMOR[item:GetSubClass()] or 0)
        end
    end
    for _, slot in ipairs({ SLOT_MAINHAND, SLOT_OFFHAND }) do
        local item = player:GetEquippedItemBySlot(slot)
        if item and item:GetClass() == ITEM_CLASS_WEAPON then
            load = load + (LOAD_BY_WEAPON[item:GetSubClass()] or 0)
            if item:GetInventoryType() == INVTYPE_2HWEAPON then
                load = load + 1
            end
        elseif item and item:GetClass() == ITEM_CLASS_ARMOR and item:GetSubClass() == ARMOR_SUBCLASS_SHIELD then
            load = load + 1
        end
    end
    local ranged = player:GetEquippedItemBySlot(SLOT_RANGED)
    if ranged and ranged:GetClass() == ITEM_CLASS_WEAPON and ranged:GetSubClass() == WEAPON_SUBCLASS_GUN then
        load = load + 1
    end
    return load
end

-- Tell the client its load when it changed, or always with `force`.
local function send_load(player, state, force)
    local load = load_of(player)
    if force or state.load ~= load then
        state.load = load
        send(player, "LOAD\t" .. load)
    end
end

-- A sprint or a roll breaks hiding with the cooldown. The client stands up for either, and its
-- CROUCH 0 may arrive first (no cooldown), so a hide ended just before counts too.
local BROKE_COVER_MS = 500
local function broke_cover(player, state)
    if state.hidden or now_ms() - (state.unhid_at or -BROKE_COVER_MS) < BROKE_COVER_MS then
        unhide(player, state, true)
    end
end

local function on_addon(event, sender, msg_type, prefix, msg, target)
    if prefix ~= PREFIX then
        return
    end
    -- Only a whisper to oneself is ours; anything else with the prefix is dropped.
    if msg_type ~= CHAT_MSG_WHISPER or not target or key(target:GetGUID()) ~= key(sender:GetGUID()) then
        return false
    end
    local fields = split(msg, "\t")
    local verb = fields[1]
    local k = key(sender:GetGUID())
    local state = souls[k]
    if verb == "HELLO" then
        send(sender, "HELLO\t" .. VERSION)
    elseif verb == "MODE" then
        if fields[2] == "1" then
            if not state then
                souls[k] = { guid = sender:GetGUID(), hits = 0 }
                sender:AttackStop()
            end
            send(sender, "MODE\t1")
            announce_peer(sender, souls[k])
            send_load(sender, souls[k], true)
        else
            leave(sender, k)
            send(sender, "MODE\t0")
        end
    elseif state and verb == "SPRINT" then
        state.sprint = fields[2] == "1" or nil
        if state.sprint then
            broke_cover(sender, state)
        end
        apply_speed(sender, state)
        announce_peer(sender, state)
    elseif state and verb == "ROLL" then
        local rate = tonumber(fields[2]) or 0
        state.roll = rate > 0 and math.min(math.max(rate, ROLL_RATE_MIN), ROLL_RATE_MAX) or nil
        if state.roll then
            broke_cover(sender, state)
        end
        apply_speed(sender, state)
    elseif state and verb == "CROUCH" then
        state.crouch = fields[2] == "1" or nil
        if not state.crouch then
            unhide(sender, state, false)
        end
        apply_speed(sender, state)
        set_creep(sender, state, state.crouch == true)
    elseif state and verb == "HIT" then
        on_hit(sender, state, fields)
    elseif state and verb == "CRIT" then
        on_critical(sender, state, fields[2], fields[3] == "back")
    elseif state and verb == "SWING" then
        on_swing(sender, fields)
    elseif state and verb == "VERDICT" then
        on_verdict(sender, fields)
    end
    return false
end

-- Souls mode blocks the player's own auto-attack: their swings are the kit's.
local function on_attack_swing(event, packet, player)
    if player and souls[key(player:GetGUID())] then
        return false
    end
end

local function on_logout(event, player)
    leave(player, key(player:GetGUID()))
end

-- Whether a creature fighting `victim` should strike Souls-style: its victim is in Souls mode,
-- or a Souls player is on its threat list (a bot or pet tanking it then gets the area check).
local function wanted(creature, victim)
    if souls[key(victim:GetGUID())] then
        return true
    end
    for _, unit in ipairs(creature:GetThreatList() or {}) do
        if souls[key(unit:GetGUID())] then
            return true
        end
    end
    return false
end

local function strike_watchers(creature, strike)
    local out = {}
    for _, other in ipairs(creature:GetPlayersInRange(MANAGE_RANGE_YARDS) or {}) do
        if souls[key(other:GetGUID())] then
            out[#out + 1] = other
        end
    end
    return out
end

-- Authored entry policy: kobold workers/miners thrust; Harvest Golems slam.
-- Everything else sweeps. No model/family guesses or extra server data required.
local STRIKE_KIND_BY_ENTRY = { [6] = "thrust", [257] = "thrust", [36] = "slam", [114] = "slam" }
local STRIKE_SHAPES = {
    sweep = { half_cone = math.rad(90), extra_reach = 0, windup_scale = 1 },
    thrust = { half_cone = math.rad(20), extra_reach = 1, windup_scale = 1 },
    slam = { half_cone = math.pi, extra_reach = 0, windup_scale = 1.5 },
}

local function start_strike(creature, ck, m, victim, now)
    -- AzerothCore keeps the attack time as a float (Unit::SetAttackTime); read as an integer it
    -- is the float's bits (2000.0 reads 1157234688), which put every next strike days away.
    local attack_ms = creature:GetFloatValue(UNIT_FIELD_BASEATTACKTIME) or 2000
    if not (attack_ms >= 500 and attack_ms <= 10000) then
        attack_ms = 2000
    end
    local kind = STRIKE_KIND_BY_ENTRY[creature:GetEntry()] or "sweep"
    local shape = STRIKE_SHAPES[kind]
    local windup = math.floor(math.max(WINDUP_MIN_MS, math.min(WINDUP_MAX_MS, attack_ms * WINDUP_SHARE)) * shape.windup_scale)
    local id = next_strike_id
    next_strike_id = next_strike_id + 1
    -- The stock chase stops inside the melee range; a strike reaches as far as it may start, so a
    -- victim who stays put is hit and one who steps back out of it is missed. It does not stretch
    -- to where the victim stands.
    local reach = melee_reach(creature, victim) + START_SLACK_YARDS
    -- Decision 9: the creature stops and its facing locks for the windup. A root, not MoveIdle:
    -- MoveIdle leaves a running chase on top of the motion master, and the stock chase resumes
    -- by itself once the root is lifted.
    creature:SetRooted(true)
    creature:SetFacing(creature:GetAngle(victim))
    m.strike = {
        id = id,
        victim = victim:GetGUID(),
        victim_key = key(victim:GetGUID()),
        reach = reach,
        kind = kind,
        resolve_at = now + windup,
        verdict = nil,
    }
    m.next_at = now + math.max(attack_ms, windup + 400)
    trace("strike %d from %s on %s: windup %d ms, next in %d ms", id, ck, m.strike.victim_key, windup,
        m.next_at - now)
    for _, watcher in ipairs(strike_watchers(creature, m.strike)) do
        local mine = key(watcher:GetGUID()) == m.strike.victim_key and 1 or 0
        send(watcher, string.format("STRIKE\t%d\t%s\t%d\t%.2f\t%d\t%s", id, ck, windup, reach, mine, kind))
    end
end

-- The server's own judgement: inside reach and the frontal cone.
local function area_check(creature, victim, strike)
    local shape = STRIKE_SHAPES[strike.kind]
    if centre_distance(creature, victim) > strike.reach + shape.extra_reach then
        return "miss"
    end
    if angle_diff(creature:GetO(), creature:GetAngle(victim)) > shape.half_cone then
        return "miss"
    end
    return "hit"
end

local function resolve_strike(creature, ck, m, now)
    local strike = m.strike
    m.strike = nil
    local victim = creature:GetMap():GetWorldObject(strike.victim)
    local outcome = "miss"
    if victim and victim:IsAlive() then
        local is_souls = souls[strike.victim_key] ~= nil
        outcome = (is_souls and strike.verdict) or area_check(creature, victim, strike)
        -- The player's armour, on the same curve.
        local roll = defended(creature, victim, weapon_roll(creature))
        if outcome == "hit" then
            creature:DealDamage(victim, math.max(1, math.floor(roll + 0.5)), true)
        elseif outcome == "guard" then
            -- Sent even at zero, so the client's guard pays its stamina for the blow.
            local taken = has_shield(victim) and 0 or GUARD_TAKEN_NO_SHIELD
            creature:DealDamage(victim, math.floor(roll * taken + 0.5), false)
        elseif outcome == "parry" then
            m.parried_until = now + RIPOSTE_MS
            m.next_at = now + PARRY_STAGGER_MS
            creature:PerformEmote(EMOTE_ONESHOT_WOUND)
            send(victim, string.format("PARRIED\t%s\t%d", ck, RIPOSTE_MS))
        end
    end
    for _, watcher in ipairs(strike_watchers(creature, strike)) do
        send(watcher, "RESOLVE\t" .. strike.id .. "\t" .. outcome)
    end
    trace("strike %d from %s resolved %s; next in %d ms", strike.id, ck, outcome, m.next_at - now)
    creature:SetRooted(false)
end

-- A hit reaction cannot chain-lock a creature: separate, per-creature cooldowns.
local FLINCH_COOLDOWN_MS = 450
local INTERRUPT_COOLDOWN_MS = 1800
react_to_hit = function(creature, m, cost, now)
    if m.strike and cost >= m.stance_max / 3 and now >= (m.can_interrupt_at or 0) then
        local strike = m.strike
        m.strike = nil
        m.can_interrupt_at = now + INTERRUPT_COOLDOWN_MS
        m.can_flinch_at = now + FLINCH_COOLDOWN_MS
        for _, watcher in ipairs(strike_watchers(creature, strike)) do
            send(watcher, "RESOLVE\t" .. strike.id .. "\tinterrupted")
        end
        creature:SetRooted(false)
        creature:PerformEmote(EMOTE_ONESHOT_WOUND_CRITICAL)
        trace("strike %d interrupted: %.1f stance damage", strike.id, cost)
    elseif now >= (m.can_flinch_at or 0) then
        m.can_flinch_at = now + FLINCH_COOLDOWN_MS
        creature:PerformEmote(EMOTE_ONESHOT_WOUND)
        trace("flinch %s: %.1f stance damage", key(creature:GetGUID()), cost)
    end
end

-- A broken stance: the strike in progress is cancelled, casts are interrupted, and the creature
-- stands stunned with its riposte window open until it recovers.
break_stance = function(creature, ck, m, now)
    if m.strike then
        for _, watcher in ipairs(strike_watchers(creature, m.strike)) do
            send(watcher, "RESOLVE\t" .. m.strike.id .. "\tbroken")
        end
        m.strike = nil
    end
    m.broken_until = now + STANCE_BREAK_MS
    m.parried_until = nil
    m.next_at = now + STANCE_BREAK_MS + 400
    m.stance = m.stance_max
    m.stance_refill_at = nil
    creature:InterruptSpell(1)  -- CURRENT_GENERIC_SPELL
    creature:InterruptSpell(2)  -- CURRENT_CHANNELED_SPELL
    creature:SetRooted(true)
    creature:EmoteState(EMOTE_STATE_STUN)
    trace("stance of %s broken", ck)
    for _, watcher in ipairs(strike_watchers(creature, nil)) do
        send(watcher, string.format("BROKEN\t%s\t%d", ck, STANCE_BREAK_MS))
    end
end

local function end_break(creature, m)
    m.broken_until = nil
    creature:EmoteState(EMOTE_STATE_NONE)
    creature:SetRooted(false)
end

local CRITICAL_MULTIPLIER = 3 -- ESTIMATE: critical damage before armour.
on_critical = function(player, state, guid, back)
    local m = managed[guid]
    local now = now_ms()
    if back then
        if now < (state.next_backstab or 0) or not player:IsAlive() then return end
        for _, creature in ipairs(player:GetUnfriendlyUnitsInRange(HIT_REACH_YARDS) or {}) do
            if key(creature:GetGUID()) == guid and creature:GetObjectType() == "Creature"
                and creature:IsAlive() and creature:IsWithinDist(player, melee_reach(creature, player)) then
                local away = angle_diff(creature:GetO(), creature:GetAngle(player)) >= 3 * math.pi / 4
                local victim = creature:GetVictim()
                local fighting_player = victim and key(victim:GetGUID()) == key(player:GetGUID())
                if away and not fighting_player then
                    state.next_backstab = now + 1000
                    local damage = defended(player, creature, weapon_roll(player) * CRITICAL_MULTIPLIER)
                    player:DealDamage(creature, math.max(1, math.floor(damage + 0.5)), true)
                    reward_rage(player, math.max(1, math.floor(damage + 0.5)), true)
                    unhide(player, state, true)
                    trace("backstab %s: %.1f damage", guid, damage)
                end
                return
            end
        end
        return
    end
    if not m or not m.broken_until or now >= m.broken_until then
        return
    end
    local creature = managed_creature(m)
    if not creature or not creature:IsAlive() or not player:IsAlive()
        or creature:GetMap() ~= player:GetMap() then
        return
    end
    if not creature:IsWithinDist(player, melee_reach(creature, player))
        or angle_diff(creature:GetO(), creature:GetAngle(player)) > math.pi / 4 then
        return
    end
    end_break(creature, m)
    for _, watcher in ipairs(strike_watchers(creature, nil)) do
        send(watcher, "BROKEN\t" .. guid .. "\t0")
    end
    local damage = defended(player, creature, weapon_roll(player) * CRITICAL_MULTIPLIER)
    player:DealDamage(creature, math.max(1, math.floor(damage + 0.5)), true)
    reward_rage(player, math.max(1, math.floor(damage + 0.5)), true)
    trace("critical %s: %.1f damage", guid, damage)
end

local function adopt(creature, now)
    local ck = key(creature:GetGUID())
    if managed[ck] then
        return
    end
    creature:SetFlag(UNIT_FIELD_FLAGS, UNIT_FLAG_PACIFIED)
    local stance = STANCE_BASE + STANCE_PER_LEVEL * creature:GetLevel()
    if creature:IsElite() then
        stance = stance * STANCE_ELITE_MULTIPLIER
    end
    managed[ck] = {
        guid = creature:GetGUID(),
        map = creature:GetMapId(),
        instance = creature:GetInstanceId(),
        next_at = now + 300,
        stance = stance,
        stance_max = stance,
    }
    trace("adopted %s (entry %d, level %d, stance %d)", ck, creature:GetEntry(), creature:GetLevel(), stance)
end

local function tick()
    local now = now_ms()
    -- Adopt the creatures fighting Souls players.
    for k, state in pairs(souls) do
        local player = GetPlayerByGUID(state.guid)
        if not player or not player:IsInWorld() then
            souls[k] = nil
        else
            send_load(player, state, false)
            keep_speed(player, state)
            stealth_tick(player, state, now)
        end
        if player and souls[k] and player:IsInCombat() then
            -- Hostile and neutral alike (0): a neutral creature fights back once struck.
            for _, creature in ipairs(player:GetCreaturesInRange(MANAGE_RANGE_YARDS, 0, 0, 1) or {}) do
                local victim = creature:IsInCombat() and creature:GetVictim()
                if victim and wanted(creature, victim) then
                    adopt(creature, now)
                end
            end
        end
    end
    -- Drive them: one telegraphed strike per attack time while in reach.
    for ck, m in pairs(managed) do
        local creature = managed_creature(m)
        if not creature then
            trace("lost %s: not found on map %d instance %d", ck, m.map, m.instance)
            managed[ck] = nil
        else
            local victim = creature:IsAlive() and creature:IsInCombat() and creature:GetVictim()
            if not victim or not wanted(creature, victim) then
                trace("released %s: alive %s, in combat %s, victim %s", ck, tostring(creature:IsAlive()),
                    tostring(creature:IsInCombat()), victim and key(victim:GetGUID()) or "none")
                release(creature, ck)
            elseif m.broken_until then
                if now >= m.broken_until then
                    end_break(creature, m)
                end
            elseif m.strike then
                local strike = m.strike
                local judged = strike.verdict ~= nil or souls[strike.victim_key] == nil
                if (judged and now >= strike.resolve_at) or now >= strike.resolve_at + VERDICT_GRACE_MS then
                    resolve_strike(creature, ck, m, now)
                end
            elseif now >= m.next_at then
                local casting = creature:IsCasting()
                local reach = melee_reach(creature, victim)
                if not casting and centre_distance(creature, victim) <= reach + START_SLACK_YARDS then
                    start_strike(creature, ck, m, victim, now)
                elseif not m.held_traced_at or now >= m.held_traced_at + TRACE_HELD_MS then
                    m.held_traced_at = now
                    trace("held %s: casting %s, %.1f yd from %s (reach %.1f)", ck, tostring(casting),
                        centre_distance(creature, victim), key(victim:GetGUID()), reach)
                end
            end
        end
    end
end

-- A reload or shutdown forgets this script's state: hand everything back first.
local function on_close()
    for ck, m in pairs(managed) do
        release(managed_creature(m), ck)
    end
    for k, state in pairs(souls) do
        local player = GetPlayerByGUID(state.guid)
        if player then
            restore_stance(player, state)
        end
    end
    managed, souls = {}, {}
end

RegisterServerEvent(ADDON_EVENT_ON_MESSAGE, on_addon)
RegisterServerEvent(ALE_EVENT_ON_LUA_STATE_CLOSE, on_close)
RegisterPlayerEvent(PLAYER_EVENT_ON_LOGOUT, on_logout)
RegisterPacketEvent(CMSG_ATTACKSWING, PACKET_EVENT_ON_PACKET_RECEIVE, on_attack_swing)
CreateLuaEvent(tick, TICK_MS, 0)
