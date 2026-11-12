use mlua::{Function, ObjectLike, Table, Value};

use crate::framexml::Element;

use super::{children_named, Loader};

impl Loader<'_> {
    /// `<Scripts>` (`0x769ef0`): `SetScript` for each handler. Returns the `OnLoad`, which the
    /// caller fires bottom-up.
    pub(super) fn apply_scripts(
        &mut self,
        el: &Element,
        wrapper: &Table,
        dbg: &str,
    ) -> Option<Function> {
        let mut onload = None;
        // A handler's chunk name is `"<name>:<Handler>"`, the frame's own name or `<unnamed>`
        // (`0x7025fd`-`0x70263c`), never the loader's `dbg`.
        for scripts in children_named(el, "Scripts") {
            let owner = crate::script::binding_abi::name_of(self.lua, wrapper);
            for handler in &scripts.children {
                let name = handler.tag.clone();
                // An empty body clears the handler to nil, but a whitespace one is an empty
                // function: `SetScript` (`0x7025c0`) tests only NULL and the first byte
                // (`0x7025f8`), and nothing trims the body (`0x6f29d0`). The stock blanks
                // (`BuffFrame.xml:101`) are whitespace, so their `GetScript` answers a function.
                // A wrath `function=""` (`UIPanelTemplates.xml:365`, `:396`, the faux scroll
                // templates overriding an inherited handler) is the same clear: the real client's
                // global lookup finds no function and installs nil.
                let wrath = self.model().target == crate::script::UiTarget::Wrath335;
                let cleared = handler.body.is_empty()
                    && (!wrath
                        || handler
                            .attr("function")
                            .is_none_or(str::is_empty));
                let func = if cleared {
                    None
                } else {
                    match self.compile_handler(handler, &name, &owner, dbg) {
                        Some(f) => Some(f),
                        None => continue,
                    }
                };
                if let Err(e) = wrapper.call_method::<()>("SetScript", (name.clone(), func.clone()))
                {
                    self.warn_once(
                        &format!("script:{name}"),
                        format!("{dbg}: SetScript(\"{name}\") unsupported in v1: {e}"),
                    );
                    continue;
                }
                // After each `SetScript` the XML walker enables the handler's input kind
                // (`0x769ef0` → `0x76af00`): these five arm the mouse (`OnDragStop` and
                // `OnReceiveDrag` do not), `OnMouseWheel` the wheel (kind 3). The Lua `SetScript`
                // never does (`0x7748d0`).
                const MOUSE_KIND: [&str; 5] = [
                    "OnEnter",
                    "OnLeave",
                    "OnMouseDown",
                    "OnMouseUp",
                    "OnDragStart",
                ];
                if MOUSE_KIND.iter().any(|k| name.eq_ignore_ascii_case(k)) {
                    self.call(wrapper, "EnableMouse", true, dbg);
                }
                if name.eq_ignore_ascii_case("OnMouseWheel") {
                    self.call(wrapper, "EnableMouseWheel", true, dbg);
                }
                // The key handlers, the same rule, one bit each: `OnChar` the char bit,
                // `OnKeyDown` and `OnKeyUp` the key bit (`0x769f6b`, `0x769fa4`).
                if let Err(e) = crate::script::enable_for_script(self.lua, wrapper, &name) {
                    self.report.fault(format!("{dbg}: {name}: {e}"));
                }
                if name.eq_ignore_ascii_case("OnLoad") {
                    // A cleared `OnLoad` also unsets a template's, or the caller would fire it.
                    onload = func;
                }
            }
        }
        onload
    }

    /// Compile a handler body as the chunk itself, as the reference loads it (`0x704c70`): it takes
    /// no arguments and reads its frame from `this`, so a `self` in it is an ordinary global. The
    /// loader reads no other attribute of the element (no `function=`).
    pub(super) fn compile_handler(
        &mut self,
        handler: &Element,
        name: &str,
        owner: &str,
        dbg: &str,
    ) -> Option<Function> {
        // The raw body, never trimmed: a whitespace body is a real empty chunk (`apply_scripts`).
        let body = handler.body.as_str();
        let wrath = self.model().target == crate::script::UiTarget::Wrath335;
        if wrath {
            if let Some(function) = handler.attr("function") {
                if !body.trim().is_empty() {
                    self.report.fault(format!("{dbg}: <{name}> combines function= and an inline body; this form is not implemented"));
                    return None;
                }
                let result = named_handler(self.lua(), function);
                return match result {
                    Ok(function) => Some(function),
                    Err(error) => {
                        self.report.raise(format!("{dbg}: <{name} function=\"{function}\">: {error}"), &error);
                        None
                    }
                };
            }
        }
        if body.is_empty() {
            return None;
        }
        let result = if wrath {
            let source = format!("return function({}) {body}\nend", handler_parameters(name));
            self.lua().load(source).set_name(format!("{owner}:{name}"))
                .set_mode(mlua::ChunkMode::Text).eval::<Function>()
        } else {
            self.lua().load(body).set_name(format!("{owner}:{name}"))
                .set_mode(mlua::ChunkMode::Text).into_function()
        };
        match result {
            Ok(f) => {
                // An insecure addon's handler is its code whoever fires it ([`crate::script::taint`]).
                crate::script::taint::tag_for_load(self.lua(), &f);
                Some(f)
            }
            Err(e) => {
                self.report
                    .raise(format!("{dbg}: compiling <{name}>: {e}"), &e);
                None
            }
        }
    }

    /// Fire a captured `OnLoad` under the event path's convention, `this` set and restored
    /// (`0x704d50`); an error is recorded, never propagated.
    pub(super) fn fire_onload(&mut self, wrapper: &Table, func: &Function, dbg: &str) {
        if let Err(e) = self.invoke_handler(wrapper, func) {
            self.report.raise(format!("{dbg}: OnLoad: {e}"), &e);
        }
    }

    /// Call a frame-wrapper method, recording (not propagating) any error.
    pub(super) fn call(
        &mut self,
        wrapper: &Table,
        method: &str,
        args: impl mlua::IntoLuaMulti,
        dbg: &str,
    ) {
        if let Err(e) = wrapper.call_method::<()>(method, args) {
            self.report.fault(format!("{dbg}: {method}: {e}"));
        }
    }

    /// Install the texture a widget element's child (`<BarTexture>`, `<ThumbTexture>`, a ColorSelect
    /// texture) built as the widget's object, through the engine's own install, recording (not
    /// propagating) any error: the Lua setters take the object too, but the loader needs no Lua
    /// call to say it.
    pub(super) fn install_texture(
        &mut self,
        wrapper: &Table,
        tag: &str,
        texture: &Table,
        layer: Option<&str>,
        dbg: &str,
    ) {
        if let Err(e) =
            crate::script::install_loaded_texture(self.lua, wrapper, tag, texture, layer)
        {
            self.report.fault(format!("{dbg}: <{tag}>: {e}"));
        }
    }

    /// [`Self::call_region`] on a region wrapper, else [`Self::call`].
    pub(super) fn call_on(
        &mut self,
        wrapper: &Table,
        region: bool,
        method: &str,
        args: impl mlua::IntoLuaMulti,
        dbg: &str,
    ) {
        if region {
            self.call_region(wrapper, method, args, dbg);
        } else {
            self.call(wrapper, method, args, dbg);
        }
    }

    /// Call a region-wrapper method, recording (not propagating) any error.
    pub(super) fn call_region(
        &mut self,
        region: &Table,
        method: &str,
        args: impl mlua::IntoLuaMulti,
        dbg: &str,
    ) {
        if let Err(e) = region.call_method::<()>(method, args) {
            self.report.fault(format!("{dbg}: region {method}: {e}"));
        }
    }
}

/// Resolve a named handler without evaluating the attribute as arbitrary Lua source.
fn named_handler(lua: &mlua::Lua, name: &str) -> mlua::Result<Function> {
    let mut value = Value::Table(lua.globals());
    for part in name.split('.') {
        if part.is_empty() || !part.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        }) {
            return Err(mlua::Error::runtime("invalid named script handler"));
        }
        let Value::Table(table) = value else {
            return Err(mlua::Error::runtime("named script handler parent is not a table"));
        };
        value = table.get(part)?;
    }
    match value {
        Value::Function(function) => Ok(function),
        _ => Err(mlua::Error::runtime("named script handler is not a function")),
    }
}

fn handler_parameters(name: &str) -> &'static str {
    match name {
        "OnEvent" => "self, event, ...",
        "OnUpdate" => "self, elapsed, ...",
        "OnClick" | "PreClick" | "PostClick" => "self, button, down, ...",
        "OnAttributeChanged" => "self, name, value, ...",
        "OnDoubleClick" | "OnMouseDown" | "OnMouseUp" => "self, button, ...",
        "OnMouseWheel" => "self, delta, ...",
        "OnEnter" | "OnLeave" => "self, motion, ...",
        "OnKeyDown" | "OnKeyUp" => "self, key, ...",
        "OnChar" => "self, text, ...",
        "OnSizeChanged" => "self, width, height, ...",
        "OnValueChanged" => "self, value, ...",
        "OnTextChanged" => "self, userInput, ...",
        "OnCursorChanged" => "self, x, y, width, height, ...",
        "OnTooltipAddMoney" => "self, cost, maxcost, ...",
        "OnInputLanguageChanged" => "self, language, ...",
        "OnHorizontalScroll" | "OnVerticalScroll" => "self, offset, ...",
        "OnScrollRangeChanged" => "self, xrange, yrange, ...",
        "OnHyperlinkClick" => "self, link, text, button, ...",
        "OnHyperlinkEnter" | "OnHyperlinkLeave" => "self, link, text, ...",
        "OnColorSelect" => "self, r, g, b, ...",
        "OnMovieShowSubtitle" => "self, text, ...",
        _ => "self, ...",
    }
}
