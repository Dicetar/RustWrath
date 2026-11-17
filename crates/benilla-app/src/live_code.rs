//! Live code: the whole client's native code patched into the running process (ADR-0002, "Live
//! code").
//!
//! A build with the `live_code` feature, started by `dx serve --hot-patch`, takes each saved edit
//! as a patch: dx rebuilds the edited workspace crates and the crates above them, and Bevy's
//! `HotPatchPlugin` (in `DefaultPlugins` under `bevy/hotpatching`) installs the new code's jump
//! table. A patch reaches new code only where a call goes through the table:
//!
//! - every Bevy system and observer, each run;
//! - the network threads' read and encode ([`call`] in `net::io`), each packet and each command;
//! - whatever those call, since a patched function calls the patched versions of its callees.
//!
//! What a patch cannot reach needs a restart, and the launcher says so: a changed struct or enum
//! layout, a changed static or thread-local initializer, a new or reordered system, set or plugin
//! (the schedule is built once), a new dependency, and a Bevy upgrade.
//!
//! Data derived before the patch is still the old code's output. Systems in [`Rederive`] run once
//! on the frame after a patch lands and rebuild it with the new code: the saved template stores
//! decode their kept answer bytes again (`crate::wdb`), and the spell catalog reloads
//! (`crate::ui_action`). A deriver that a decoder fix can change joins the set.
//!
//! Without the feature nothing here patches: [`call`] calls its closure, nothing sends
//! [`LiveCodePatched`], and [`Rederive`] never runs.

use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;

/// Sent in `First` on the frame after a patch lands, to the [`Rederive`] set and any other reader.
#[derive(Message, Default, Clone, Copy, Debug)]
// Only `announce_patch` sends it, and that is compiled with `live_code` alone.
#[cfg_attr(not(feature = "live_code"), allow(dead_code))]
pub(crate) struct LiveCodePatched;

/// The derivers re-run after a patch, in `First`, after the patch is announced.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Rederive;

/// Calls `f` through the live-code jump table, so a patch landed since the last call runs the new
/// code. A long-lived loop outside Bevy's systems wraps each pass's work in it; the loop's own body
/// stays the code it entered. Only debug-assertion builds patch (the `dev` profile, not `play`).
///
/// `f` may run again when a patch unwinds it mid-call, so it must not move out of its captures.
#[cfg(feature = "live_code")]
pub(crate) fn call<O>(f: impl FnMut() -> O) -> O {
    bevy::app::hotpatch::call(f)
}

/// Without `live_code`, a direct call.
#[cfg(not(feature = "live_code"))]
pub(crate) fn call<O>(mut f: impl FnMut() -> O) -> O {
    f()
}

/// Adds `systems` to [`Rederive`]. The set is gated here as well as in [`LiveCodePlugin`], so an
/// app built without the plugin (a test's) never runs a deriver every frame.
pub(crate) fn rederive<M>(app: &mut App, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
    gate(app);
    app.add_systems(First, systems.in_set(Rederive));
}

/// The patch message and the set's gate, once per app.
fn gate(app: &mut App) {
    if app.world().contains_resource::<Messages<LiveCodePatched>>() {
        return;
    }
    app.add_message::<LiveCodePatched>()
        .configure_sets(First, Rederive.run_if(on_message::<LiveCodePatched>));
}

pub(crate) struct LiveCodePlugin;

impl Plugin for LiveCodePlugin {
    fn build(&self, app: &mut App) {
        gate(app);
        #[cfg(feature = "live_code")]
        app.add_systems(First, announce_patch.before(Rederive));
    }
}

/// Bevy's `HotPatched`, which its plugin sends in `Last`, as [`LiveCodePatched`] at the head of
/// the next frame, with a log line naming the patch.
#[cfg(feature = "live_code")]
fn announce_patch(
    mut patched: MessageReader<bevy::ecs::HotPatched>,
    mut out: MessageWriter<LiveCodePatched>,
    mut count: Local<u32>,
) {
    if patched.read().count() == 0 {
        return;
    }
    *count += 1;
    info!(
        "live code: patch {} landed; re-deriving templates and the spell catalog",
        *count
    );
    out.write(LiveCodePatched);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Runs(u32);

    fn count(mut runs: ResMut<Runs>) {
        runs.0 += 1;
    }

    /// A deriver in [`Rederive`] runs on the frame a patch is announced and on no other.
    #[test]
    fn rederive_runs_only_on_a_patched_frame() {
        let mut app = App::new();
        app.init_resource::<Runs>();
        rederive(&mut app, count);
        app.add_plugins(LiveCodePlugin);
        app.update();
        assert_eq!(app.world().resource::<Runs>().0, 0, "no patch, no run");
        app.world_mut().write_message(LiveCodePatched);
        app.update();
        assert_eq!(app.world().resource::<Runs>().0, 1, "the patched frame");
        app.update();
        assert_eq!(app.world().resource::<Runs>().0, 1, "once per patch");
    }
}
