//! The 3.3.5a FrameScript security natives and restricted frame-handle proxy surface that stock
//! FrameXML (`RestrictedFrames.lua`, `RestrictedExecution.lua`, `SecureHandlers.lua`) and Wrath
//! secure-handler addons call at load and runtime.
//!
//! `GetFrameHandle(frame, create)` creates and caches a dedicated `newproxy`-backed userdata
//! handle per live frame wrapper in weak registry tables (`REG_FRAME_TO_HANDLE` /
//! `REG_HANDLE_TO_FRAME`), distinct from the raw frame table, with a protected metatable
//! (`__metatable = false`) whose `__index` routes restricted handle methods safely to the
//! underlying frame and whose identity survives restricted execution (including idempotent
//! `GetFrameHandle(handle)` calls such as `SecureHandlers.lua:108`).
//!
//! The 1.12 host VM must never see these globals, so the Wrath block alone installs them.

use mlua::{Function, Lua, MultiValue, Table, Value};

/// Registry key for the weak-key map `frame_wrapper_table -> frame_handle_userdata`.
pub(super) const REG_FRAME_TO_HANDLE: &str = "benilla_wrath_frame_to_handle";
/// Registry key for the weak-key/value map `frame_handle_userdata -> frame_wrapper_table`.
pub(super) const REG_HANDLE_TO_FRAME: &str = "benilla_wrath_handle_to_frame";
/// Registry key for the restricted snippet runner `(frame_or_handle, body) -> ...`.
const REG_RUN_RESTRICTED: &str = "benilla_wrath_run_restricted";

/// Whether `value` is a live FrameScript wrapper table (`rawget(table, 0)` is a lightuserdata).
pub(super) fn is_frame_wrapper(value: &Value) -> bool {
    let Value::Table(table) = value else {
        return false;
    };
    table
        .raw_get::<Value>(0)
        .is_ok_and(|v| matches!(v, Value::LightUserData(_)))
}

/// Resolve either a live frame wrapper table or a restricted frame-handle proxy to its underlying
/// frame wrapper table. Returns `Ok(None)` for any other value.
pub(super) fn resolve_frame_wrapper(lua: &Lua, value: &Value) -> mlua::Result<Option<Table>> {
    if is_frame_wrapper(value) {
        if let Value::Table(table) = value {
            return Ok(Some(table.clone()));
        }
    }
    if matches!(value, Value::Nil) {
        return Ok(None);
    }
    let Ok(handle_to_frame) = lua.named_registry_value::<Table>(REG_HANDLE_TO_FRAME) else {
        return Ok(None);
    };
    match handle_to_frame.raw_get::<Value>(value.clone())? {
        Value::Table(frame) if is_frame_wrapper(&Value::Table(frame.clone())) => Ok(Some(frame)),
        _ => Ok(None),
    }
}

/// Execute a validated restricted snippet body against `frame` with `self` bound to its
/// restricted frame handle (`SecureHandler:Execute` / `SecureHandlerExecute`).
pub(super) fn run_restricted_body(lua: &Lua, frame: &Table, body: &str) -> mlua::Result<()> {
    let runner: Function = lua.named_registry_value(REG_RUN_RESTRICTED)?;
    runner.call::<()>((frame.clone(), body))
}

const FRAME_HANDLE_LUA: &str = r##"
return function(is_frame_wrapper)
    local type = type
    local select = select
    local error = error
    local tostring = tostring
    local rawget = rawget
    local setmetatable = setmetatable
    local getmetatable = getmetatable
    local setfenv = setfenv
    local loadstring = loadstring
    local newproxy = newproxy
    local string_lower = string.lower
    local _G = _G

    -- Weak-key frame->handle (strong value keeps cached handle alive while the frame lives)
    -- and weak-kv handle->frame (no strong cycle).
    local FRAME_TO_HANDLE = setmetatable({}, { __mode = "k" })
    local HANDLE_TO_FRAME = setmetatable({}, { __mode = "kv" })

    local HANDLE_PROTO = newproxy(true)
    local HANDLE_MT = getmetatable(HANDLE_PROTO)
    local HANDLE_METHODS = {}

    local function check_handle(self)
        local frame = self ~= nil and HANDLE_TO_FRAME[self] or nil
        if not frame then
            error("Invalid frame handle", 3)
        end
        return frame
    end

    local function unwrap_if_handle(v)
        if v == nil then
            return nil
        end
        local f = HANDLE_TO_FRAME[v]
        if f ~= nil then
            return f
        end
        return v
    end

    local function GetFrameHandle(frame, create)
        if frame == nil then
            return nil
        end
        -- Idempotent when passed an existing handle (e.g. restricted `self` at SecureHandlers.lua:108).
        if HANDLE_TO_FRAME[frame] ~= nil then
            return frame
        end
        if not is_frame_wrapper(frame) then
            return nil
        end
        local handle = FRAME_TO_HANDLE[frame]
        if handle ~= nil then
            return handle
        end
        if create == false then
            return nil
        end
        handle = newproxy(HANDLE_PROTO)
        FRAME_TO_HANDLE[frame] = handle
        HANDLE_TO_FRAME[handle] = frame
        return handle
    end

    local function IsFrameHandle(value, _isManaged)
        return value ~= nil and HANDLE_TO_FRAME[value] ~= nil
    end

    local function GetFrameHandleFrame(handle, _force, _allowNonManaged)
        local frame = handle ~= nil and HANDLE_TO_FRAME[handle] or nil
        if frame ~= nil then
            return frame, true
        end
        return nil
    end

    local function GetManagedFrame(handle)
        if handle == nil then
            return nil
        end
        return HANDLE_TO_FRAME[handle]
    end

    local function wrap_handles(n, i, v, ...)
        if i > n then
            return
        end
        return GetFrameHandle(v), wrap_handles(n, i + 1, ...)
    end

    local function wrap_all_handles(...)
        local n = select("#", ...)
        if n == 0 then
            return
        end
        return wrap_handles(n, 1, ...)
    end

    local RESTRICTED_GLOBALS = {
        "select", "type", "pairs", "ipairs", "next", "tonumber", "tostring", "unpack",
        "error", "assert", "pcall", "xpcall",
        "math", "string", "table", "bit",
        "abs", "acos", "asin", "atan", "atan2", "ceil", "cos", "deg", "exp", "floor",
        "frexp", "ldexp", "log", "log10", "max", "min", "mod", "rad", "random", "sin",
        "sqrt", "tan",
        "format", "gmatch", "gsub", "strbyte", "strchar", "strconcat", "strfind",
        "strjoin", "strlen", "strlower", "strmatch", "strrep", "strrev", "strsplit",
        "strsub", "strtrim", "strupper",
        "sort", "tinsert", "tremove", "tconcat", "wipe", "newtable", "scrub",
        "GetFrameHandle", "IsFrameHandle", "SecureCmdOptionParse",
        "GetActionBarPage", "GetBonusBarOffset", "GetShapeshiftForm", "HasAction",
        "IsShiftKeyDown", "IsControlKeyDown", "IsAltKeyDown", "IsModifierKeyDown",
        "IsMouseButtonDown", "IsMounted", "IsSwimming", "IsFlying", "IsFlyableArea",
        "IsIndoors", "IsOutdoors", "IsStealthed", "IsPossessBarVisible",
        "UnitExists", "UnitCanAttack", "UnitCanAssist", "UnitIsDeadOrGhost",
        "UnitAffectingCombat", "UnitHasVehicleUI", "UnitChannelInfo",
        "GetNumPartyMembers", "GetNumRaidMembers", "GetActiveTalentGroup",
        "GetMouseButtonClicked", "InCombatLockdown",
    }

    local function make_restricted_env(control_handle, owner_handle)
        local env = {
            control = control_handle,
            owner = owner_handle,
        }
        for i = 1, #RESTRICTED_GLOBALS do
            local k = RESTRICTED_GLOBALS[i]
            local v = _G[k]
            if v ~= nil then
                env[k] = v
            end
        end
        env.GetFrameHandle = GetFrameHandle
        env.IsFrameHandle = IsFrameHandle
        return env
    end

    local function run_snippet(control_handle, target_handle, body, ...)
        if type(body) ~= "string" then
            return
        end
        local chunk, err = loadstring("return function(self, ...) " .. body .. "\nend", "benilla:restricted-snippet")
        if not chunk then
            error(err, 3)
        end
        local env = make_restricted_env(control_handle, target_handle)
        setfenv(chunk, env)
        local fn = chunk()
        setfenv(fn, env)
        return fn(target_handle, ...)
    end

    -- Identity & hierarchy
    function HANDLE_METHODS:GetFrameHandle()
        check_handle(self)
        return self
    end

    function HANDLE_METHODS:GetName()
        return check_handle(self):GetName()
    end

    function HANDLE_METHODS:GetObjectType()
        return check_handle(self):GetObjectType()
    end

    function HANDLE_METHODS:IsObjectType(kind)
        return check_handle(self):IsObjectType(kind)
    end

    function HANDLE_METHODS:IsProtected()
        return check_handle(self):IsProtected()
    end

    function HANDLE_METHODS:GetID()
        return check_handle(self):GetID()
    end

    function HANDLE_METHODS:SetID(id)
        return check_handle(self):SetID(id)
    end

    function HANDLE_METHODS:GetParent()
        return GetFrameHandle(check_handle(self):GetParent())
    end

    function HANDLE_METHODS:SetParent(parent)
        local frame = check_handle(self)
        if parent == nil then
            return frame:SetParent(nil)
        end
        return frame:SetParent(unwrap_if_handle(parent))
    end

    function HANDLE_METHODS:GetChildren()
        return wrap_all_handles(check_handle(self):GetChildren())
    end

    function HANDLE_METHODS:GetChildList(tbl)
        local frame = check_handle(self)
        if type(tbl) ~= "table" then
            error("Usage: handle:GetChildList(table)", 2)
        end
        for k in pairs(tbl) do
            tbl[k] = nil
        end
        local function fill(i, n, v, ...)
            if i > n then
                return
            end
            local h = GetFrameHandle(v)
            if h ~= nil then
                tbl[#tbl + 1] = h
            end
            return fill(i + 1, n, ...)
        end
        local function collect(...)
            return fill(1, select("#", ...), ...)
        end
        collect(frame:GetChildren())
        return tbl
    end

    function HANDLE_METHODS:GetFrameRef(label)
        local frame = check_handle(self)
        if type(label) ~= "string" then
            return nil
        end
        local ref = frame:GetAttribute("_frameref-" .. string_lower(label))
        return GetFrameHandle(ref)
    end

    function HANDLE_METHODS:SetFrameRef(label, ref)
        local frame = check_handle(self)
        if type(label) ~= "string" then
            error("Usage: handle:SetFrameRef(label, frame)", 2)
        end
        local raw = ref == nil and nil or unwrap_if_handle(ref)
        if raw ~= nil and not is_frame_wrapper(raw) then
            error("Invalid frame reference", 2)
        end
        return frame:SetAttribute("_frameref-" .. string_lower(label), raw)
    end

    -- Attributes
    function HANDLE_METHODS:GetAttribute(...)
        local frame = check_handle(self)
        local v = frame:GetAttribute(...)
        if is_frame_wrapper(v) then
            return GetFrameHandle(v)
        end
        return v
    end

    function HANDLE_METHODS:SetAttribute(name, value)
        local frame = check_handle(self)
        return frame:SetAttribute(name, unwrap_if_handle(value))
    end

    -- Visibility & enable
    function HANDLE_METHODS:Show()
        return check_handle(self):Show()
    end

    function HANDLE_METHODS:Hide()
        return check_handle(self):Hide()
    end

    function HANDLE_METHODS:IsShown()
        return check_handle(self):IsShown()
    end

    function HANDLE_METHODS:IsVisible()
        return check_handle(self):IsVisible()
    end

    function HANDLE_METHODS:Enable()
        local frame = check_handle(self)
        if frame.Enable then
            return frame:Enable()
        end
    end

    function HANDLE_METHODS:Disable()
        local frame = check_handle(self)
        if frame.Disable then
            return frame:Disable()
        end
    end

    function HANDLE_METHODS:IsEnabled()
        local frame = check_handle(self)
        if frame.IsEnabled then
            return frame:IsEnabled()
        end
        return nil
    end

    -- Geometry, scale, alpha, strata, anchors
    function HANDLE_METHODS:GetWidth() return check_handle(self):GetWidth() end
    function HANDLE_METHODS:GetHeight() return check_handle(self):GetHeight() end
    function HANDLE_METHODS:SetWidth(w) return check_handle(self):SetWidth(w) end
    function HANDLE_METHODS:SetHeight(h) return check_handle(self):SetHeight(h) end
    function HANDLE_METHODS:SetSize(w, h)
        local frame = check_handle(self)
        if frame.SetSize then
            return frame:SetSize(w, h)
        end
        frame:SetWidth(w)
        frame:SetHeight(h)
    end
    function HANDLE_METHODS:GetScale() return check_handle(self):GetScale() end
    function HANDLE_METHODS:SetScale(s) return check_handle(self):SetScale(s) end
    function HANDLE_METHODS:GetEffectiveScale() return check_handle(self):GetEffectiveScale() end
    function HANDLE_METHODS:GetAlpha() return check_handle(self):GetAlpha() end
    function HANDLE_METHODS:SetAlpha(a) return check_handle(self):SetAlpha(a) end
    function HANDLE_METHODS:GetEffectiveAlpha()
        local frame = check_handle(self)
        if frame.GetEffectiveAlpha then
            return frame:GetEffectiveAlpha()
        end
        return frame:GetAlpha()
    end
    function HANDLE_METHODS:GetFrameLevel() return check_handle(self):GetFrameLevel() end
    function HANDLE_METHODS:SetFrameLevel(lvl) return check_handle(self):SetFrameLevel(lvl) end
    function HANDLE_METHODS:GetFrameStrata() return check_handle(self):GetFrameStrata() end
    function HANDLE_METHODS:SetFrameStrata(s) return check_handle(self):SetFrameStrata(s) end
    function HANDLE_METHODS:GetRect() return check_handle(self):GetRect() end
    function HANDLE_METHODS:GetLeft() return check_handle(self):GetLeft() end
    function HANDLE_METHODS:GetRight() return check_handle(self):GetRight() end
    function HANDLE_METHODS:GetTop() return check_handle(self):GetTop() end
    function HANDLE_METHODS:GetBottom() return check_handle(self):GetBottom() end
    function HANDLE_METHODS:GetCenter() return check_handle(self):GetCenter() end
    function HANDLE_METHODS:GetNumPoints() return check_handle(self):GetNumPoints() end

    function HANDLE_METHODS:GetPoint(index)
        local frame = check_handle(self)
        local pt, relTo, relPt, x, y = frame:GetPoint(index)
        if pt == nil then
            return
        end
        return pt, (relTo ~= nil and GetFrameHandle(relTo) or nil), relPt, x, y
    end

    function HANDLE_METHODS:ClearAllPoints()
        return check_handle(self):ClearAllPoints()
    end

    function HANDLE_METHODS:SetPoint(point, relTo, ...)
        local frame = check_handle(self)
        if select("#", ...) == 0 then
            if relTo == nil then
                return frame:SetPoint(point)
            end
            return frame:SetPoint(point, unwrap_if_handle(relTo))
        end
        return frame:SetPoint(point, unwrap_if_handle(relTo), ...)
    end

    function HANDLE_METHODS:SetAllPoints(relTo)
        local frame = check_handle(self)
        if relTo == nil then
            return frame:SetAllPoints()
        end
        return frame:SetAllPoints(unwrap_if_handle(relTo))
    end

    function HANDLE_METHODS:IsUnderMouse(_recursive)
        local frame = check_handle(self)
        if frame.IsMouseOver then
            return not not frame:IsMouseOver()
        end
        return false
    end

    function HANDLE_METHODS:GetMousePosition()
        check_handle(self)
        return nil, nil
    end

    -- Restricted execution & dispatch
    function HANDLE_METHODS:CallMethod(methodName, ...)
        local frame = check_handle(self)
        if type(methodName) ~= "string" then
            error("Invalid method name", 2)
        end
        local fn = frame[methodName]
        if type(fn) ~= "function" then
            error("Invalid method '" .. methodName .. "'", 2)
        end
        fn(frame, ...)
    end

    function HANDLE_METHODS:Run(body, ...)
        check_handle(self)
        return run_snippet(self, self, body, ...)
    end

    function HANDLE_METHODS:RunAttribute(attrName, ...)
        local frame = check_handle(self)
        if type(attrName) ~= "string" then
            return
        end
        local body = frame:GetAttribute(attrName)
        if type(body) == "string" then
            return run_snippet(self, self, body, ...)
        end
    end

    function HANDLE_METHODS:RunFor(otherHandle, body, ...)
        check_handle(self)
        local target = GetFrameHandle(otherHandle)
        if not target then
            error("Invalid target frame handle", 2)
        end
        return run_snippet(self, target, body, ...)
    end

    function HANDLE_METHODS:ChildUpdate(snippetid, message)
        local frame = check_handle(self)
        local attr = type(snippetid) == "string" and ("_childupdate-" .. snippetid) or "_childupdate"
        local function visit(i, n, child, ...)
            if i > n then
                return
            end
            if is_frame_wrapper(child) then
                local body = child:GetAttribute(attr)
                if body == nil and attr ~= "_childupdate" then
                    body = child:GetAttribute("_childupdate")
                end
                if type(body) == "string" then
                    local childHandle = GetFrameHandle(child)
                    if childHandle ~= nil then
                        run_snippet(self, childHandle, body, snippetid, message)
                    end
                end
            end
            return visit(i + 1, n, ...)
        end
        local function dispatch(...)
            return visit(1, select("#", ...), ...)
        end
        dispatch(frame:GetChildren())
    end

    -- Override bindings & auto-hide helpers used by secure action bar addons
    function HANDLE_METHODS:SetBinding(priority, key, command)
        local frame = check_handle(self)
        if _G.SetOverrideBinding then
            _G.SetOverrideBinding(frame, priority, key, command)
        end
    end

    function HANDLE_METHODS:SetBindingClick(priority, key, buttonName, mouseButton)
        local frame = check_handle(self)
        if _G.SetOverrideBindingClick then
            _G.SetOverrideBindingClick(frame, priority, key, buttonName, mouseButton)
        end
    end

    function HANDLE_METHODS:SetBindingItem(priority, key, item)
        local frame = check_handle(self)
        if _G.SetOverrideBindingItem then
            _G.SetOverrideBindingItem(frame, priority, key, item)
        end
    end

    function HANDLE_METHODS:SetBindingMacro(priority, key, macro)
        local frame = check_handle(self)
        if _G.SetOverrideBindingMacro then
            _G.SetOverrideBindingMacro(frame, priority, key, macro)
        end
    end

    function HANDLE_METHODS:SetBindingSpell(priority, key, spell)
        local frame = check_handle(self)
        if _G.SetOverrideBindingSpell then
            _G.SetOverrideBindingSpell(frame, priority, key, spell)
        end
    end

    function HANDLE_METHODS:ClearBinding(key)
        local frame = check_handle(self)
        if _G.SetOverrideBinding then
            _G.SetOverrideBinding(frame, false, key, nil)
        end
    end

    function HANDLE_METHODS:ClearBindings()
        local frame = check_handle(self)
        if _G.ClearOverrideBindings then
            _G.ClearOverrideBindings(frame)
        end
    end

    function HANDLE_METHODS:RegisterAutoHide(_delay)
        check_handle(self)
    end

    function HANDLE_METHODS:UnregisterAutoHide()
        check_handle(self)
    end

    function HANDLE_METHODS:AddToAutoHide(other)
        check_handle(self)
        if other ~= nil and not GetFrameHandle(other) then
            error("Invalid frame handle for AddToAutoHide", 2)
        end
    end

    HANDLE_MT.__index = HANDLE_METHODS
    HANDLE_MT.__newindex = function()
        error("Attempt to modify a restricted frame handle", 2)
    end
    HANDLE_MT.__tostring = function(self)
        local frame = HANDLE_TO_FRAME[self]
        local name = frame and frame:GetName() or nil
        if name then
            return "FrameHandle<" .. tostring(name) .. ">"
        end
        return "FrameHandle"
    end
    HANDLE_MT.__metatable = false

    _G.GetFrameHandle = GetFrameHandle
    _G.IsFrameHandle = IsFrameHandle
    _G.GetFrameHandleFrame = GetFrameHandleFrame
    _G.GetManagedFrame = GetManagedFrame

    local function run_for_frame(frame_or_handle, body)
        local handle = GetFrameHandle(frame_or_handle)
        if not handle then
            error("Invalid frame handle", 2)
        end
        return handle:Run(body)
    end

    return FRAME_TO_HANDLE, HANDLE_TO_FRAME, run_for_frame
end
"##;

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    // `scrub(...)` returns its arguments unchanged in the 12340 restricted-execution bootstrap;
    // stock `RestrictedExecution.lua`/`SecureHandlers.lua` capture it at load and fail without it.
    lua.globals().set(
        "scrub",
        lua.create_function(|_, args: MultiValue| Ok(args))?,
    )?;

    // Install the `newproxy`-backed restricted frame-handle proxy system and expose its weak
    // lookup tables in the registry for Rust-side `secure.rs` resolution.
    let frame_check = lua.create_function(|_, value: Value| Ok(is_frame_wrapper(&value)))?;
    let init: Function = lua
        .load(FRAME_HANDLE_LUA)
        .set_name("benilla:wrath-frame-handles")
        .eval()?;
    let (frame_to_handle, handle_to_frame, run_for_frame): (Table, Table, Function) =
        init.call(frame_check)?;
    lua.set_named_registry_value(REG_FRAME_TO_HANDLE, frame_to_handle)?;
    lua.set_named_registry_value(REG_HANDLE_TO_FRAME, handle_to_frame)?;
    lua.set_named_registry_value(REG_RUN_RESTRICTED, run_for_frame)?;

    // `forceinsecure()`: the host never enforces taint, so the call is a no-op.
    lua.globals()
        .set("forceinsecure", lua.create_function(|_, ()| Ok(()))?)?;
    // `securecallfunction(f, ...)`: the function-value form of securecall.
    lua.globals().set(
        "securecallfunction",
        lua.create_function(|_, (f, args): (Function, MultiValue)| f.call::<MultiValue>(args))?,
    )?;

    let issecure = lua.create_function(|lua, _args: ()| {
        Ok(matches!(
            super::taint::current(lua),
            super::taint::Context::Client
        ))
    })?;
    lua.globals().set("issecure", issecure)?;

    lua.load(
        r#"
function securecall(f, ...)
    if type(f) == "string" then
        f = _G[f]
    end
    return f(...)
end
"#,
    )
    .set_name("benilla:wrath-secure-natives")
    .exec()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{UiScript, UiTarget};

    /// The restricted frame-handle binding must return a distinct, cached userdata proxy for a
    /// live frame, reject plain tables, and stay idempotent when passed an existing handle (the
    /// `SecureHandlers.lua:108` path).
    #[test]
    fn get_frame_handle_accepts_a_wrath_frame_and_rejects_a_plain_table() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let result: (bool, bool, bool) = s
            .eval(
                "local f = CreateFrame('Frame')
                 local h = GetFrameHandle(f)
                 return h ~= nil, IsFrameHandle(h), IsFrameHandle({})",
            )
            .unwrap();
        assert_eq!(result, (true, true, false));
    }

    #[test]
    fn frame_handle_proxy_identity_caching_and_unwrapping() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let ok: bool = s
            .eval(
                r#"
                local f = CreateFrame("Frame", "HandleIdentityTestFrame")
                -- create=false returns nil before the handle exists
                assert(GetFrameHandle(f, false) == nil)
                local h1 = GetFrameHandle(f)
                local h2 = GetFrameHandle(f)
                local h3 = GetFrameHandle(h1)
                local raw, managed = GetFrameHandleFrame(h1)
                assert(type(h1) == "userdata")
                assert(h1 ~= f)
                assert(h1 == h2)
                assert(h1 == h3)
                assert(GetFrameHandle(f, false) == h1)
                assert(IsFrameHandle(h1) == true)
                assert(IsFrameHandle(f) == false)
                assert(IsFrameHandle(nil) == false)
                assert(raw == f and managed == true)
                assert(GetManagedFrame(h1) == f)
                assert(getmetatable(h1) == false)
                -- Attempting to mutate the handle must raise
                local mutated = pcall(function() h1.foo = 1 end)
                assert(not mutated)
                return true
                "#,
            )
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn frame_handle_routes_methods_hierarchy_refs_and_restricted_execution() {
        let s = UiScript::with_target(UiTarget::Wrath335).unwrap();
        let ok: bool = s
            .eval(
                r#"
                local parent = CreateFrame("Frame", "HandleParentFrame", nil, "SecureHandlerStateTemplate")
                local child = CreateFrame("Button", "HandleChildButton", parent, "SecureActionButtonTemplate")
                local refTarget = CreateFrame("Frame", "HandleRefTarget")

                local hp = GetFrameHandle(parent)
                local hc = GetFrameHandle(child)
                local hr = GetFrameHandle(refTarget)
                -- Hierarchy returns handles, never raw frames
                assert(hc:GetParent() == hp)
                assert(hp:GetChildren() == hc)
                local list = {}
                hp:GetChildList(list)
                assert(#list == 1 and list[1] == hc)

                -- FrameRefs round-trip as handles on the handle and raw frames on the underlying frame
                parent:SetFrameRef("target", refTarget)
                assert(hp:GetFrameRef("target") == hr)
                assert(parent:GetFrameRef("target") == refTarget)
                hp:SetFrameRef("child", hc)
                assert(hp:GetFrameRef("child") == hc)
                assert(parent:GetFrameRef("child") == child)
                -- Attributes, visibility, size, ID
                hp:SetAttribute("state-page", "3")
                assert(parent:GetAttribute("state-page") == "3")
                assert(hp:GetAttribute("state-page") == "3")
                hp:SetID(42)
                assert(hp:GetID() == 42 and parent:GetID() == 42)
                hp:Hide()
                assert(not hp:IsShown())
                hp:Show()
                assert(hp:IsShown())
                hp:SetSize(120, 48)
                -- Wrath layout rounds a script length through the screen cells; one unit is the
                -- model's step, not a proxy error.
                assert(math.abs(hp:GetWidth() - 120) <= 1 and math.abs(hp:GetHeight() - 48) <= 1)
                -- Anchors unwrap handles cleanly and re-wrap on GetPoint
                hc:ClearAllPoints()
                hc:SetPoint("TOPLEFT", hp, "TOPLEFT", 4, -6)
                local pt, relTo, relPt, x, y = hc:GetPoint(1)
                assert(pt == "TOPLEFT" and relTo == hp and relPt == "TOPLEFT")
                assert(math.abs(x - 4) <= 1 and math.abs(y + 6) <= 1)
                -- CallMethod invokes the underlying frame method
                local called_with = nil
                function parent:CustomPing(arg)
                    called_with = (self == parent) and arg or "bad-self"
                end
                hp:CallMethod("CustomPing", "ok")
                assert(called_with == "ok")
                -- Run, RunAttribute, RunFor, and ChildUpdate execute with handle identity intact
                local ran_id = hp:Run([[
                    assert(IsFrameHandle(self))
                    assert(GetFrameHandle(self) == self)
                    local t = self:GetFrameRef("target")
                    t:SetAttribute("from-run", "yes")
                    return self:GetID()
                ]])
                assert(ran_id == 42)
                assert(refTarget:GetAttribute("from-run") == "yes")
                parent:SetAttribute("_onstate-test", [[
                    local arg = ...
                    self:SetAttribute("ran-attr", arg)
                ]])
                hp:RunAttribute("_onstate-test", "page-7")
                assert(parent:GetAttribute("ran-attr") == "page-7")
                hp:RunFor(hc, [[
                    self:SetAttribute("from-runfor", control:GetName())
                ]])
                assert(child:GetAttribute("from-runfor") == "HandleParentFrame")
                child:SetAttribute("_childupdate-page", [[
                    local id, msg = ...
                    self:SetAttribute("child-msg", id .. ":" .. tostring(msg))
                ]])
                hp:ChildUpdate("page", 9)
                assert(child:GetAttribute("child-msg") == "page:9")
                -- SecureHandler:Execute runs inside the restricted handle environment
                parent:Execute([[
                    assert(IsFrameHandle(self))
                    self:SetAttribute("from-execute", self:GetFrameRef("target"):GetName())
                ]])
                assert(parent:GetAttribute("from-execute") == "HandleRefTarget")

                return true
                "#,
            )
            .unwrap();
        assert!(ok);
    }
}
