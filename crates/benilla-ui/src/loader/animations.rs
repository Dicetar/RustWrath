//! The wrath `<Animations>` arm of the loader: `AnimationGroup`s and the animations inside them
//! become live objects ([`crate::script::animation`]) under the frame or region that declares
//! them, named (`$parent` expanded) and published like every other named XML child, with their
//! `parentKey` put on the declaring object. The stock UI addresses these objects at load:
//! `TutorialFrame.lua:790` stops `TutorialFrameCallOutPulser`, the named `Alpha` inside
//! `TutorialFrameCallOut`'s group.

use mlua::{ObjectLike, Table};

use crate::framexml::Element;

use super::{children_named, Loader};

impl Loader<'_> {
    /// Every `<Animations>` block of `el`, in document order; `owner_name` is the name `$parent`
    /// expands from (the declaring frame or region's own name).
    pub(super) fn apply_animations(
        &mut self,
        el: &Element,
        owner: &Table,
        owner_name: &str,
        dbg: &str,
    ) {
        if self.model().target != crate::script::UiTarget::Wrath335 {
            return;
        }
        for animations in children_named(el, "Animations") {
            for child in &animations.children {
                self.build_animation(child, owner, owner_name, None, dbg);
            }
        }
    }

    /// One animation element and, for a group, the animations inside it. Its `Scripts` handlers are
    /// compiled and stored through the object's `SetScript`; the animation engine fires them.
    fn build_animation(
        &mut self,
        el: &Element,
        owner: &Table,
        owner_name: &str,
        parent: Option<&Table>,
        dbg: &str,
    ) -> Option<Table> {
        let name = el
            .name()
            .map(|raw| crate::framexml::resolve_name(raw, owner_name));
        let animation = match crate::script::animation::build(
            self.lua(),
            &el.tag,
            name.as_deref(),
            el.attrs(),
            owner,
            parent,
        ) {
            Ok(animation) => animation,
            Err(error) => {
                self.report
                    .raise(format!("{dbg}: <{}>: {error}", el.tag), &error);
                return None;
            }
        };
        if let Some(name) = &name {
            if let Err(error) = self.lua().globals().set(name.clone(), animation.clone()) {
                self.report
                    .raise(format!("{dbg}: animation global {name:?}: {error}"), &error);
            }
        }
        if let Some(key) = el.attr("parentKey").filter(|key| !key.is_empty()) {
            if let Err(error) = owner.raw_set(key, animation.clone()) {
                self.report.raise(
                    format!("{dbg}: animation parentKey {key:?}: {error}"),
                    &error,
                );
            }
        }
        let chunk_owner = name.as_deref().unwrap_or("<unnamed>");
        for scripts in children_named(el, "Scripts") {
            for handler in &scripts.children {
                let handler_name = handler.tag.clone();
                if let Some(func) =
                    self.compile_handler(handler, &handler_name, chunk_owner, dbg)
                {
                    if let Err(error) = animation
                        .call_method::<()>("SetScript", (handler_name.clone(), Some(func)))
                    {
                        self.report
                            .raise(format!("{dbg}: SetScript({handler_name:?}): {error}"), &error);
                    }
                }
            }
        }
        // A group's children are its animations; `$parent` names the group when it is named, else
        // the declaring object, as the XML walker expands it. The children keep the frame or
        // region as their owner (`GetRegionParent`), with the group only their sequence parent.
        if el.tag.eq_ignore_ascii_case("AnimationGroup") {
            let children = match self.lua().create_table() {
                Ok(children) => children,
                Err(error) => {
                    self.report.raise(format!("{dbg}: animation children: {error}"), &error);
                    return Some(animation);
                }
            };
            let mut count = 0;
            for child in &el.children {
                if child.tag.eq_ignore_ascii_case("Scripts") {
                    continue;
                }
                if let Some(built) = self.build_animation(
                    child,
                    owner,
                    name.as_deref().unwrap_or(owner_name),
                    Some(&animation),
                    dbg,
                ) {
                    count += 1;
                    if let Err(error) = children.raw_set(count, built) {
                        self.report
                            .raise(format!("{dbg}: animation child {count}: {error}"), &error);
                    }
                }
            }
            if let Err(error) = crate::script::animation::link_children(&animation, children) {
                self.report
                    .raise(format!("{dbg}: animation group children: {error}"), &error);
            }
        }
        Some(animation)
    }
}
