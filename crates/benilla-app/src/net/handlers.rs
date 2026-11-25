//! The inbound handler table, the reference's shape: `NetClient` holds an opcode → handler table
//! (`+0x74`, 828 slots) and its dispatcher `0x537aa0` calls each packet's handler in packet order,
//! dropping an unregistered opcode in silence. Here each subsystem registers one-shot systems per
//! [`SessionEventKind`] through [`NetHandlerApp::net_handler`], and the exclusive drain runs them
//! in packet order, each seeing what the one before it did.
//!
//! The core names no feature here. Where a reference handler is the session's work with a
//! feature's inline (a chat line, a pane's callback), the core registers its half and reserves a
//! [`HandlerSlot`] beside it ([`NetHandlerApp::net_slot`]); the feature fills the slot from its own
//! plugin ([`NetHandlerApp::net_handler_in`]), and its half runs where the core put it, whatever
//! the plugin order.

use std::collections::HashMap;

use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::ecs::system::SystemId;
use bevy::prelude::*;

/// One registered handler: the one-shot system and its name for the census.
type Handler = (SystemId<In<SessionEvent>>, &'static str);

/// A place the core reserves among one kind's handlers for a feature's half of a reference
/// handler. Each names the reference work that stands there, never the module that fills it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum HandlerSlot {
    /// `SMSG_SET_FACTION_STANDING`'s chat lines, the "increased by" line and then the rank lines
    /// (`0x4d57f5`, `0x4d5896`), ahead of the store write they read the deltas against.
    StandingLines,
    /// What a standing message's latches drive past the store
    /// ([`crate::net::StandingLatches`]): the quest log's rebuild on a moved rank (`0x4d58ea`),
    /// the reputation tutorial (`0x4d5963`) and the questgiver re-ask.
    StandingFollowUps,
    /// `SMSG_SET_FACTION_VISIBLE`'s chat line, which names the rank before the flag lands.
    VisibleLine,
    /// A worldport's roster hook: every streamed roster member is about to be freed, and each
    /// one's destructor runs it (`0x5dd6a0` → `0x4e85d0` mode 0), ahead of the teardown.
    WorldportRoster,
    /// `SMSG_ITEM_QUERY_SINGLE_RESPONSE`'s loot window callbacks (`0x4c2ac0` for a row,
    /// `0x491ee0` for a push), each at the stamp the core took at its place among the record's
    /// callbacks ([`crate::net::LootAnswers`]).
    ItemLootCallbacks,
    /// Past the record callbacks: the combat log's callback for its held lines (`0x6294b0`).
    ItemHeldLines,
    /// Then the auction lists' callbacks (`0x4cc9bf`, `0x4ccbff`, `0x4cce4f`).
    ItemAuctionLists,
    /// The session's end, past the bridge's teardown: the cooldown list, which
    /// `SMSG_INITIAL_SPELLS` refills at the next world entry.
    SessionCooldowns,
}

impl HandlerSlot {
    /// The one kind this slot sits among.
    pub(crate) fn kind(self) -> SessionEventKind {
        use SessionEventKind as K;
        match self {
            Self::StandingLines | Self::StandingFollowUps => K::ReputationDelta,
            Self::VisibleLine => K::ReputationVisible,
            Self::WorldportRoster => K::Worldport,
            Self::ItemLootCallbacks | Self::ItemHeldLines | Self::ItemAuctionLists => {
                K::ItemTemplate
            }
            Self::SessionCooldowns => K::Disconnected,
        }
    }
}

/// One place in a kind's list: a handler, or a reserved slot and what filled it.
enum Entry {
    One(Handler),
    Slot(HandlerSlot, Vec<Handler>),
}

impl Entry {
    fn handlers(&self) -> &[Handler] {
        match self {
            Self::One(h) => std::slice::from_ref(h),
            Self::Slot(_, hs) => hs,
        }
    }

    #[cfg(test)]
    fn handlers_mut(&mut self) -> &mut [Handler] {
        match self {
            Self::One(h) => std::slice::from_mut(h),
            Self::Slot(_, hs) => hs,
        }
    }
}

/// The table, filled at plugin build and read by the drain.
#[derive(Resource, Default)]
pub(crate) struct NetHandlers {
    by_kind: HashMap<SessionEventKind, Vec<Entry>>,
    /// Fills of a slot the core has not reserved yet: a plugin built ahead of the core's. The
    /// reservation takes them, in their order.
    early: HashMap<HandlerSlot, Vec<Handler>>,
    /// What runs after every handler, before the next ([`NetHandlerApp::net_after_each_handler`]).
    after_each: Vec<fn(&mut World)>,
    /// Every handler in registration order, for [`measure`].
    #[cfg(test)]
    registered: Vec<Handler>,
    /// Each handler's declared access, by name, in registration order, once [`measure`] ran.
    #[cfg(test)]
    accesses: Option<Vec<(&'static str, bevy::ecs::query::FilteredAccessSet)>>,
}

impl NetHandlers {
    /// Each handler's declared access, by name, in registration order, for the tests that ask what
    /// the handlers write. Panics before [`measure`].
    #[cfg(test)]
    pub(crate) fn accesses(&self) -> &[(&'static str, bevy::ecs::query::FilteredAccessSet)] {
        self.accesses
            .as_deref()
            .expect("the handlers' accesses are read after `net::handlers::measure`")
    }

    /// Every kind with a handler, and the handlers' names in registration order.
    #[cfg(test)]
    pub(crate) fn census(&self) -> std::collections::BTreeMap<SessionEventKind, Vec<&'static str>> {
        self.by_kind
            .iter()
            .map(|(k, v)| {
                let names: Vec<&'static str> = v
                    .iter()
                    .flat_map(Entry::handlers)
                    .map(|(_, n)| *n)
                    .collect();
                (*k, names)
            })
            .filter(|(_, names)| !names.is_empty())
            .collect()
    }

    /// The slots filled and never reserved: their handlers never run.
    #[cfg(test)]
    pub(crate) fn unreserved(&self) -> Vec<HandlerSlot> {
        self.early
            .iter()
            .filter(|(_, hs)| !hs.is_empty())
            .map(|(s, _)| *s)
            .collect()
    }

    /// Runs every handler once with `ev`, whatever its kind, and names those that could not run:
    /// a wrong-kind event still fetches the system's parameters.
    #[cfg(test)]
    pub(crate) fn probe(&self, world: &mut World, ev: &SessionEvent) -> Vec<String> {
        let mut failed = Vec::new();
        for (kind, list) in &self.by_kind {
            for (id, name) in list.iter().flat_map(Entry::handlers) {
                if let Err(e) = world.run_system_with(*id, ev.clone()) {
                    failed.push(format!("{kind:?}: `{name}` — {e}"));
                }
            }
        }
        failed.sort();
        failed
    }

    fn push(&mut self, kind: SessionEventKind, handler: Handler, ahead: bool) {
        let list = self.by_kind.entry(kind).or_default();
        if ahead {
            list.insert(0, Entry::One(handler));
        } else {
            list.push(Entry::One(handler));
        }
    }

    /// Reserves `slot` at the end of its kind's list, with whatever filled it already.
    fn reserve(&mut self, kind: SessionEventKind, slot: HandlerSlot) {
        assert_eq!(
            slot.kind(),
            kind,
            "{slot:?} is reserved under the wrong kind"
        );
        let list = self.by_kind.entry(kind).or_default();
        assert!(
            !list
                .iter()
                .any(|e| matches!(e, Entry::Slot(s, _) if *s == slot)),
            "{slot:?} is reserved twice"
        );
        let early = self.early.remove(&slot).unwrap_or_default();
        list.push(Entry::Slot(slot, early));
    }

    /// Adds `handler` to `slot`, reserved or not yet.
    fn fill(&mut self, kind: SessionEventKind, slot: HandlerSlot, handler: Handler) {
        assert_eq!(slot.kind(), kind, "{slot:?} is filled under the wrong kind");
        let reserved = self.by_kind.get_mut(&kind).and_then(|list| {
            list.iter_mut().find_map(|e| match e {
                Entry::Slot(s, hs) if *s == slot => Some(hs),
                _ => None,
            })
        });
        match reserved {
            Some(hs) => hs.push(handler),
            None => self.early.entry(slot).or_default().push(handler),
        }
    }

    /// Runs the event's handlers in registration order, each slot's where it was reserved.
    fn run(&self, world: &mut World, ev: SessionEvent) {
        let Some(list) = self.by_kind.get(&SessionEventKind::from(&ev)) else {
            return;
        };
        let mut left: usize = list.iter().map(|e| e.handlers().len()).sum();
        let mut ev = Some(ev);
        for (id, name) in list.iter().flat_map(Entry::handlers) {
            left -= 1;
            // The last handler takes the event; the ones before it a copy each.
            let this = if left == 0 { ev.take() } else { ev.clone() };
            if let Some(this) = this {
                call(world, *id, name, this);
                for hook in &self.after_each {
                    hook(world);
                }
            }
        }
    }
}

fn call(world: &mut World, id: SystemId<In<SessionEvent>>, name: &str, ev: SessionEvent) {
    if let Err(e) = world.run_system_with(id, ev) {
        // A plugin bug (most likely a missing resource), logged loud so the smoke gate sees it.
        error!("net: handler `{name}` did not run: {e}");
    }
}

/// `app.net_handler(SessionEventKind::X, on_x)` registers a system taking `In<SessionEvent>`;
/// several handlers on one kind run in registration order.
pub(crate) trait NetHandlerApp {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self;

    /// [`Self::net_handler`], run ahead of the kind's handlers registered before it: the half of
    /// a reference handler that comes before another subsystem's half, whatever the plugin order.
    fn net_handler_ahead<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self;

    /// Runs `hook` after every handler, its commands applied, before the next: the animation
    /// layer's unit edits a handler's seams queued.
    fn net_after_each_handler(&mut self, hook: fn(&mut World)) -> &mut Self;

    /// Reserves `slot` here, after `kind`'s handlers registered so far: the core's call, beside
    /// its own half of the reference handler.
    fn net_slot(&mut self, kind: SessionEventKind, slot: HandlerSlot) -> &mut Self;

    /// [`Self::net_handler`] into a slot the core reserves: it runs where the slot sits, whatever
    /// the plugin order. `kind` is the slot's own ([`HandlerSlot::kind`]).
    fn net_handler_in<M>(
        &mut self,
        kind: SessionEventKind,
        slot: HandlerSlot,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self;
}

impl NetHandlerApp for App {
    fn net_handler<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self {
        register(self, kind, handler, Place::Last)
    }

    fn net_handler_ahead<M>(
        &mut self,
        kind: SessionEventKind,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self {
        register(self, kind, handler, Place::Ahead)
    }

    fn net_after_each_handler(&mut self, hook: fn(&mut World)) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<NetHandlers>()
            .after_each
            .push(hook);
        self
    }

    fn net_slot(&mut self, kind: SessionEventKind, slot: HandlerSlot) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<NetHandlers>()
            .reserve(kind, slot);
        self
    }

    fn net_handler_in<M>(
        &mut self,
        kind: SessionEventKind,
        slot: HandlerSlot,
        handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    ) -> &mut Self {
        register(self, kind, handler, Place::In(slot))
    }
}

/// Where a registration goes in its kind's list.
enum Place {
    Last,
    Ahead,
    In(HandlerSlot),
}

fn register<M>(
    app: &mut App,
    kind: SessionEventKind,
    handler: impl IntoSystem<In<SessionEvent>, (), M> + 'static,
    place: Place,
) -> &mut App {
    let name = std::any::type_name_of_val(&handler);
    let id = app.world_mut().register_system(handler);
    let mut handlers = app.world_mut().get_resource_or_init::<NetHandlers>();
    match place {
        Place::Last => handlers.push(kind, (id, name), false),
        Place::Ahead => handlers.push(kind, (id, name), true),
        Place::In(slot) => handlers.fill(kind, slot, (id, name)),
    }
    #[cfg(test)]
    handlers.registered.push((id, name));
    app
}

/// Reads every handler's declared access, for [`NetHandlers::accesses`]: each is initialized, its
/// access kept, and registered again, where its first run initializes it again on the same state.
/// Done on a test's asking, not at registration: a handler that lends the call host is most of a
/// plugin's build to initialize, and most tests run few handlers or none.
#[cfg(test)]
pub(crate) fn measure(world: &mut World) {
    world.resource_scope(|world, mut handlers: Mut<NetHandlers>| {
        if handlers.accesses.is_some() {
            return;
        }
        let mut accesses = Vec::new();
        for i in 0..handlers.registered.len() {
            let (id, name) = handlers.registered[i];
            let mut system = world
                .unregister_system(id)
                .expect("a registered handler")
                .system();
            accesses.push((name, system.initialize(world)));
            let again = world.register_boxed_system(system);
            handlers.registered[i].0 = again;
            for list in handlers.by_kind.values_mut() {
                for entry in list
                    .iter_mut()
                    .flat_map(Entry::handlers_mut)
                    .filter(|(at, _)| *at == id)
                {
                    entry.0 = again;
                }
            }
        }
        handlers.accesses = Some(accesses);
    });
}

/// One drain's dispatch in wire order, each handler's commands applied before the next event.
pub(crate) fn dispatch(world: &mut World, events: Vec<SessionEvent>) {
    let handlers = world.remove_resource::<NetHandlers>().unwrap_or_default();
    for ev in events {
        handlers.run(world, ev);
    }
    world.insert_resource(handlers);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What ran, in order.
    #[derive(Resource, Default)]
    struct Log(Vec<String>);

    fn on_queued(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        let SessionEvent::LoginQueued { position, .. } = ev else {
            panic!("the table routes by kind");
        };
        log.0.push(format!("queued:{}", position.unwrap_or(0)));
    }

    fn on_logged_out(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        assert!(matches!(ev, SessionEvent::LoggedOut));
        log.0.push("logged_out".into());
    }

    fn on_logged_out_too(In(_): In<SessionEvent>, mut log: ResMut<Log>) {
        log.0.push("logged_out_too".into());
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Log>();
        app
    }

    fn queued(position: u32) -> SessionEvent {
        SessionEvent::LoginQueued {
            position: Some(position),
            realm: None,
        }
    }

    fn run(app: &mut App, events: Vec<SessionEvent>) -> Vec<String> {
        dispatch(app.world_mut(), events);
        std::mem::take(&mut app.world_mut().resource_mut::<Log>().0)
    }

    #[test]
    fn events_run_in_wire_order_and_an_unhandled_kind_is_skipped() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoginQueued, on_queued)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out);
        let stage = || SessionEvent::LoginStage {
            stage: benilla_protocol::LoginStage::Connecting,
        };
        let log = run(
            &mut app,
            vec![
                queued(1),
                stage(),
                SessionEvent::LoggedOut,
                queued(2),
                stage(),
            ],
        );
        assert_eq!(log, vec!["queued:1", "logged_out", "queued:2"]);
    }

    #[test]
    fn several_handlers_on_one_kind_run_in_registration_order_each_with_the_event() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler(SessionEventKind::LoggedOut, on_logged_out_too);
        let log = run(&mut app, vec![SessionEvent::LoggedOut]);
        assert_eq!(log, vec!["logged_out", "logged_out_too"]);
        assert_eq!(
            app.world().resource::<NetHandlers>().census()[&SessionEventKind::LoggedOut].len(),
            2
        );
    }

    #[test]
    fn a_handler_registered_ahead_runs_before_the_kinds_earlier_ones() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler_ahead(SessionEventKind::LoggedOut, on_logged_out_too);
        let log = run(&mut app, vec![SessionEvent::LoggedOut]);
        assert_eq!(log, vec!["logged_out_too", "logged_out"]);
    }

    fn on_worldport_core(In(ev): In<SessionEvent>, mut log: ResMut<Log>) {
        assert!(matches!(ev, SessionEvent::Worldport { .. }));
        log.0.push("core".into());
    }

    fn on_worldport_half(In(_): In<SessionEvent>, mut log: ResMut<Log>) {
        log.0.push("half".into());
    }

    fn on_worldport_listener(In(_): In<SessionEvent>, mut log: ResMut<Log>) {
        log.0.push("listener".into());
    }

    fn worldport() -> SessionEvent {
        SessionEvent::Worldport {
            map_id: 0,
            position: [0.0; 3],
            orientation: 0.0,
            needs_ack: false,
        }
    }

    /// A slot runs where the core reserved it, whether its feature's plugin was built before the
    /// core's or after it, ahead of the listeners registered after the core.
    #[test]
    fn a_slot_runs_where_it_was_reserved_whenever_it_was_filled() {
        use SessionEventKind as K;
        for fill_first in [true, false] {
            let mut app = app();
            if fill_first {
                app.net_handler_in(
                    K::Worldport,
                    HandlerSlot::WorldportRoster,
                    on_worldport_half,
                );
            }
            app.net_slot(K::Worldport, HandlerSlot::WorldportRoster)
                .net_handler(K::Worldport, on_worldport_core);
            app.net_handler(K::Worldport, on_worldport_listener);
            if !fill_first {
                app.net_handler_in(
                    K::Worldport,
                    HandlerSlot::WorldportRoster,
                    on_worldport_half,
                );
            }
            let log = run(&mut app, vec![worldport()]);
            assert_eq!(
                log,
                vec!["half", "core", "listener"],
                "filled first: {fill_first}"
            );
            let table = app.world().resource::<NetHandlers>();
            assert!(table.unreserved().is_empty());
            assert_eq!(table.census()[&K::Worldport].len(), 3);
        }
    }

    /// An empty slot runs nothing, and the census does not list it.
    #[test]
    fn an_empty_slot_is_skipped() {
        use SessionEventKind as K;
        let mut app = app();
        app.net_slot(K::Worldport, HandlerSlot::WorldportRoster)
            .net_handler(K::Worldport, on_worldport_core);
        assert_eq!(run(&mut app, vec![worldport()]), vec!["core"]);
        assert_eq!(
            app.world().resource::<NetHandlers>().census()[&K::Worldport].len(),
            1
        );
    }

    #[test]
    #[should_panic(expected = "wrong kind")]
    fn a_slot_filled_under_another_kind_is_refused() {
        app().net_handler_in(
            SessionEventKind::LoggedOut,
            HandlerSlot::WorldportRoster,
            on_logged_out,
        );
    }

    /// Every slot a feature filled is one the core reserved; a fill of an unreserved slot would
    /// never run.
    #[test]
    fn every_filled_slot_on_the_built_client_is_reserved() {
        let app = crate::game_plugins::schedule_tests::headless_client_unrun();
        let unreserved = app.world().resource::<NetHandlers>().unreserved();
        assert!(
            unreserved.is_empty(),
            "filled, never reserved: {unreserved:?}"
        );
    }

    /// A kind with no handler is a decoded packet dropped in silence, as the reference does.
    #[test]
    fn every_session_event_kind_has_one_owner() {
        let mut app = crate::game_plugins::schedule_tests::headless_client_unrun();
        let table = app.world_mut().resource::<NetHandlers>().census();
        let unowned: Vec<String> = SessionEventKind::all()
            .filter(|k| !table.contains_key(k))
            .map(|k| format!("{k:?}: decoded and dropped — no handler in the table"))
            .collect();
        eprintln!(
            "session event kinds: {} — {} in the handler table ({} handlers)",
            SessionEventKind::all().count(),
            table.len(),
            table.values().map(Vec::len).sum::<usize>(),
        );
        assert!(unowned.is_empty(), "{}", unowned.join("\n"));
    }

    /// A handler's parameters are fetched only when its packet arrives, which for a rare packet is
    /// never in a smoke run; this fetches them all once, with an event each handler ignores.
    #[test]
    fn every_registered_handler_can_run_on_the_built_client() {
        let mut app = crate::game_plugins::schedule_tests::headless_client_unrun();
        let world = app.world_mut();
        let handlers = world
            .remove_resource::<NetHandlers>()
            .expect("the built client registers handlers");
        let failed = handlers.probe(world, &SessionEvent::LoggedOut);
        let count: usize = handlers.census().values().map(Vec::len).sum();
        eprintln!(
            "net handlers probed on the built client: {count}, {} failed",
            failed.len()
        );
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// A handler registered under the wrong kind compiles and silently ignores its packet, so each
    /// handler must name `SessionEvent::<Kind>` in its body; a session-end listener is exempt.
    #[test]
    fn every_handler_names_the_kind_it_registered_for() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut dirs = vec![src];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).expect("src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let (mut seen, mut problems) = (0, Vec::new());
        for path in files {
            if path.ends_with("net/handlers.rs") {
                continue; // this file's own fixtures and prose
            }
            // A test file's registrations build no client, so the census below never counts them.
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name == "tests.rs"
                || name.ends_with("_tests.rs")
                || path.components().any(|c| c.as_os_str() == "tests")
            {
                continue;
            }
            let text = regex_lite_strip_comments(&std::fs::read_to_string(&path).expect("source"))
                .replace("net_handler_ahead(", "net_handler(");
            // A slot's fill names its kind, then the slot, then the handler.
            let calls = text
                .split("net_handler(")
                .skip(1)
                .map(|c| (c, false))
                .chain(text.split("net_handler_in(").skip(1).map(|c| (c, true)));
            for (call, slotted) in calls {
                let Some((args, _)) = call.split_once(')') else {
                    continue;
                };
                let Some((kind, handler)) = args.split_once(',') else {
                    continue;
                };
                let handler = if slotted {
                    match handler.split_once(',') {
                        Some((_, handler)) => handler,
                        None => continue,
                    }
                } else {
                    handler
                };
                let kind = kind.trim().rsplit("::").next().unwrap_or_default();
                // rustfmt breaks a long call one argument a line, with a trailing comma.
                let handler = handler.trim().trim_end_matches(',').trim_end();
                seen += 1;
                if kind == "Disconnected" {
                    continue;
                }
                let Some(at) = text.find(&format!("fn {handler}(")) else {
                    problems.push(format!(
                        "{}: `{handler}` is not in the file that registers it",
                        path.display()
                    ));
                    continue;
                };
                // The handler's own lines: from its `fn` to the next item's.
                let body: Vec<&str> = text[at..]
                    .lines()
                    .enumerate()
                    .take_while(|(i, l)| {
                        let l = l.trim_start();
                        *i == 0
                            || !(l.starts_with("fn ")
                                || l.starts_with("pub")
                                || l.starts_with("#["))
                    })
                    .map(|(_, l)| l)
                    .collect();
                // The whole variant name: `Chat` is also a prefix of `ChatPlayerNotFound`.
                let names_kind = {
                    let body = body.join("\n");
                    let needle = format!("SessionEvent::{kind}");
                    body.match_indices(&needle).any(|(at, hit)| {
                        !body[at + hit.len()..]
                            .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                    })
                };
                if !names_kind {
                    problems.push(format!(
                        "{}: `{handler}` is registered for {kind} and never names it",
                        path.display()
                    ));
                }
            }
        }
        let registered: usize = crate::game_plugins::schedule_tests::headless_client_unrun()
            .world()
            .resource::<NetHandlers>()
            .census()
            .values()
            .map(Vec::len)
            .sum();
        assert_eq!(
            seen, registered,
            "a registration this scan cannot read — spell it `net_handler(K::Kind, handler)`"
        );
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[derive(Resource)]
    struct Absent;

    fn needs_what_nobody_inserted(In(_): In<SessionEvent>, _absent: Res<Absent>) {}

    #[test]
    fn the_probe_names_a_handler_whose_resource_is_missing() {
        let mut app = app();
        app.net_handler(SessionEventKind::LoggedOut, on_logged_out)
            .net_handler(SessionEventKind::LoginQueued, needs_what_nobody_inserted);
        let handlers = app.world_mut().remove_resource::<NetHandlers>().unwrap();
        let failed = handlers.probe(app.world_mut(), &SessionEvent::LoggedOut);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(
            failed[0].contains("needs_what_nobody_inserted"),
            "{failed:?}"
        );
    }

    /// `//` comments out, so a variant named in prose does not count as an arm.
    fn regex_lite_strip_comments(s: &str) -> String {
        s.lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
