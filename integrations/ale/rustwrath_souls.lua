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
  CROUCH <1|0>                   lower / restore the run speed; the crouch pose for others
  HIT <mv%> <guid,guid,...>      one hit window's struck units, nearest first; damage with cleave
  SWING <action>                 relayed to Souls players nearby as SWING <guid> <action>
  VERDICT <id> <hit|guard|parry|dodge|miss>   the client's judgement of strike <id>

Server -> client
  STRIKE <id> <creature guid> <windup ms> <reach yd> <1 if you judge it|0>
  RESOLVE <id> <outcome>
  PARRIED <creature guid> <riposte ms>
  BROKEN <creature guid> <riposte ms>   its stance broke (to every Souls player nearby)
  SWING <player guid> <action>

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
local UNIT_FLAG_PACIFIED = 0x00020000
local BYTES_1_VIS_FLAG = 2          -- UNIT_BYTES_1_OFFSET_VIS_FLAG
local STAND_FLAG_CREEP = 0x02       -- UNIT_STAND_FLAGS_CREEP (the stealth crouch pose)

local MOVE_RUN = 1
local BASE_RUN_SPEED = 7.0
local SLOT_MAINHAND = 15
local SLOT_OFFHAND = 16
local ITEM_CLASS_WEAPON = 2
local ITEM_CLASS_ARMOR = 4
local ARMOR_SUBCLASS_SHIELD = 6
local EMOTE_ONESHOT_WOUND = 33
local EMOTE_ONESHOT_WOUND_CRITICAL = 34
local EMOTE_STATE_NONE = 0
local EMOTE_STATE_STUN = 64

local TICK_MS = 100
local SPRINT_RATE = 1.4             -- ESTIMATE: ER's sprint over its run
local CROUCH_RATE = 0.5             -- ESTIMATE: ER's crouch walk over its run
local WINDUP_SHARE = 0.4            -- decision 16: 40 % of the attack time...
local WINDUP_MIN_MS = 600           -- ...clamped 0.6-1.5 s
local WINDUP_MAX_MS = 1500
local VERDICT_GRACE_MS = 500        -- wait this long past the windup for the client's verdict
local CONE_HALF_RADIANS = math.pi / 3   -- decision 16: a 120 degree frontal cone
local GUARD_TAKEN_NO_SHIELD = 0.4   -- ESTIMATE: a weapon guard lets 40 % through; a shield none
local PARRY_STAGGER_MS = 2500       -- ESTIMATE: a parried creature's next strike waits this long
local RIPOSTE_MS = 3000             -- ESTIMATE: a hit this soon after a parry is a riposte...
local RIPOSTE_MULTIPLIER = 3.0      -- ...for this many times the damage
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

local function melee_reach(attacker, victim)
    local a = attacker:GetFloatValue(UNIT_FIELD_COMBATREACH) or 1.5
    local b = victim:GetFloatValue(UNIT_FIELD_COMBATREACH) or 1.5
    return math.max(5.0, a + b + 4.0 / 3.0)
end

local function has_shield(player)
    local item = player:GetEquippedItemBySlot(SLOT_OFFHAND)
    return item ~= nil and item:GetClass() == ITEM_CLASS_ARMOR and item:GetSubClass() == ARMOR_SUBCLASS_SHIELD
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

local function restore_speed(player, state)
    if state.sprint_rate then
        player:SetSpeed(MOVE_RUN, state.sprint_rate, true)
        state.sprint_rate = nil
    end
end

-- The run speed for the client's stance: sprinting raises it, crouching lowers it.
local function apply_speed(player, state)
    local factor = (state.sprint and SPRINT_RATE) or (state.crouch and CROUCH_RATE) or 1
    if factor == 1 then
        restore_speed(player, state)
        return
    end
    if not state.sprint_rate then
        state.sprint_rate = player:GetSpeed(MOVE_RUN) / BASE_RUN_SPEED
    end
    player:SetSpeed(MOVE_RUN, state.sprint_rate * factor, true)
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

local function restore_stance(player, state)
    state.sprint, state.crouch = nil, nil
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
local break_stance

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
    local roll = weapon_roll(player)
    for i, unit in ipairs(units) do
        local m = managed[key(unit:GetGUID())]
        local riposte = m and m.parried_until and now < m.parried_until
        local damage = roll * mv * share[i] * (riposte and RIPOSTE_MULTIPLIER or 1)
        if riposte then
            m.parried_until = nil
        end
        player:DealDamage(unit, math.max(1, math.floor(damage + 0.5)), true, 0)
        if m and not m.broken_until and unit:IsAlive() then
            local cost = STANCE_PER_MOTION * mv * share[i] * (riposte and RIPOSTE_MULTIPLIER or 1)
            if m.stance_refill_at and now >= m.stance_refill_at then
                m.stance = m.stance_max
            end
            m.stance = m.stance - cost
            m.stance_refill_at = now + STANCE_REFILL_MS
            if m.stance <= 0 then
                break_stance(unit, key(unit:GetGUID()), m, now)
            end
        end
    end
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
        else
            leave(sender, k)
            send(sender, "MODE\t0")
        end
    elseif state and verb == "SPRINT" then
        state.sprint = fields[2] == "1" or nil
        apply_speed(sender, state)
    elseif state and verb == "CROUCH" then
        state.crouch = fields[2] == "1" or nil
        apply_speed(sender, state)
        set_creep(sender, state, state.crouch == true)
    elseif state and verb == "HIT" then
        on_hit(sender, state, fields)
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

local function start_strike(creature, ck, m, victim, now)
    local attack_ms = creature:GetUInt32Value(UNIT_FIELD_BASEATTACKTIME) or 2000
    if attack_ms <= 0 then
        attack_ms = 2000
    end
    local windup = math.floor(math.max(WINDUP_MIN_MS, math.min(WINDUP_MAX_MS, attack_ms * WINDUP_SHARE)))
    local id = next_strike_id
    next_strike_id = next_strike_id + 1
    local reach = melee_reach(creature, victim)
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
        resolve_at = now + windup,
        verdict = nil,
    }
    m.next_at = now + math.max(attack_ms, windup + 400)
    trace("strike %d from %s on %s: windup %d ms, next in %d ms", id, ck, m.strike.victim_key, windup,
        m.next_at - now)
    for _, watcher in ipairs(strike_watchers(creature, m.strike)) do
        local mine = key(watcher:GetGUID()) == m.strike.victim_key and 1 or 0
        send(watcher, string.format("STRIKE\t%d\t%s\t%d\t%.2f\t%d", id, ck, windup, reach, mine))
    end
end

-- The server's own judgement: inside reach and the frontal cone.
local function area_check(creature, victim, strike)
    if not creature:IsWithinDist(victim, strike.reach) then
        return "miss"
    end
    if angle_diff(creature:GetO(), creature:GetAngle(victim)) > CONE_HALF_RADIANS then
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
        local roll = weapon_roll(creature)
        if outcome == "hit" then
            creature:DealDamage(victim, math.max(1, math.floor(roll + 0.5)), true, 0)
        elseif outcome == "guard" then
            -- Sent even at zero, so the client's guard pays its stamina for the blow.
            local taken = has_shield(victim) and 0 or GUARD_TAKEN_NO_SHIELD
            creature:DealDamage(victim, math.floor(roll * taken + 0.5), false, 0)
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
    m.parried_until = now + STANCE_BREAK_MS
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
        elseif player:IsInCombat() then
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
                if not casting and creature:IsWithinDist(victim, reach) then
                    start_strike(creature, ck, m, victim, now)
                elseif not m.held_traced_at or now >= m.held_traced_at + TRACE_HELD_MS then
                    m.held_traced_at = now
                    trace("held %s: casting %s, %.1f yd from %s (reach %.1f)", ck, tostring(casting),
                        creature:GetDistance(victim), key(victim:GetGUID()), reach)
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
