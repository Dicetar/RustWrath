//! The frame object model (`0x701bd0`).

use super::common::script;

/// The client publishes one frame method table per kind, with the base methods in it, and addon
/// code enumerates it to forward the API (Details' DF, `Libs/DF/button.lua:1052`). Lua 5.1's
/// `pairs` does not follow `__index`, so the inherited entries must be in the table itself.
#[test]
fn a_frame_method_table_enumerates_its_inherited_methods() {
    let s = script();
    let ok: bool = s
        .eval(
            r#"
        local button = CreateFrame("Button")
        local methods = getmetatable(button).__index
        local seen = {}
        for name, fn in pairs(methods) do seen[name] = fn end
        return type(seen.SetBackdrop) == "function"
            and type(seen.Show) == "function"
            and type(seen.SetText) == "function"
            and methods.SetScrollChild == nil
    "#,
        )
        .unwrap();
    assert!(ok);
}
#[test]
fn create_frame_publishes_global_and_names_resolve() {
    let s = script();
    s.run(
        r#"
        local p = CreateFrame("Frame", "ParentF")
        local c = CreateFrame("Frame", "ChildF", p)
        assert(ParentF == p, "named frame auto-publishes to _G")
        assert(getglobal("ParentF") == p, "getglobal resolves the published name")
        assert(p:GetName() == "ParentF")
        assert(c:GetParent() == p, "GetParent returns the parent wrapper")
        assert(c:GetParent():GetName() == "ParentF")
    "#,
    )
    .unwrap();
}

#[test]
fn wrapper_identity_is_stable_across_lookups() {
    let s = script();
    let same: bool = s
        .eval(
            r#"
        local p = CreateFrame("Frame", "IdF")
        local c = CreateFrame("Frame", nil, p)
        -- GetParent twice, and the _G publish, must all be the SAME table (cached wrapper).
        return (c:GetParent() == c:GetParent()) and (c:GetParent() == p) and (IdF == p)
    "#,
        )
        .unwrap();
    assert!(same, "wrapper identity must be cached/stable");
}

#[test]
fn non_overwriting_named_publish() {
    let s = script();
    let ok: bool = s
        .eval(
            r#"
        local a = CreateFrame("Frame", "Dup")
        local b = CreateFrame("Frame", "Dup")   -- same name; must NOT overwrite _G
        return (Dup == a) and (a ~= b)
    "#,
        )
        .unwrap();
    assert!(ok);
}
