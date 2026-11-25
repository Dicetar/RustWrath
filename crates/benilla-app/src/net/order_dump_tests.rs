//! Scratch (not committed): dumps the packet-handler table and every schedule's executor order of
//! the headless client to `$ORDER_DUMP`, to diff before/after a move.

use bevy::prelude::*;

#[test]
fn dump_handler_and_system_order() {
    let Ok(out) = std::env::var("ORDER_DUMP") else {
        return;
    };
    let mut text = String::new();
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    {
        let table = app
            .world()
            .resource::<crate::net::handlers::NetHandlers>()
            .census();
        for (kind, names) in table {
            text.push_str(&format!("HANDLER {kind:?}\n"));
            for n in names {
                text.push_str(&format!("  {n}\n"));
            }
        }
    }
    let mut labels: Vec<_> = app
        .world()
        .resource::<Schedules>()
        .iter()
        .map(|(_, schedule)| {
            let label = schedule.label();
            (format!("{label:?}"), label)
        })
        .collect();
    labels.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, label) in labels {
        app.world_mut().schedule_scope(label, |world, schedule| {
            if schedule.initialize(world).is_err() {
                text.push_str(&format!("SCHEDULE {name} (did not build)\n"));
                return;
            }
            text.push_str(&format!("SCHEDULE {name}\n"));
            for (_, system) in schedule.systems().expect("initialized") {
                text.push_str(&format!("  {}\n", system.name()));
            }
        });
    }
    std::fs::write(out, text).expect("write dump");
}
