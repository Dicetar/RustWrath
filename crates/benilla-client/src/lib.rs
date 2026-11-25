//! `benilla-client`: the client assembled. [`run`] and [`run_with`] boot it: the install check, the
//! window, the engine (`benilla_world`), the game's plugin group (`benilla_app::game_plugins`) and
//! the dev instruments around them, in the order `launch` adds them.
//!
//! The game itself lives in `benilla-app`, the core, and in the crates that sit on top of it. This
//! crate is kept thin on purpose: an edit in a crate above the core rebuilds that crate and this
//! one, not the core's ~300k lines.

// A player build's dead code is the seam working (see `benilla-app`'s root).
#![cfg_attr(not(feature = "dev"), allow(dead_code))]

use bevy::prelude::*;

// The core's modules `launch` reads, imported at the root so the boot's `crate::…` and bare
// module paths read as they did inside `benilla-app`.
#[cfg(feature = "hotpatch")]
use benilla_app::hotpatch;
use benilla_app::{crash, game_plugins, local_state, no_install, perf, run_mode, shutdown, video};
// The instruments, a crate of their own on top of the core.
use benilla_dev as dev;

/// The Wrath session's config, under the path the boot names it by.
mod wrath {
    pub(crate) use benilla_app::WrathSessionConfig;
}

// The launcher's and a crate on top's whole surface, re-exported so neither needs the core by
// name: the build stamp and exit status, and the objects and interaction a crate on top reads.
pub use benilla_app::{
    config_dir, run_worldview, AppExit, BuildId, Interact, ObjectFields, ObjectType, Objects,
};

/// Builds and runs the client app. `build` is the launcher's compile-time git stamp, passed in as
/// data so the sha lives in the shim's fingerprint and a commit does not recompile this crate.
pub fn run(build: BuildId) -> AppExit {
    launch(build, None)
}

/// [`run`], plus `extend` adding a crate's own plugins: how a feature 1.12.1 lacks is built on top
/// of benilla (`examples/extended_launcher.rs`), the build marked `extended` wherever it is named.
/// `extend` gets the client fully built and runs before the probe fleet, which sees what it adds.
pub fn run_with(build: BuildId, extend: impl FnOnce(&mut App)) -> AppExit {
    launch(build, Some(Box::new(extend)))
}

/// What a crate on top of benilla adds to the built app ([`run_with`]).
type Extension<'a> = Box<dyn FnOnce(&mut App) + 'a>;

/// The one body behind [`run`] and [`run_with`], not generic, so a launcher compiles none of it.
fn launch(build: BuildId, extend: Option<Extension<'_>>) -> AppExit {
    // The capture harness's boot answers (`run_mode::capture_ui_opted_in`, `start_state`), before
    // anything below reads them.
    dev::install_boot_hooks();
    // `WOW_HOVER_LOG_REPORT=<csv>` and `WOW_CAPTURE=list` print and exit before any setup.
    if let Ok(path) = std::env::var("WOW_HOVER_LOG_REPORT") {
        dev::report_recorded_hover_log(&path);
        return AppExit::Success;
    }
    if std::env::var("WOW_CAPTURE").as_deref() == Ok("list") {
        dev::print_scenario_names();
        return AppExit::Success;
    }
    // `WOW_PROBE=list` likewise prints the probe environment registry.
    if std::env::var("WOW_PROBE").as_deref() == Ok("list") {
        dev::print_probe_vars();
        return AppExit::Success;
    }

    // Every line that names the build says whether a crate on top extended it.
    let build = BuildId {
        extended: extend.is_some(),
        ..build
    };

    // A dev build's install, state folder and probe identity resolve from the launcher's folder,
    // so a crate on top of benilla keeps them in its own and not in cargo's checkout of benilla.
    benilla_formats::set_project_folder(build.project_dir);
    if run_mode::unattended() && std::env::var_os("BENILLA_HOME").is_none() {
        // Before the logger exists; whoever reads what the run wrote needs the folder.
        if let Some(home) = local_state::home() {
            println!(
                "state: an unattended run keeps its own state in {} (BENILLA_HOME pins one)",
                home.display()
            );
        }
    }

    // From here a panic leaves `benilla-config/Diagnostics/crash-<unix>.txt`; armed before the
    // `App` exists, so a panic while plugins build is reported too.
    crash::install(build);

    // Fonts, art and the stock interface all come from the install, so without one the client
    // ends here, before any window or plugin exists (`no_install`).
    let data_dir = match no_install::resolve() {
        Ok(data_dir) => data_dir,
        Err(err) => return no_install::fail(&err, run_mode::unattended()),
    };

    let mut app = App::new();
    // The panel footer and the build banner read the stamp back.
    app.insert_resource(build);
    // Static-scene transform tracking pinned on: the default threshold re-decides every frame with
    // two full scans of the rows the tracking exists to skip, and this scene is static-heavy. It
    // costs more only when most rows move in one frame (a load burst into a near-empty world).
    app.insert_resource(bevy::transform::systems::StaticTransformOptimizations::enabled());
    // Update runs single-threaded: most per-frame Update systems are non-Send (mlua's `UiScript`,
    // kira's audio handles) and serialize through the multi-threaded executor anyway, so its
    // dispatch is pure overhead (measured faster, tails flat). `WOW_MT_UPDATE=1` restores it.
    if std::env::var_os("WOW_MT_UPDATE").is_none() {
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
    }
    // PostUpdate too: 208 systems paid about 10 µs each of multi-threaded dispatch, and
    // single-threaded measured faster parked and in motion. `WOW_MT_POSTUPDATE=1` restores it; the
    // log line names the config a run measured.
    if std::env::var_os("WOW_MT_POSTUPDATE").is_none() {
        app.edit_schedule(PostUpdate, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        println!(
            "executor: PostUpdate -> single-threaded (the default; WOW_MT_POSTUPDATE=1 for MT)"
        );
    }
    // The build banner ships in every build: which build produced a log is the first thing a report
    // from another machine must establish.
    app.add_systems(Startup, benilla_world::build_id::banner);

    // With `$WOW_CAPTURE` set the app runs a deterministic, server-less capture (net off, so no
    // NPCs stream in) and exits.
    let capturing = run_mode::scenario_active();
    // The 3.3.5a client is the client: the boot selects the Wrath session unless the donor
    // fixture is forced. `WOW_DONOR_112=1` is the only way back to the 1.12 path.
    if std::env::var_os("WOW_DONOR_112").is_none() {
        std::env::set_var("WOW_WRATH", "1");
    }
    // The Wrath session is mutually exclusive with the stock NetPlugin's connection, so one
    // network/session owner exists.
    let wrath_config = match crate::wrath::WrathSessionConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("wrath: FATAL - {error:#}");
            return AppExit::error();
        }
    };
    // Every instrumented run, captures and live probes, opens in the background so it never takes
    // over a person's screen; `WOW_BG` overrides.
    let background = benilla_world::bgwin::background_run();
    if capturing {
        // Ground clutter scatters with per-run randomness, so captures turn it off for byte-stable
        // baselines; set before plugins build so `ClutterConfig::from_env` reads it. The frame
        // clock, frozen in `capture`, is the other source of drift.
        std::env::set_var("WOW_CLUTTER_DENSITY", "0");
        // Anim-LOD park/wake (`creature_anim::lod::gate_rig_animation`) hangs on asset-load timing,
        // which the frozen clock does not control, so captures turn it off unless overridden. A rig
        // in frame should never be parked, so the shot keeps its subject.
        if std::env::var("WOW_NO_ANIM_LOD").is_err() {
            std::env::set_var("WOW_NO_ANIM_LOD", "1");
        }
    }

    // The `mpq://` source must be registered before `AssetPlugin` (in `DefaultPlugins`) builds. The
    // install is found by `benilla_formats::wow_data`, as everywhere, and is required above; one
    // whose archives will not open ends the boot the same way (`no_install`).
    //
    // Profile selection (route-asset handoff C1): `WM_ASSET_MANIFEST` selects a manifest-defined
    // profile (`WM_ASSET_PROFILE` names it; default `base`); without the variable the stock
    // discovery is used. One registration, never both.
    let source_result = match std::env::var_os("WM_ASSET_MANIFEST").map(std::path::PathBuf::from) {
        Some(manifest_path) => {
            let profile_name =
                std::env::var("WM_ASSET_PROFILE").unwrap_or_else(|_| "base".to_owned());
            benilla_assets::register_profile_source(
                &mut app,
                &data_dir,
                &manifest_path,
                &profile_name,
            )
        }
        None => benilla_assets::register_mpq_source(&mut app, &data_dir),
    };
    if let Err(e) = source_result {
        return no_install::fail(
            &no_install::NoInstall::unreadable(&data_dir, &e),
            run_mode::unattended(),
        );
    }

    // No `game://` source: the five UI shaders are compiled in by `crate::shaders`
    // (`embedded://benilla_app/shaders/…`), so no build-machine path reaches the binary.

    app.add_plugins(benilla_world::boot::tuned_default_plugins(Window {
        title: "benilla".into(),
        // Born in the player's display mode (`gxWindow` read straight off `config.toml`) rather
        // than flipped into it at `Startup`, which would flash on every launch and, under
        // gamescope, spend the first second in the input state fullscreen is meant to end.
        // Instrumented runs stay windowed (`video::windowed_env`).
        mode: video::boot_window_mode(),
        // UI-fixture captures size the window to what they photograph: the action bar's 1024 px
        // plus 128 px end caps get a wide, short window, and vplates pins 1024×768, where one gx
        // unit is 1280 px and the plate lands at the border texture's native 128×32, diffable
        // against the BLP.
        resolution: video::at_requested_dpi(
            // Same opt-in as the UI load (`ui_script::lifecycle::ui_wanted`); the two must agree,
            // or the right content is captured at the wrong size.
            if capturing && crate::run_mode::capture_ui_opted_in() {
                // `$WOW_WIN` overrides here too, for scale-dependent UI bugs.
                if let Some(win) = video::requested_window_size() {
                    win
                } else {
                    match std::env::var("WOW_CAPTURE").as_deref() {
                        Ok("ui-actionbar") => UVec2::new(1300, 260),
                        Ok("vplates") => UVec2::new(1024, 768),
                        // Short enough for the action bar to overlap the chat edit box.
                        Ok("ui-chatedit") => UVec2::new(566, 377),
                        // The map's chrome is a centered 1024×768 block, with margin.
                        Ok("ui-worldmap") => UVec2::new(1100, 800),
                        // Margin keeps the 920×724 window's edge tile and close X in frame.
                        Ok("ui-options") | Ok("ui-options-audio") | Ok("ui-options-graphics") => {
                            UVec2::new(1200, 900)
                        }
                        _ => UVec2::new(640, 700),
                    }
                }
                .into()
            } else {
                // `$WOW_WIN=WxH` (logical px) overrides the size. FFXGlow's blur is pinned in
                // texels, so thin bright features self-amplify at high resolution; matching the
                // reference's pixel density (`WOW_WIN=512x288` on a 2× display is 1024×576)
                // isolates that term.
                video::requested_window_size()
                    // A run that reads no pixels gets a small window: it is held `AlwaysOnTop`
                    // against the occlusion throttle (`capture::ProbeFocusPlugin`), and small and
                    // cornered keeps it out of the way. Pixel runs keep the full size.
                    .unwrap_or(if benilla_world::bgwin::no_pixel_run() {
                        UVec2::new(640, 360)
                    } else {
                        // The player's `gxResolution`; `bevy_winit` ignores it while fullscreen.
                        video::boot_windowed_size()
                    })
                    .into()
            },
        ),
        // `$WOW_NOVSYNC=1` uncaps presentation from boot so a headless FPS-journal run measures
        // frame cost, not the vsync ceiling; otherwise the player's `gxVSync` takes over from
        // `Startup` ([`crate::video`]).
        present_mode: video::present_mode(!video::novsync_env()),
        // A background run opens unfocused, so it cannot take keystrokes meant for another app, and
        // is born `AlwaysOnBottom` (`kCGNormalWindowLevel - 1`), so winit's two raises before the
        // first frame cannot flash it on top. `BgWinPlugin` promotes it to Normal once the launch
        // settles, so it can still be raised.
        focused: !background,
        window_level: if background {
            bevy::window::WindowLevel::AlwaysOnBottom
        } else {
            bevy::window::WindowLevel::Normal
        },
        ..default()
    }))
    .add_plugins(benilla_world::thread_qos::ThreadQosPlugin)
    // Undoes winit's forced macOS app activation for background runs; the window-side half is the
    // `Window` above.
    .add_plugins(benilla_world::bgwin::BgWinPlugin)
    // macOS `Cmd+Q` goes straight to `terminate:`, which never runs another frame, so the session
    // was never written; re-pointed at the window close, which [`shutdown`] sees.
    .add_plugins(benilla_world::mac_quit::MacQuitPlugin)
    // Alt+F4 on Windows and Linux, where a tiling window manager binds nothing to it.
    .add_plugins(shutdown::AltF4Plugin)
    // The engine as one group; `world_plugins.rs` has its load-bearing ordering edges and what it
    // leaves out (`pipe_warm`).
    .add_plugins(benilla_world::world_plugins::WorldPlugins)
    // The instruments, which the engine group does not carry; `--no-default-features` compiles them
    // all out (see `dev.rs`).
    .add_plugins(dev::DevToolsPlugin)
    // The FPS journal ships: `/console fpsJournal 1` appends a per-second row of position, frame
    // cost and the GPU's per-pass split to `benilla-config/Diagnostics/fps-journal.csv`;
    // `WOW_FPS_JOURNAL=<csv>` is the harness lever.
    .add_plugins(perf::FpsJournalPlugin)
    // The game as one group on top of the engine; `game_plugins.rs` has its members, its
    // load-bearing ordering edges and the test that builds it headless.
    .add_plugins(
        game_plugins::GamePlugins {
            connect: !capturing && wrath_config.is_none(),
            start: run_mode::start_state(),
            wrath: wrath_config,
        }
        .build()
        // Souls combat on the playback, at its place in the group: right after the state it
        // writes, which the core reads.
        .add_after::<benilla_app::souls::SoulsStatePlugin>(benilla_souls::SoulsPlugin),
    );

    // benilla-assets' loaders go into the live `AssetServer`, so they register after `AssetPlugin`.
    benilla_assets::register_asset_loaders(&mut app);

    // `ExtractSchedule` runs single-threaded too: bevy_render leaves it multi-threaded, with no
    // non-Send member, and single-threaded measured faster. `WOW_MT_EXTRACT=1` restores it.
    //
    // `Render` stays multi-threaded: under pipelined rendering that executor is also bevy's route
    // for non-Send systems to the main thread, and `create_surfaces` needs it because macOS makes a
    // Metal layer only on the UI thread (the single-threaded executor panics at startup).
    //
    // Set here because the render sub-app exists only once the plugin chain has built.
    if std::env::var_os("WOW_MT_EXTRACT").is_none() {
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app.edit_schedule(bevy::render::ExtractSchedule, |s| {
                s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
            });
            println!(
                "executor: ExtractSchedule -> single-threaded (the default; WOW_MT_EXTRACT=1 for MT)"
            );
        } else {
            // A missing sub-app would silently flip nothing.
            eprintln!("executor: no render app — ExtractSchedule flip NOT applied");
        }
    }

    // A crate on top of benilla adds its plugins here ([`run_with`]).
    if let Some(extend) = extend {
        extend(&mut app);
    }

    // The probe fleet, last so it observes the fully-built app; compiled out by
    // `--no-default-features`.
    app.add_plugins(dev::DevProbesPlugin);
    #[cfg(feature = "hotpatch")]
    app.add_plugins(hotpatch::plugin);

    // Returns the app's exit status: a failed capture sets `AppExit::error()`
    // (`capture::drive_capture`), which must not exit 0 with no PNG on disk.
    app.run()
}
