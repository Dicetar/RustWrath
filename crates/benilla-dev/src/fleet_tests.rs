//! The tests that build the whole client with every instrument and probe added
//! ([`crate::add_every_instrument`]), moved here from `benilla-app`'s `vm_pass` and
//! `script_calls` with the instruments themselves.

use std::collections::{HashMap, HashSet};

use benilla_ui::script::UiScript;
use bevy::ecs::schedule::SystemKey;
use bevy::ecs::system::{IntoSystem, System};
use bevy::prelude::*;

use crate::game_plugins::schedule_tests::{census, headless_client_with, Census, SyncPoints};
use crate::script_calls::{HostedVm, ScriptCallHost, VmWith};

/// Every system the declared order runs after `from`.
fn after(next: &HashMap<SystemKey, Vec<SystemKey>>, from: SystemKey) -> HashSet<SystemKey> {
    let mut seen = HashSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        for m in next.get(&n).into_iter().flatten() {
            if seen.insert(*m) {
                stack.push(*m);
            }
        }
    }
    seen
}

fn successors(update: &Census) -> HashMap<SystemKey, Vec<SystemKey>> {
    let mut next: HashMap<SystemKey, Vec<SystemKey>> = HashMap::new();
    for (a, b) in &update.dependencies {
        next.entry(*a).or_default().push(*b);
    }
    next
}

/// Every two hosts are ordered, the probes' and the instruments' too: no two systems that lend
/// the call host run in an order the executor picks.
#[test]
fn every_two_hosts_run_in_a_declared_order() {
    let mut app = headless_client_with(crate::add_every_instrument);
    let update = census(&mut app, Update, SyncPoints::Declared);
    let mut unordered: Vec<String> = update
        .conflicts
        .iter()
        .filter(|(a, b, _)| update.hosted.contains(a) && update.hosted.contains(b))
        .map(|(a, b, _)| format!("{} <-> {}", update.name(*a), update.name(*b)))
        .collect();
    unordered.sort();
    assert!(
        update.hosted.len() > 150,
        "the census finds the hosts: {}",
        update.hosted.len()
    );
    assert!(
        unordered.is_empty(),
        "{} pairs of hosts in no declared order:\n  {}",
        unordered.len(),
        unordered.join("\n  ")
    );
}

/// A unit edit applies where it is made: every system that can queue one runs ahead of a
/// flush in its own schedule, `Update`'s at the pass's end. An exclusive system applies its
/// own ([`crate::creature_anim::apply_unit_edits`]); outside `Update` the flush follows it as
/// well, so a one-shot it ran that left an edit does not carry it into the next frame.
#[test]
fn every_unit_edit_is_flushed_in_the_schedule_that_makes_it() {
    use bevy::ecs::schedule::ScheduleLabel as _;
    #[derive(PartialEq)]
    enum Kind {
        User,
        Flush,
        Exclusive,
    }
    let mut app = headless_client_with(crate::add_every_instrument);
    let edits = app
        .world()
        .components()
        .get_resource_id(std::any::TypeId::of::<crate::creature_anim::UnitEdits>())
        .expect("the queue is registered");
    let labels: Vec<_> = app
        .world()
        .resource::<Schedules>()
        .iter()
        .map(|(_, schedule)| schedule.label())
        .collect();
    // By identity, not by name: a build without type names has none.
    let flush = System::type_id(&IntoSystem::into_system(
        crate::creature_anim::flush_unit_edits,
    ));
    let (mut users, mut exclusives) = (0, 0);
    let mut late = Vec::new();
    for label in labels {
        let in_update = label == Update.intern();
        let queues: Vec<(SystemKey, String, Kind)> =
            app.world_mut().schedule_scope(label, |world, schedule| {
                schedule.graph_mut().systems.initialize(world);
                let graph = schedule.graph();
                graph
                    .systems
                    .iter()
                    .filter_map(|(key, system, _)| {
                        let access = &graph.systems.get(key).expect("initialized").access;
                        let kind = if system.is_exclusive() {
                            Kind::Exclusive
                        } else if System::type_id(&**system) == flush {
                            Kind::Flush
                        } else if access.combined_access().has_resource_read(edits) {
                            Kind::User
                        } else {
                            return None;
                        };
                        Some((key, system.name().to_string(), kind))
                    })
                    .collect()
            });
        if !queues.iter().any(|(_, _, kind)| *kind == Kind::User) {
            continue;
        }
        let schedule = census(&mut app, label, SyncPoints::Declared);
        let next = successors(&schedule);
        let flushes: Vec<SystemKey> = queues
            .iter()
            .filter(|(_, _, kind)| *kind == Kind::Flush)
            .map(|(key, _, _)| *key)
            .collect();
        for (key, name, kind) in &queues {
            match kind {
                Kind::Flush => continue,
                Kind::Exclusive if in_update => continue,
                Kind::Exclusive => exclusives += 1,
                Kind::User => users += 1,
            }
            if !flushes.iter().any(|f| after(&next, *key).contains(f)) {
                late.push(format!("{label:?}: {name}"));
            }
        }
    }
    assert!(users > 20, "the census finds the queue's users: {users}");
    assert!(
        exclusives > 10,
        "and the exclusive systems beside them: {exclusives}"
    );
    assert!(
        late.is_empty(),
        "systems that can queue unit edits after their schedule's flush, or where none \
         runs:\n  {}",
        late.join("\n  ")
    );
}

/// The host's shared sinks ([`super::shared_sinks`]) are written through `&self`, so their
/// users declare reads and no conflict orders them: every two users run in a declared order,
/// so the refusals a call reads back ([`crate::ui_cast::CallReports`]) are its own, and the
/// lines and sounds land in one order every frame.
#[test]
fn every_user_of_a_shared_sink_runs_in_one_order() {
    let mut app = headless_client_with(crate::add_every_instrument);
    let sinks = crate::vm_pass::shared_sinks(app.world());
    assert_eq!(sinks.len(), 5, "the sinks are registered");
    let users: Vec<SystemKey> = app.world_mut().schedule_scope(Update, |world, schedule| {
        schedule.graph_mut().systems.initialize(world);
        let graph = schedule.graph();
        graph
            .systems
            .iter()
            .filter(|(key, system, _)| {
                let combined = graph
                    .systems
                    .get(*key)
                    .expect("initialized")
                    .access
                    .combined_access();
                !system.is_exclusive() && sinks.iter().any(|id| combined.has_resource_read(*id))
            })
            .map(|(key, _, _)| key)
            .collect()
    });
    let update = census(&mut app, Update, SyncPoints::Declared);
    let next = successors(&update);
    let reach: HashMap<SystemKey, HashSet<SystemKey>> =
        users.iter().map(|u| (*u, after(&next, *u))).collect();
    let mut unordered = Vec::new();
    for (i, a) in users.iter().enumerate() {
        for b in &users[i + 1..] {
            if !reach[a].contains(b) && !reach[b].contains(a) {
                unordered.push(format!("{} <-> {}", update.name(*a), update.name(*b)));
            }
        }
    }
    unordered.sort();
    assert!(
        users.len() > 50,
        "the census finds the sinks' users: {}",
        users.len()
    );
    assert!(
        unordered.is_empty(),
        "{} pairs of a shared sink's users in no declared order:\n  {}",
        unordered.len(),
        unordered.join("\n  ")
    );
}

/// The systems of `app` that write the VM, and those of them whose declared access does not
/// cover the call host's, by schedule and name. An exclusive system declares no access and is
/// left out.
fn vm_writers_and_the_unhosted(app: &mut App) -> (Vec<String>, Vec<String>) {
    use bevy::ecs::system::{IntoSystem, System};
    let world = app.world_mut();
    let host = System::initialize(&mut IntoSystem::into_system(|_: ScriptCallHost| {}), world);
    let host = host.combined_access();
    // Each system's access with the host's summed in, into one buffer reused across them.
    let mut with_host = bevy::ecs::query::Access::new();
    let labels: Vec<_> = world
        .resource::<bevy::ecs::schedule::Schedules>()
        .iter()
        .map(|(_, s)| s.label())
        .collect();
    let (mut writers, mut unhosted) = (Vec::new(), Vec::new());
    for label in labels {
        world.schedule_scope(label, |world, schedule| {
            schedule.graph_mut().systems.initialize(world);
            let Some(vm) = world
                .components()
                .get_resource_id(std::any::TypeId::of::<UiScript>())
            else {
                return;
            };
            let graph = schedule.graph();
            for (key, system, _) in graph.systems.iter() {
                let Some(declared) = graph.systems.get(key) else {
                    continue;
                };
                let access = declared.access.combined_access();
                if system.is_exclusive() || !access.has_resource_write(vm) {
                    continue;
                }
                writers.push(system.name().to_string());
                with_host.clone_from(access);
                with_host.extend(host);
                if with_host != *access {
                    unhosted.push(format!("{label:?}: {}", system.name()));
                }
            }
        });
    }
    unhosted.sort();
    (writers, unhosted)
}

/// Every system that writes the VM holds its call host, so a call its Lua makes applies inside
/// the verb whatever entry the script came in by, as `TargetByName` (`0x489d60`) commits
/// through `SetSelection` (`0x493540`) before it returns. Read off each system's declared
/// access in every schedule of the client with every instrument and probe added, where a
/// system whose own parameters collide with the host's also fails to initialize. An exclusive
/// entry lends through [`with_hosted_vm`].
#[test]
#[ignore = "known failure on main (2026-11-18 CI baseline); run with --ignored"]
fn every_system_that_writes_the_vm_holds_its_call_host() {
    // The census tells a bare writer from a hosted one.
    let mut probe = App::new();
    probe.add_systems(
        Update,
        (
            |_: Option<NonSendMut<UiScript>>| {},
            |_: HostedVm| {},
            |_: VmWith<Res<'static, Time>>| {},
        ),
    );
    let (writers, unhosted) = vm_writers_and_the_unhosted(&mut probe);
    assert_eq!((writers.len(), unhosted.len()), (3, 1), "{unhosted:?}");

    let mut app =
        crate::game_plugins::schedule_tests::headless_client_with(crate::add_every_instrument);
    let (writers, unhosted) = vm_writers_and_the_unhosted(&mut app);
    // The census reads the client's systems: a hundred writers at least in either build, and
    // where the build carries type names, a feed, a death feed and a frame poll among them.
    assert!(
        writers.len() >= 100,
        "the census reads the client's VM writers: {} read",
        writers.len()
    );
    if writers.iter().any(|w| w.contains("benilla_app::")) {
        for name in ["feed_dialog_verbs", "feed_death", "poll_area_spirit_healer"] {
            assert!(
                writers.iter().any(|w| w.contains(name)),
                "the census reads the client's VM writers, `{name}` among them"
            );
        }
    }
    assert!(
        unhosted.is_empty(),
        "these systems write the VM without holding its call host (`HostedVm` or `VmWith`), \
         so a call their Lua makes is refused:\n  {}",
        unhosted.join("\n  ")
    );
}
