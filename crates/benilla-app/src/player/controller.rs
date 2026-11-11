//! The per-frame player controller, [`control`]: this frame's input to the avatar's pose, its
//! animation state and the packets owed the server. The phases live in sibling modules; their order
//! here is the reference's frame order and is load-bearing (the swim latch before the mover, the
//! posture commit before the pose, the ack after the launch).

use super::*;
// Named explicitly so the derive works from this module's own scope, not only through the parent
// module's prelude glob.
use bevy::prelude::Resource;

#[cfg(test)]
mod tests;

/// Free-flies until the server places us, then drives the avatar and streams its movement as the
/// mover; the dev chord with `F` toggles free fly.
#[allow(clippy::type_complexity)]
pub(super) fn control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    // Tupled parameters keep this system under Bevy's limit of 16.
    pointer: (
        Res<AccumulatedMouseMotion>,
        Res<camera::LookConfig>,
        Res<camera_zoom::ZoomLimit>,
        Res<camera::FollowConfig>,
        Res<camera_dynamics::CameraOptions>,
        // Only `nearclip` is read here, for the self-avatar fade.
        Res<benilla_world::view::ViewDistance>,
        // Whether the live view is customised, the auto-return's view gate (`0x50fc30`).
        Option<Res<camera_view::CameraViews>>,
    ),
    mut net: (
        Res<NetCommands>,
        MessageReader<TeleportMessage>,
        MessageReader<WorldportMessage>,
        MessageWriter<crate::creature_anim::SheathRequest>,
        MessageReader<crate::net::SpeedChangeMessage>,
        // Root, water walk, feather fall and hover on our mover, acked with the live pose.
        MessageReader<crate::net::MoveModeMessage>,
        // The hard-landing report (`0x602d00`: the wound vocal and the dust).
        MessageWriter<crate::creature_anim::HardLanding>,
        // The move edges the reference hands `AbortCast` (`0x6e4940`), for the cast bar.
        ResMut<crate::spell::LocalMoveStart>,
        // The mounted Space flourish plays locally at send; the net drain drops its echo.
        MessageWriter<crate::creature_anim::MountFlourish>,
        // A `MSG_MOVE_*` for our own mover: a pose the server wrote, no ack owed.
        MessageReader<crate::net::SelfMoveMessage>,
        MessageReader<StandStateRequest>,
        // Control of a unit granted or revoked: the mover claim and parting pose are ours to give.
        MessageReader<crate::net::ClientControlMessage>,
        // A knockback on our mover: latched by `wire_in`, flown below, acked after the launch.
        MessageReader<crate::net::KnockBackMessage>,
        crate::tutorial::InputHooks,
        // The loot window's move-start close (`0x60e990`).
        ResMut<crate::ui_loot::LootMoveStart>,
        // `SMSG_STANDSTATE_UPDATE`, applied locally and ungated, with nothing sent back.
        MessageReader<super::ServerStandState>,
    ),
    mut speed_capsule: (
        Res<MoveSpeed>,
        Res<PlayerCapsule>,
        Res<InspectMode>,
        Res<crate::ui_script::UiKeyboardCapture>,
        Res<crate::ui_script::PlayerUiClickConsumed>,
        // Every rebindable input reads here; raw `keys` serve only free fly and its chord.
        Res<crate::bindings::BindingsState>,
        // The far-sight subject, resolved earlier since this system holds our `Transform` mutably.
        Res<view_subject::ViewSubject>,
        // Tells a revoked body from a granted creature in the control handoff.
        Res<crate::net::SelfGuid>,
        Res<scoped_view::ScopedView>,
        // The loading cover blanks input at the source; a gesture in flight is unwound below.
        Res<crate::loading_screen::LoadingScreen>,
        // Our own `[0xb4b3e4]`, which a control verdict on our character writes.
        ResMut<super::OwnControlLost>,
        // The active player, whom the posture verbs read and act on whichever body is driven.
        SelfPosture,
        // `ChrClasses.dbc`, whose relic flag the sheath toggle's ranged candidate reads.
        Option<Res<crate::chr_classes::ChrClassTable>>,
        // The predicted stand state `[player+0x1d68]`, which the posture verbs write.
        ResMut<super::StandPending>,
        // The optional movement processor seam, installed by whichever adapter owns the host.
        // Absent is the baseline: the controller owns every movement field and no seam runs.
        Option<ResMut<MovementProcessor>>,
    ),
    mut commands: Commands,
    mut player: ResMut<Player>,
    mut rig: ResMut<CameraControl>,
    collide: benilla_world::collision::WorldCollision,
    mut cameras: Query<(&mut Transform, &mut FlyCam), With<Camera>>,
    // The body we drive, a possessed creature's included ([`BodyQuery`]).
    mut body: BodyQuery,
    window: Single<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    // A press starts a look and arms a click test, which the release settles on time and travel.
    mut world_clicks: (MessageWriter<WorldClick>, MessageWriter<WorldRightClick>),
    mut click_test: (
        Local<Option<camera::PressGesture>>,
        Local<Option<camera::PressGesture>>,
    ),
    // The liquid query, the transports, the parent chain from a deck collider to its transport,
    // and the active mover `0xc4da98`, as whether it is our own character.
    world_q: (
        benilla_world::world_point::WorldPoint,
        TransportQuery,
        Query<&ChildOf>,
        Query<Has<crate::net::SelfPlayer>, With<crate::net::ActiveMover>>,
        // Our create's transport block and the transport it names, placed or not yet.
        (
            Query<&crate::transport::TransportRider, With<Embodied>>,
            Res<crate::net::GuidIndex>,
            Query<&Transform, (Without<Embodied>, Without<FlyCam>, Without<Camera>)>,
            Query<&crate::net::ObjectStore>,
        ),
    ),
) {
    let (world, transports, child_of) = (&world_q.0, &world_q.1, &world_q.2);
    let (left_click, right_click) = (&mut *click_test.0, &mut *click_test.1);
    let Ok((mut cam_t, mut cam)) = cameras.single_mut() else {
        return;
    };
    let (mut window, mut cursor_opts) = window.into_inner();
    let mouse_motion = &pointer.0;
    let look_cfg = *pointer.1;
    let zoom_max = pointer.2.max;
    let (move_speed, capsule, inspect, ui_capture, click_consumed) = (
        &speed_capsule.0,
        &speed_capsule.1 .0,
        &speed_capsule.2,
        &speed_capsule.3,
        &speed_capsule.4,
    );
    let binds = &speed_capsule.5;
    let view_subject = &speed_capsule.6;
    let self_guid = speed_capsule.7 .0;
    let scoped = &speed_capsule.8;
    let covered = speed_capsule.9.covering();
    // Far sight turns the auto-follow off: our facing is not "behind" for a body we only watch.
    let follow_cfg = camera::FollowConfig {
        style: if view_subject.remote.is_some() {
            camera::FollowStyle::Never
        } else {
            pointer.3.style
        },
        tracking_style: if view_subject.remote.is_some() {
            camera::FollowStyle::Never
        } else {
            pointer.3.tracking_style
        },
        view_customised: pointer.6.as_ref().is_some_and(|v| v.current_customised()),
        ..*pointer.3
    };
    let dt = time.delta_secs();
    // Free fly reads no keys while an EditBox is `typing` (not the per-key `consumed`, a binding's
    // concern); `binds` applied that gate when it latched, and the dev chords ignore it.
    let typing = ui_capture.typing;

    // Every press below comes from `camera::latch_world_mouse`, so one the UI ate reaches none of
    // them; the raw buttons only hold and release a gesture the world already owns. Read before
    // the look session: the not-driving path seats its camera from this word and returns early.
    let input::LookInput {
        both_buttons,
        follow_command,
    } = input::look_input(binds, &player, &rig);

    // Last frame's word, as `0x50fee0` reads it: its caller `0x514446` precedes the mover lookup.
    let dynamics = camera_dynamics::DynamicsInput {
        options: *pointer.4,
        nearclip: pointer.5.nearclip,
        // Last frame's cached surface, the reference's own lag.
        surface_y: player.liquid_surface,
        smooth_style: pointer.3.style,
        tracking_style: pointer.3.tracking_style,
        subject: camera_dynamics::SubjectState {
            move_flags: player.move_flags,
            // The driven body's descriptor: a possessed creature's taxi state, not ours.
            taxi: body
                .single()
                .ok()
                .and_then(|(_, _, _, _, _, store, ..)| store)
                .is_some_and(|s| s.0.unit_flags() & 0x0010_0000 != 0),
            track: player.server_riding,
            fear: player.control_lost,
            facing: player.face_yaw,
            mounted: body
                .single()
                .ok()
                .and_then(|(_, _, _, _, _, store, ..)| store)
                .is_some_and(|s| s.0.unit_mount_display_id() > 0),
            // `GetCurrentSpeed` (`0x7c4c90`) on the mover's speeds and last frame's word.
            speed: body
                .single()
                .ok()
                .and_then(|(_, _, _, _, _, _, speeds, ..)| speeds)
                .map_or(0.0, |s| crate::net::current_speed(&s.0, player.move_flags)),
            command: follow_command,
            scoped: scoped.active(),
        },
    };

    // A shadow copy, written back only on change: a `Mut` each frame marks `CursorOptions` changed,
    // and bevy_winit then re-applies the cursor to the OS every frame, which can stall.
    let mut opts_shadow = cursor_opts.bypass_change_detection().clone();
    // The facing before the look session, restored below when the mouse may not turn the body.
    let yaw_before_look = player.face_yaw;
    // Off the driven body's descriptor. Stunned is `UNIT_FIELD_FLAGS & 0x40000` (`0x5145b0`),
    // which skips the turn and pitch emitters (`0x514755`). Dead fails the shared precondition
    // `0x5144e0` (health above zero), so a corpse is stunned to the input tick; a ghost has
    // health 1. The stand state gates the mouse alone: `0x5145e0` wants the predicted
    // `GetStandState() == 0` (`0x5ed570`; [`super::StandPending`] is `[player+0x1d68]`).
    let (stunned, dead, stand_byte) = body
        .single()
        .ok()
        .and_then(|(.., store, _, _, _, _)| {
            store.map(|s| {
                (
                    s.0.unit_flags() & UNIT_FLAG_STUNNED != 0,
                    s.0.unit_is_dead(),
                    s.0.unit_stand_state(),
                )
            })
        })
        .unwrap_or((false, false, 0));
    // The prediction is the active player's ([`posture`]): a possessed body reads its own byte.
    let self_driven = speed_capsule
        .11
        .single()
        .is_some_and(|(.., embodied)| embodied);
    let stand_state = if self_driven {
        speed_capsule.13.stand_state(stand_byte)
    } else {
        stand_byte
    };
    // The shared precondition `0x5144e0`, once per tick. Its far-sight conjunct,
    // `!(IsActivePlayer(mover) && [mover+0x1c70] & 1)`, reads the resolved subject (`0x5ee290`
    // latches only once resolved) and spares a possessed mover, which sets the same field.
    let mover = state::MoverInput {
        dead,
        view_is_out: state::view_is_out(
            player.foreign_mover.is_none(),
            view_subject.remote.is_some(),
        ),
    };
    // `0x5145b0` now, for the mouse turn; its translate sibling waits for this frame's root edge.
    let may_turn = mover.may_turn(stunned);
    // `0x5145e0`: `may_turn` and standing; a turn key stands a seated body up instead.
    let mouse_turns_body = mover.mouse_may_turn_body(stunned, stand_state);
    // The drunk wobble, for the veer and the swim porpoise; zero when turning is refused, as the
    // reference's sits behind `0x5145b0` (`0x60aa47`).
    let drunk_wobble = {
        let f = body
            .single()
            .ok()
            .and_then(|(.., store, _, _, _, _)| store.and_then(|s| s.0.player_drunk_byte()))
            .map_or(0.0, drunk::fraction);
        if !may_turn {
            0.0
        } else {
            drunk::wobble(time.elapsed().as_millis() as u32, f)
        }
    };
    // A loading cover cancels a gesture in flight: ending the look session would settle its click
    // test and click the world we just left, so the tests are dropped first.
    if covered {
        *left_click = None;
        *right_click = None;
    }
    let focused = window.focused;
    let looked = run_look_session(
        mouse_motion,
        both_buttons,
        &mut rig,
        &mut cam,
        &mut player.face_yaw,
        &mut window,
        &mut opts_shadow,
        inspect.enabled,
        click_consumed.0,
        &mut world_clicks.0,
        &mut world_clicks.1,
        left_click,
        right_click,
        look_cfg,
        &dynamics,
        time.elapsed_secs(),
        focused,
    );
    // The mouse turns the view, but the body hand-off (`0x514474`) skips a body that is stunned,
    // dead or seated (`0x5145e0`) or not ours to drive (`control_lost`, `reseat`), while the
    // camera rotate (`0x514444`) still runs. The reference never writes the facing; writing and
    // restoring is the same downstream. Standing up hands the camera's yaw back at the next
    // motion sample, a snap, as `0x5103e0` commits the camera's facing rather than a delta.
    if !mouse_turns_body || player.control_lost || player.reseat {
        player.face_yaw = yaw_before_look;
    }
    // The hand-off runs on each motion and on the press that completes both mouse bits, motion or
    // none (`0x514924`-`0x514966`), behind the same body gate `0x5145e0`.
    let hands_off = (looked || input::both_engaged(binds, &rig, both_buttons))
        && mouse_turns_body
        && !player.control_lost
        && !player.reseat;
    {
        let cur = cursor_opts.bypass_change_detection();
        if cur.visible != opts_shadow.visible
            || cur.grab_mode != opts_shadow.grab_mode
            || cur.hit_test != opts_shadow.hit_test
        {
            *cursor_opts = opts_shadow;
        }
    }

    // A rebound zoom key steps 1.0 per press, the stock `CameraZoomIn(1.0)` (`Bindings.xml:707`).
    let zoom = binds.amount(crate::bindings::Input::CameraZoomIn)
        - binds.amount(crate::bindings::Input::CameraZoomOut);
    apply_zoom_scroll(zoom, dt, &mut rig, zoom_max, pointer.2.move_speed);
    // The `MoveView*` axes, the camera update's `0x5112d0`, after this frame's calls.
    rig.view_axes.apply(binds.view_calls());
    camera_axes::tick(
        &mut rig,
        &mut cam,
        dt,
        look_cfg,
        pointer.2.move_speed,
        zoom_max,
    );

    // Free fly is a dev tool on the dev chord: a bare `F` is the player's to bind.
    if crate::run_mode::dev_chord(&keys, KeyCode::KeyF) {
        player.detached = !player.detached;
    }

    // The server's movement edges and their acks. Speed changes come back unacked while we drive,
    // for the movement stream to ack with the live pose.
    let server_moves = wire_in::apply_server_moves(
        &time,
        &mut commands,
        &mut player,
        &mut speed_capsule.10,
        &mut cam,
        &net.0,
        &mut net.1,
        &mut net.2,
        &mut net.4,
        &mut net.5,
        &mut net.12,
        &mut net.9,
        &mut net.11,
        self_guid,
        transports,
        body.single()
            .ok()
            .map(|(_, t, ..)| (t.translation, server_ride::yaw_of(t.rotation))),
        self_seat(&world_q.4),
    );
    let speed_acks = server_moves.speed_acks;
    if let (true, Ok((e, ..))) = (server_moves.precast_ended, body.single()) {
        crate::creature_anim::end_precast(&mut commands, e, crate::creature_anim::PrecastTail::Run);
    }

    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();

    // On a transport, recompose the rider from the deck's pose this frame, before input integrates.
    ride::carry(&mut player, &mut cam, transports, &world_q.4 .1);

    if player.active && !player.detached {
        // The cast bar's self-cancel, at the command press ([`input::cancels_cast`]) and ahead of
        // the mover: between the press `0x5150a7` and the cancel `0x51512a` stands only the taint
        // test `0x494a50`, so it runs whoever drives the body. It reads the commands, never the
        // streamed word, which a root empties of direction bits. The cast cancel tests no mover
        // (`0x5150f2`); the channel's compares the player with the active mover
        // (`0x51517b`-`0x515191`), which possession sets to the creature and lost control zeroes
        // (`0x5fa600`), so steering a possessed creature, or pressing while feared, ends a cast but
        // not a channel.
        let active_mover = world_q.3.iter().next();
        let autorun_on = input::toggle_autorun(binds, &mut player);
        if input::cancels_cast(
            binds,
            autorun_on,
            input::both_engaged(binds, &rig, both_buttons),
            active_mover.is_some(),
        ) {
            net.7.pressed = true;
            net.7.mover_is_player = active_mover == Some(true);
        }
        // Not driving, while the camera stays on the body and input, physics and the stream yield:
        // - `server_riding`: a server spline (Charge, knockback, taxi, a flee path) owns the body;
        //   `drive_self_ride` has mirrored it into `Player`, reporting FORWARD on purpose.
        // - `control_lost`: someone else, or fear, drives our body. The server neither roots nor
        //   validates the victim, so this gate is the immobility.
        // - `reseat`: the mover guid is changing and the claimed unit has not streamed in; moves
        //   carry no guid, so driving would write one body's pose onto another.
        // Possession is none of these: it runs the ordinary path below, on the creature.
        if player.server_riding || player.control_lost || player.reseat {
            // A movement module never steers a body another owner drives; drain its proposal so a
            // stale intent cannot be applied when control returns.
            if let Some(processor) = speed_capsule.14.as_deref_mut() {
                processor.drain();
            }
            // The latch outlives the yield: lost control and an unresolved reseat leave the tick
            // no unit (`0x51466e`), but a ride keeps the mover and fails `0x5144e0`, so the tick
            // the ride's monster move runs (`0x602053`), and each press's, clears the bit
            // (`0x514748`).
            if player.server_riding && active_mover.is_some() {
                player.autorun = false;
            }
            // The moving body's transform is the truth; without this the camera stays behind
            // during a fear. Not while reseating, when `Player` still holds the body let go.
            if player.control_lost && !player.reseat {
                if let Ok((_, t, ..)) = body.single() {
                    let yaw = server_ride::yaw_of(t.rotation);
                    player.pos = t.translation;
                    player.face_yaw = yaw;
                    player.model_yaw = yaw;
                }
            }
            let head = player.pos + Vec3::Y * (CAPSULE_HEIGHT - CAPSULE_RADIUS);
            // Far sight outlives all three (Sentry Totem has no interrupt flags), and the
            // auto-follow runs through the reference's `Track` and `Fear` states.
            camera::seat_on_subject(
                dt,
                0.0,
                player.pos,
                head,
                body.single().ok().and_then(|(_, _, _, pivot, .., net)| {
                    // The pivot preset follows the body's own SWIMMING (`0x50f880`), here the
                    // last streamed word, which `wire_in` merges from the server's poses.
                    body_pose::pivot_target(
                        pivot,
                        net,
                        player.move_flags() & crate::creature_anim::move_flags::SWIMMING != 0,
                    )
                }),
                view_subject,
                &mut rig,
                &mut cam,
                &mut cam_t,
                &collide,
                &camera::FollowInput {
                    cfg: follow_cfg,
                    face_yaw: player.face_yaw,
                    command: follow_command,
                },
                &dynamics,
            );
            // Flush a stale run once, but never under a ride, whose FORWARD is deliberate.
            if !player.server_riding {
                movement_net::park_mover(&net.0 .0, &mut player);
            }
            // After the park, so a fear's ack carries the stopped word.
            movement_net::ack_speeds_undriven(&net.0 .0, &player, &speed_acks);
            return;
        }
        // `0x514560`, after `apply_server_moves`, so this frame's root edge is already in `modes`.
        let may_translate = mover.may_translate(player.modes.rooted);
        let axes = input::move_axes(
            binds,
            &buttons,
            &mut player,
            &rig,
            both_buttons,
            may_translate,
            may_turn,
        );
        let input::MoveAxes {
            fwd: fwd_axis,
            side: side_axis,
            mouselook,
            turning,
            translating,
            turn_left,
            turn_right,
            ..
        } = axes;
        if fwd_axis != 0 || side_axis != 0 {
            net.13.moved();
        }
        if mouselook {
            net.13.mouselooked();
        }
        // The driven unit's own speeds: the reference's input path never reads ours for another
        // mover, so a possessed creature moves and turns at its own.
        let mover_speeds = body.single().ok().and_then(|q| q.6).map(|s| s.0);
        // Zero is the ctor state, not a rate: a unit whose create block has not landed falls back.
        let turn_rate = mover_speeds
            .map(|s| s.turn_rate)
            .filter(|r| *r > 0.0)
            .unwrap_or(TURN_RATE);
        // This frame's own turn; `seat_camera` carries the camera by it, the two turning as one.
        let mut turn_delta = 0.0;
        if turning {
            let mut turn = 0.0;
            if turn_left {
                turn += 1.0;
            }
            if turn_right {
                turn -= 1.0;
            }
            // 0.75× while translating or falling (`flags & 0x200f`, `0x7c5c73`).
            let slowed = translating || player.airborne_since.is_some();
            let rate = turn_rate * if slowed { TURN_RATE_MOVING } else { 1.0 };
            turn_delta = turn * rate * dt;
            player.face_yaw += turn_delta;
        }
        // The drunk veer: while moving, the wobble adds to the facing every frame and commits
        // through the facing setter, so it streams like a turn (`0x60aa70`-`0x60aab7`,
        // `0x60de30`); a held keyboard turn skips it (`0x60aa5a`). Both yaw conventions grow
        // leftward, and it joins `turn_delta`, so the camera follows the meander.
        if drunk_wobble != 0.0 && translating && !turning {
            player.face_yaw += drunk_wobble;
            turn_delta = drunk_wobble;
        }
        let face_rot = Quat::from_rotation_y(player.face_yaw);
        let move_fwd = flat(face_rot * Vec3::NEG_Z);
        let move_right = flat(face_rot * Vec3::X);
        let mut dir = Vec3::ZERO;
        // One step in the net axis's sign, as the emitter issues one START in `sign(axis)`.
        match fwd_axis.signum() {
            1 => dir += move_fwd,
            -1 => dir -= move_fwd,
            _ => {}
        }
        // Strafe likewise; a cancelled pair is no strafe.
        match side_axis.signum() {
            1 => dir += move_right,
            -1 => dir -= move_right,
            _ => {}
        }
        // Translation dies here, but a root leaves turning live: the reference's allow-list permits
        // the turn, pitch, run/walk and SetFacing commands (`0x615c71`, `0x618054`). A stun, which
        // vmangos applies with a root (`SpellAuras.cpp:3502`), or death stops the turn as well.
        if !may_translate {
            dir = Vec3::ZERO;
        }
        let moving = dir != Vec3::ZERO;
        // A keyboard turn or the veer stands a seated body; a mouse turn does not, since the
        // facing commit refuses a seated player (`0x51460c`, `0x51520a`) and the turn emitter
        // skips its stand arm while the right button is held (`0x514f6d`). A right-click stands
        // you instead (`0x514ae0`, `camera::PressGesture::is_click`; `crate::target::click`).
        let turned = turn_delta != 0.0;
        // `TOGGLERUN` before the speed select, which reads the bit this frame's press left: the
        // mover reads `CMovement+0x40` after the input phase.
        walk::update(&mut player, &body, binds);
        // The stand state and the sheath toggle, which interlock; returns the committed stand.
        let reads = posture::PlayerReads {
            control_lost: speed_capsule.10 .0,
        };
        let stand_now = posture::update(
            &mut player,
            &mut speed_capsule.13,
            &body,
            &speed_capsule.11,
            binds,
            &net.0,
            &mut commands,
            &mut net.3,
            &mut net.14,
            &mut net.10,
            &mut net.15,
            reads,
            speed_capsule.12.as_deref().map(|t| &t.0),
            moving,
            turned,
        );
        // The backward flag selects the backward speed over strafe; a backward jump lands shorter.
        let net_backward = fwd_axis < 0;
        // The server's speeds (the LIVING block, then `SMSG_FORCE_*_SPEED_CHANGE`), or before the
        // create and under the `WOW_MOVE_SPEED` override, a set at the 2.5/4.5/7.0 ratios.
        let speeds = match mover_speeds {
            Some(s) if !move_speed.env_override => s,
            _ => benilla_protocol::MoveSpeeds {
                walk: move_speed.value * WALK_RATIO,
                run: move_speed.value,
                run_back: move_speed.value * RUN_BACK_RATIO,
                ..Default::default()
            },
        };
        // `GetCurrentSpeed` (`0x7c4c90`), shared with the remote extrapolator: the walk arm comes
        // before the backward min, so walking backwards is walk speed. It takes this frame's gait
        // intent, since the wire word is built after the mover.
        let speed = crate::net::current_speed(
            &speeds,
            if net_backward {
                move_flags::BACKWARD
            } else {
                move_flags::FORWARD
            } | if player.walking {
                move_flags::WALK_MODE
            } else {
                0
            },
        );
        // `Jump` (`0x513bd0`) inlines `0x5144e0` and `0x514560`, which is `may_translate` term for
        // term: health, root and stand state 7. Hover's refusal is the movement handler's
        // (`0x7c623a`), which keeps the mounted flourish reachable while hovering.
        let mut want_jump = binds.fired(crate::bindings::Input::Jump) && may_translate;

        // Swim or walk, latched with hysteresis at the `0x6030c0` boundary against flicker.
        let surface_y = swim::surface_over_feet(world, player.pos);
        // For the camera's water corridor, which reads it a frame late, as `0x670630` does.
        player.liquid_surface = surface_y;
        let swimming = swim::update_swimming(&mut player, surface_y, time.elapsed_secs());
        if let Some(surface) = surface_y {
            move_trace::swim(player.pos.y, surface, swimming, player.collision_height.0);
        }
        // Space while swimming is the Jump command (`0x7c6230`), once per press: it breaches at
        // the surface and hops about 1.6 yd below it, re-latching swim once the launch halves
        // (`0x7c5de0`). It clears SWIMMING before the mover, so this frame's mover, flags and wire
        // see a jump. Hover refuses it (`0x7c623a`, ahead of the take-off select at `0x7c6261`),
        // and hover does not stop swim entry (`0x6030c0` tests only LEVITATING).
        //
        // The `Jump(force = 0)` a hover grant owes: `force` 0 skips the hover refusal (`0x7c6236`),
        // so it launches a body already hovering; root and falling still refuse it (`0x7c625c`).
        let wire_jump = player.take_wire_jump();
        // The knockback: horizontal `(cos, sin) · xy_speed` in absolute world XY, vertical
        // `−zspeed`, since the wire's take-off speed is down-positive, as in the jump tail.
        let knockback = player.take_knockback();
        let knock_launch = knockback.map(|k| {
            let (c, sn, xy) = (k.launch.cos_angle, k.launch.sin_angle, k.launch.xy_speed);
            wow_to_bevy([c * xy, sn * xy, -k.launch.zspeed])
        });

        // A knockback sets FALLING, which excludes SWIMMING; it brings its own launch, so the land
        // mover flies it, not `breach_step`.
        let breach = swimming && (want_jump && !player.modes.hover || wire_jump);
        let knock_breach = swimming && knock_launch.is_some();
        if breach || knock_breach {
            player.swimming = false;
        }
        let swimming = swimming && !breach && !knock_breach;

        // Read by both the swim mover and the flag build, so the two cannot disagree.
        let (swim_fwd, swim_side) = if swimming {
            swim::translate_amounts(&axes, !may_translate)
        } else {
            (0.0, 0.0)
        };

        // The pitch keys: the input tick's pitch emitter (`0x515010`, behind `0x514755` and the
        // swim test `0x514766`), then the mover's pitch at the pitch rate (`0x7c4f80`), before
        // the swim mover reads it.
        swim::pitch_emitter(
            &mut player,
            (
                binds.pressed(crate::bindings::Input::PitchUp),
                binds.pressed(crate::bindings::Input::PitchDown),
            ),
            follow_command & camera::follow_cmd::RIGHT_MOUSE != 0,
            may_turn,
            swimming,
        );
        // The integration runs in any mode the bits survive into (`0x7c53b9`), at three quarters
        // under a direction bit or FALLING (`0x200f`, `0x7c5cb3`).
        let moving = if swimming {
            swim_fwd != 0.0 || swim_side != 0.0
        } else {
            (may_translate && (fwd_axis != 0 || side_axis != 0)) || player.airborne_since.is_some()
        };
        swim::integrate_pitch_keys(&mut player, turn_rate, moving, dt);

        // The mounted Space flourish, the jump-key handler `0x60dea0`: mounted, still and grounded
        // plays MountSpecial (94) locally, then sends `CMSG_MOUNTSPECIAL_ANIM`; moving jumps;
        // turning in place eats the press. Airborne, the press falls through and the mover drops
        // it (the reference's ground-clearance test `0x605650`; the airborne arc stands in).
        if want_jump && !moving && !swimming && player.airborne_since.is_none() {
            if let Ok((e, .., store, _, _, _, _)) = body.single() {
                if store.is_some_and(|s| s.0.unit_mount_display_id() != 0) {
                    want_jump = false;
                    if !turning {
                        let _ = net.0 .0.send(ClientCommand::MountSpecial);
                        net.8.write(crate::creature_anim::MountFlourish { unit: e });
                    }
                }
            }
        }

        // The mover pitch (`CMovement+0x20`), in every mode: held when unsteered (`0x7c4f80`) and
        // set by mouse look to the aim ([`swim::hand_off_pitch`]), whose write precedes its
        // SWIMMING test (`0x7c6f91`). A left-drag orbit steers nothing. It is written per mouse
        // motion (event `0x400500cb`), not per frame, so other writers survive a still mouse. The
        // aim includes the pivot bias, as `0x5103e0` passes `[cam+0x104] + [cam+0xf4]`.
        if hands_off {
            swim::hand_off_pitch(&mut player, cam.pitch + rig.smart_pivot.bias());
        }
        let mut swim_pitch = 0.0_f32;
        // Pre-step feet Y, the true take-off height: the step already rises one jump tick.
        let launch_y = player.pos.y;
        let mover::Outcome {
            held,
            grounded,
            jumped,
            knocked,
            air_nudged,
            ground,
            gated,
        } = if breach {
            // Jump while swimming (`0x7c6230`): falls unconditionally, seeded about 14% over a land
            // jump, and streams as a normal jump.
            swim::breach_step(&mut player, &time, &collide, capsule)
        } else if swimming {
            // One swimming frame; it also picks the presented pitch, levelled at the surface cap.
            let frame = swim::drive_step(
                &mut player,
                &time,
                &collide,
                capsule,
                world,
                surface_y,
                (move_fwd, move_right),
                (swim_fwd, swim_side),
                mover_speeds,
                move_speed,
                if translating { drunk_wobble } else { 0.0 },
            );
            swim_pitch = frame.pitch;
            frame.outcome
        } else {
            // Water walking hands the mover the liquid surface as ground ([`mover::water_floor`]),
            // where the reference ORs the liquid layers into the walk trace's class mask
            // (`0x63162e`); liquid is queried here rather than swept.
            let water_floor = mover::water_floor(
                player.modes.water_walking,
                swimming,
                player.mover_pitch,
                surface_y,
            );
            mover::step(
                &mut player,
                &time,
                &collide,
                capsule,
                moving,
                dir,
                speed,
                want_jump,
                wire_jump,
                knock_launch,
                water_floor,
                // The air nudge speed is `min(walk, run)`, live (`0x7c4c90(1)`, `0x7c4d19`), with
                // the default walk speed until the server sends speeds.
                mover_speeds.map_or(super::AIR_NUDGE_SPEED, |s| s.walk.min(s.run)),
            )
        };

        // Traced here, the first point that knows both the latch and the mover's verdict.
        if let Some(k) = knockback {
            move_trace::knockback(knocked, k.launch);
        }

        // Movement module seam. The baseline controller above produced this frame's pose; a
        // validated module proposal may adjust only the fields the baseline already owns
        // (position delta, velocity, facing, the delegated steering flags). The wire writer below
        // is untouched and remains the only sender, and the server-correction path in `wire_in`
        // already ran, so a module can never overwrite an authoritative server pose.
        let module_correction = {
            let baseline = MovementBaseline {
                position: player.pos,
                velocity: Vec3::new(player.horiz_vel.x, player.vel_y, player.horiz_vel.z),
                facing: player.face_yaw,
                speed,
                dt_seconds: dt,
                turn_rate,
                may_translate,
                may_turn,
            };
            // A settle hold, gated step, knockback arc, transport deck or fallen-through tick
            // outranks the module: drain its proposal so a stale one can never apply later.
            let free =
                !held && !gated && !player.knock_arc && !player.settling && player.ride.is_none();
            let tick = match speed_capsule.14.as_deref_mut() {
                Some(processor) if free => processor.take_tick(&baseline),
                Some(processor) => {
                    processor.drain();
                    MovementProcessorTick::Baseline
                }
                None => MovementProcessorTick::Baseline,
            };
            match tick {
                MovementProcessorTick::Corrected(correction) => {
                    if apply_movement_correction(&mut *player, &correction) {
                        Some(correction)
                    } else {
                        // A non-finite application is refused before it reaches the transform: the
                        // baseline pose stands and the processor is closed for this motion.
                        if let Some(processor) = speed_capsule.14.as_deref_mut() {
                            processor.refuse(MovementProcessorRefusal::Intent(
                                MovementIntentRejection::NonFinitePosition,
                            ));
                        }
                        None
                    }
                }
                MovementProcessorTick::Baseline => None,
            }
        };

        let now = time.elapsed_secs();
        if gated {
            player.skip_arc_time(dt);
        }
        if gated != player.gated {
            player.gated = gated;
            move_trace::gate(gated, player.pos);
        }
        // Our FALLING: never while swimming, and a root ends it outright (`SetRoot` `0x7c7340`
        // calls `StopFalling` `0x7c6290`), leaving the body hanging; `grounded` keeps true contact.
        let airborne = !swimming && !held && !player.modes.rooted && (!grounded || jumped);
        // Airborne keeps the attachment, so a jump above a deck lands where it took off.
        ride::update_attachment(
            &mut player,
            transports,
            &world_q.4 .1,
            child_of,
            ground,
            grounded,
            swimming,
        );
        // The live flag word, the take-off-frozen one the animation reads, and the fall clock.
        let flags::FrameFlags {
            wire: mut move_flags_now,
            pose: pose_flags,
            landed,
            fall_time: wire_fall_time,
        } = flags::this_frame(
            &mut player,
            &axes,
            swimming.then_some((swim_fwd, swim_side)),
            airborne,
            jumped,
            held,
            air_nudged,
            may_translate,
            may_turn,
            now,
            launch_y,
        );
        // A movement module may replace the delegated steering bits only. Root, falling, swimming,
        // transport, the pitch bits and the granted modes stay the baseline's and the server's, so
        // an accepted correction cannot lift a root or invent an arc. The pose word keeps the
        // baseline's takeoff-frozen directions: the module owns the wire, not the animation.
        if let Some(correction) = module_correction {
            move_flags_now = merge_movement_flags(move_flags_now, &correction);
        }
        let anim_flags = gait::drive_body_heading(
            &mut player,
            pose_flags,
            dt,
            swimming,
            moving,
            airborne,
            turning || mouselook,
            turn_rate,
        );
        let cam_pivot_target = body_pose::drive(
            &player,
            &mut body,
            &mut net.6,
            swimming,
            swim_pitch,
            move_flags_now,
            anim_flags,
            landed,
            stand_now,
        );

        // The camera sweep's root: the capsule's top hemisphere centre, not the framing pivot.
        let head = player.pos + Vec3::Y * (CAPSULE_HEIGHT - CAPSULE_RADIUS);
        // The spyglass lock (aura 76): re-parking each frame stands in for the reference's camera
        // flag `0x8`, which makes `SetCameraView` return early.
        if scoped.active() {
            rig.park_distance(0.0);
        }
        // Far sight moves only the picture: `seat_on_subject` orbits the object, all else runs.

        // `WOW_CAM_DUMP`: this frame's input beside `seat_camera`'s realized `[cam]` line.
        if crate::player::camera::cam_dump_enabled() {
            eprintln!(
                "[turn] t={:.6} dt={:.6} dx={:.3} dy={:.3} look={} face={:.6} model={:.6} \
                 pos [{:.4},{:.4},{:.4}] pivot={:.4}->{:.4}",
                time.elapsed_secs_f64(),
                dt,
                mouse_motion.delta.x,
                mouse_motion.delta.y,
                match rig.look {
                    Some(LookButton::Right) => "R",
                    Some(LookButton::Left) => "L",
                    None => "-",
                },
                player.face_yaw,
                player.model_yaw,
                player.pos.x,
                player.pos.y,
                player.pos.z,
                // The pivot channel's live height and its target.
                rig.pivot.probe().0,
                rig.pivot.probe().1,
            );
        }
        // `turn_delta` is our own turn; the deck's yaw reached `cam.yaw` in the ride carry.
        let follow = camera::FollowInput {
            cfg: follow_cfg,
            face_yaw: player.face_yaw,
            command: follow_command,
        };
        camera::seat_on_subject(
            dt,
            turn_delta,
            player.pos,
            head,
            cam_pivot_target,
            view_subject,
            &mut rig,
            &mut cam,
            &mut cam_t,
            &collide,
            &follow,
            &dynamics,
        );

        let steering_ourselves = player.foreign_mover.is_none();
        // The loot window's walk-away, the movement-start guard `0x60e990`: forward, back, strafe,
        // keyboard turn, pitch and jump close it, mouse-look facing does not, and only for our
        // own character.
        if steering_ourselves
            && (move_flags_now
                & (move_flags::ANY_MOVE | move_flags::TURN_LEFT | move_flags::TURN_RIGHT)
                & !player.move_flags
                != 0
                || jumped)
        {
            net.14 .0 = true;
        }

        // Stream the movement: a `MSG_MOVE_*` per axis transition, the jump and fall edges and a
        // 500 ms heartbeat, which vmangos relays to nearby players.
        let wire_transport = movement_net::wire_transport(&player);
        // A held frame is simulation time skipped; the mover named is the one we drive.
        let skip = movement_net::SkipClock {
            dt,
            held: player.settling,
            gated,
            mover: player.foreign_mover.or(self_guid),
        };
        let precast_ended = movement_net::stream_self_movement(
            &net.0 .0,
            &mut player,
            move_flags_now,
            swim_pitch,
            movement_net::ArcEdges {
                jumped,
                // Wire launches send no `MSG_MOVE_JUMP`: the hover grant's jump sends nothing
                // (`0x61a620`) and a knockback sends its ack instead.
                wire_launch: knocked || wire_jump,
                air_nudged,
                landed,
                fall_time: wire_fall_time,
            },
            now,
            &speed_acks,
            // Acked only if flown: under root the reference drops the record unacked (`0x615c71`).
            knockback.filter(|_| knocked),
            wire_transport,
            skip,
        );
        if let (true, Ok((e, ..))) = (precast_ended, body.single()) {
            crate::creature_anim::end_precast(
                &mut commands,
                e,
                crate::creature_anim::PrecastTail::Run,
            );
        }
    } else {
        // Free fly. Park the mover first, so a detach mid-move leaves no stale flags for observers
        // to extrapolate; a no-op once stopped.
        movement_net::park_mover(&net.0 .0, &mut player);
        // Free fly is not the mover's tick: drop any proposal so it cannot apply after re-entry.
        if let Some(processor) = speed_capsule.14.as_deref_mut() {
            processor.drain();
        }
        camera::fly_free(dt, &keys, typing, &mut rig, &mut cam, &mut cam_t);
    }
}

// ============================================================================
// Movement module seam
// ============================================================================
//
// The seam is deliberately narrow. A `MovementModuleV1` (staged and validated by the hotpatch
// host) hands the controller one *proposal* per tick. [`evaluate_movement_intent`] decides whether
// the proposal is inside the contract this controller owns; an accepted [`MovementCorrection`]
// carries only a position delta, a velocity, a facing and the delegated steering flags. Nothing
// here sends a packet: [`wrath_stream_movement`] and `movement_net::stream_self_movement` remain
// the only movement writers, and the server-correction path in `wire_in` runs ahead of all of it.
//
// The processor is an optional resource. When it is absent the controller behaves exactly as
// before: `Disabled` is the default and no proposal is ever consulted.

/// The move-flag bits a movement module may own. Everything else (root, falling/falling-far,
/// swimming, on-transport, walk mode, the pitch bits and the server-granted modes) is baseline or
/// server authority; a proposal setting one is refused, never silently masked.
pub(crate) const MODULE_OWNED_FLAGS: u32 =
    move_flags::ANY_MOVE | move_flags::TURN_LEFT | move_flags::TURN_RIGHT;

/// Slack over the baseline's own frame travel, absorbing integration order differences between a
/// module and the mover. A proposal beyond it is a teleport-like jump, not this frame's motion.
pub(crate) const MODULE_STEP_SLACK: f32 = 2.0;
/// Floor so a standing body may still nudge within a frame's rounding noise.
pub(crate) const MODULE_STEP_FLOOR: f32 = 0.05;
/// Slack over the baseline's own speed for the proposed velocity.
pub(crate) const MODULE_SPEED_SLACK: f32 = 1.5;
/// Slack over the baseline's own turn for a facing change, and a floor for rounding noise.
const MODULE_FACING_SLACK: f32 = 2.0;
const MODULE_FACING_FLOOR: f32 = 0.2;

/// A validated module proposal, as the hosting adapter hands it over. Coordinates are Bevy world
/// units; the adapter converts from the ABI's own axes and never passes pointers or handles.
#[allow(dead_code)] // The hosting adapter is the only producer; see `MovementProcessor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MovementProposal {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) facing: f32,
    /// Validated but not applied: the mover owns the pitch channel.
    pub(crate) pitch: f32,
    pub(crate) move_flags: u32,
}

/// What the baseline controller produced this frame, before any module proposal. The module is
/// always judged against this, never against its own previous output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MovementBaseline {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) facing: f32,
    /// This frame's current speed cap (yd/s).
    pub(crate) speed: f32,
    pub(crate) dt_seconds: f32,
    pub(crate) turn_rate: f32,
    pub(crate) may_translate: bool,
    pub(crate) may_turn: bool,
}

/// An accepted correction: exactly the values the baseline controller owns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MovementCorrection {
    pub(crate) position_delta: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) facing: f32,
    pub(crate) move_flags: u32,
}

/// Why the controller refused a proposal. Rejection is a refusal, not a clamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MovementIntentRejection {
    NonFinitePosition,
    NonFiniteVelocity,
    NonFiniteFacing,
    NonFinitePitch,
    /// The proposal moved further than one frame at the baseline's own speed.
    TeleportJump,
    /// The proposed horizontal or vertical speed exceeded this frame's cap.
    SpeedCap,
    /// The proposed facing differs from the baseline by more than one frame's turn.
    FacingRate,
    /// The proposal set a bit the seam does not own, or turned under a stun.
    DisallowedFlags(u32),
    /// The proposal moved a body the baseline had rooted, stunned, seated or held.
    MotionWhileStopped,
    /// The baseline's own frame step was unusable.
    FrameCadence,
}

/// The controller's verdict for one proposal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MovementIntentVerdict {
    Accepted(MovementCorrection),
    Rejected(MovementIntentRejection),
}

/// Judge one module proposal against this frame's baseline. Pure: no ECS, clock or socket access.
///
/// The contract rejects rather than silently repairing: non-finite values, out-of-budget travel,
/// over-cap speed, an over-rate facing change, a flag the seam does not own and any motion while
/// the baseline has the body rooted, stunned or held.
pub(crate) fn evaluate_movement_intent(
    proposal: &MovementProposal,
    baseline: &MovementBaseline,
) -> MovementIntentVerdict {
    if !proposal.position.is_finite() {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::NonFinitePosition);
    }
    if !proposal.velocity.is_finite() {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::NonFiniteVelocity);
    }
    if !proposal.facing.is_finite() {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::NonFiniteFacing);
    }
    if !proposal.pitch.is_finite() {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::NonFinitePitch);
    }
    let dt = baseline.dt_seconds;
    if !dt.is_finite() || dt <= 0.0 || !baseline.speed.is_finite() {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::FrameCadence);
    }
    let disallowed = proposal.move_flags & !MODULE_OWNED_FLAGS;
    if disallowed != 0 {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::DisallowedFlags(
            disallowed,
        ));
    }
    if !baseline.may_translate
        && (proposal.move_flags & move_flags::ANY_MOVE != 0
            || proposal.position != baseline.position
            || proposal.velocity != Vec3::ZERO)
    {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::MotionWhileStopped);
    }
    if !baseline.may_turn
        && proposal.move_flags & (move_flags::TURN_LEFT | move_flags::TURN_RIGHT) != 0
    {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::DisallowedFlags(
            proposal.move_flags & (move_flags::TURN_LEFT | move_flags::TURN_RIGHT),
        ));
    }
    let delta = proposal.position - baseline.position;
    // The baseline's own motion is the reference: a held-direction frame is already at `speed`,
    // and an airborne or knockback arc may legitimately carry more than the ground cap.
    let body_speed = baseline.velocity.length().max(baseline.speed);
    let max_step = body_speed * dt * MODULE_STEP_SLACK + MODULE_STEP_FLOOR;
    if delta.length() > max_step {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::TeleportJump);
    }
    let horizontal = Vec3::new(proposal.velocity.x, 0.0, proposal.velocity.z).length();
    let max_horizontal = body_speed * MODULE_SPEED_SLACK + MODULE_STEP_FLOOR;
    if horizontal > max_horizontal {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::SpeedCap);
    }
    if proposal.velocity.y.abs() > TERMINAL_VELOCITY * MODULE_SPEED_SLACK {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::SpeedCap);
    }
    let turn_cap = if baseline.may_turn {
        (baseline.turn_rate * dt * MODULE_FACING_SLACK).max(MODULE_FACING_FLOOR)
    } else {
        0.0
    };
    let mut facing_delta = (proposal.facing - baseline.facing) % std::f32::consts::TAU;
    if facing_delta > std::f32::consts::PI {
        facing_delta -= std::f32::consts::TAU;
    } else if facing_delta < -std::f32::consts::PI {
        facing_delta += std::f32::consts::TAU;
    }
    if facing_delta.abs() > turn_cap {
        return MovementIntentVerdict::Rejected(MovementIntentRejection::FacingRate);
    }
    MovementIntentVerdict::Accepted(MovementCorrection {
        position_delta: delta,
        velocity: proposal.velocity,
        facing: proposal.facing,
        move_flags: proposal.move_flags & MODULE_OWNED_FLAGS,
    })
}

/// Write an accepted correction onto the player. Refuses (and writes nothing) if the result would
/// be non-finite, so no NaN transform can reach the pose or the wire.
pub(crate) fn apply_movement_correction(
    player: &mut Player,
    correction: &MovementCorrection,
) -> bool {
    let position = player.pos + correction.position_delta;
    if !position.is_finite() || !correction.velocity.is_finite() || !correction.facing.is_finite() {
        return false;
    }
    player.pos = position;
    player.horiz_vel = Vec3::new(correction.velocity.x, 0.0, correction.velocity.z);
    player.vel_y = correction.velocity.y;
    player.face_yaw = correction.facing;
    true
}

/// Merge a correction's delegated steering bits into this frame's baseline word.
pub(crate) fn merge_movement_flags(baseline: u32, correction: &MovementCorrection) -> u32 {
    (baseline & !MODULE_OWNED_FLAGS) | (correction.move_flags & MODULE_OWNED_FLAGS)
}

/// Which processor owns the movement tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MovementProcessorState {
    /// The seam is off. The baseline controller owns every field and no proposal is consulted.
    #[default]
    Disabled,
    /// The seam is enabled but no module is staged: baseline.
    Baseline,
    /// A validated candidate is staged and has not produced an accepted intent yet.
    Staged,
    /// The module produced an accepted intent; corrections may override the baseline each tick.
    Active,
    /// The last candidate/intent was refused. Baseline owns the tick until a new one is staged.
    Refused(MovementProcessorRefusal),
}

/// A typed boundary for the processor state.
#[allow(dead_code)] // `Host`/`Unloaded` are reserved for the hosting adapter; see the impl note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MovementProcessorRefusal {
    /// The proposal failed the controller's contract.
    Intent(MovementIntentRejection),
    /// The host refused the module above the controller (kind, ABI, publication, output bounds).
    Host,
    /// The module went away mid-motion; prediction returned to the baseline once.
    Unloaded,
    /// A proposal arrived with no staged candidate.
    NotStaged,
}

/// The controller's answer for this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MovementProcessorTick {
    /// Baseline owns every movement field this tick.
    Baseline,
    /// Apply this correction exactly once.
    Corrected(MovementCorrection),
}

/// Outcome of the most recent [`MovementProcessor::take_tick`] or [`MovementProcessor::drain`]
/// evaluation, consumed by the hosting adapter after [`control`] runs to drive
/// `Host::commit_movement_processor`, `Host::withdraw_movement_proposal`, or
/// `Host::unload_movement_processor`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MovementProcessorOutcome {
    /// No module proposal was evaluated or drained this tick.
    #[default]
    None,
    /// A proposal was drained without evaluation because the baseline held authority this frame
    /// (settle hold, root gate, knockback arc, transport ride, or free-fly).
    Drained,
    /// A staged candidate's proposal was accepted and promoted the processor to `Active`.
    Promoted,
    /// An already-active processor's proposal was accepted.
    ActiveAccepted,
    /// A staged candidate's proposal was rejected by [`evaluate_movement_intent`].
    StagedRejected(MovementIntentRejection),
    /// An active processor's proposal was rejected by [`evaluate_movement_intent`].
    ActiveRejected(MovementIntentRejection),
}

/// The movement processor selection. `Disabled` by default, so an absent resource is exactly the
/// pre-seam behavior. The hosting adapter stages a validated candidate, offers at most one
/// proposal per tick and reports an unload; the controller's verdict alone decides activation.
///
/// Supports both single-candidate staging and the retained-active / tolerated-refusal contract:
/// when a candidate stages over an already-active processor (`stage_over_active`) or when the host
/// reports a tolerated active failure (`tolerate`), `last_refusal` records the refusal beside the
/// retained `Active` state while handing this frame's prediction back to `Baseline`.
#[allow(dead_code)]
#[derive(Resource, Debug, Default, PartialEq)]
pub(crate) struct MovementProcessor {
    state: MovementProcessorState,
    proposal: Option<MovementProposal>,
    retained_active: bool,
    tolerated_handback: bool,
    last_refusal: Option<MovementProcessorRefusal>,
    last_outcome: MovementProcessorOutcome,
}

#[allow(dead_code)]
impl MovementProcessor {
    pub(crate) fn state(&self) -> MovementProcessorState {
        self.state
    }

    /// Whether a previously activated processor is retained underneath a currently `Staged`
    /// candidate.
    pub(crate) fn has_retained_active(&self) -> bool {
        self.retained_active
    }

    /// Most recent refusal observed by the processor, including a tolerated active refusal or a
    /// staged-candidate refusal that fell back to a retained `Active` processor.
    pub(crate) fn last_refusal(&self) -> Option<MovementProcessorRefusal> {
        match self.state {
            MovementProcessorState::Refused(reason) => Some(reason),
            _ => self.last_refusal,
        }
    }

    /// Drain the outcome of the most recent tick for the hosting adapter to commit or withdraw on
    /// the hotpatch host.
    pub(crate) fn take_outcome(&mut self) -> MovementProcessorOutcome {
        std::mem::replace(&mut self.last_outcome, MovementProcessorOutcome::None)
    }

    /// Turn the seam on without a module: still baseline until a candidate stages.
    pub(crate) fn enable(&mut self) {
        if matches!(self.state, MovementProcessorState::Disabled) {
            self.state = MovementProcessorState::Baseline;
        }
    }

    /// Turn the seam off and drop any proposal. Baseline resumes on the next tick, once.
    pub(crate) fn disable(&mut self) {
        self.state = MovementProcessorState::Disabled;
        self.proposal = None;
        self.retained_active = false;
        self.tolerated_handback = false;
        self.last_refusal = None;
        self.last_outcome = MovementProcessorOutcome::None;
    }

    /// Register a validated candidate from a baseline or refused slot.
    pub(crate) fn stage(&mut self) -> bool {
        if matches!(
            self.state,
            MovementProcessorState::Baseline | MovementProcessorState::Refused(_)
        ) {
            self.state = MovementProcessorState::Staged;
            self.retained_active = false;
            self.tolerated_handback = false;
            true
        } else {
            false
        }
    }

    /// Stage a new candidate while retaining the currently `Active` processor as fallback if the
    /// staged candidate fails its canary or intent evaluation.
    pub(crate) fn stage_over_active(&mut self) -> bool {
        match self.state {
            MovementProcessorState::Active => {
                self.retained_active = true;
                self.tolerated_handback = false;
                self.state = MovementProcessorState::Staged;
                true
            }
            MovementProcessorState::Staged if self.retained_active => {
                self.tolerated_handback = false;
                true
            }
            MovementProcessorState::Baseline | MovementProcessorState::Refused(_) => self.stage(),
            _ => false,
        }
    }

    /// Ensure the processor is marked `Active` when the host already holds an active processor.
    pub(crate) fn mark_active(&mut self) {
        if !matches!(self.state, MovementProcessorState::Disabled) {
            self.state = MovementProcessorState::Active;
            self.retained_active = false;
            self.tolerated_handback = false;
        }
    }

    /// Offer this tick's proposal. At most one is kept: a second offer is dropped, so no duplicate
    /// intent can be applied.
    pub(crate) fn offer(&mut self, proposal: MovementProposal) -> bool {
        if self.proposal.is_some() {
            return false;
        }
        self.tolerated_handback = false;
        self.proposal = Some(proposal);
        true
    }

    /// Record a tolerated active-processor refusal (`MovementRefusal::Tolerated`). Hands this
    /// tick's prediction back to `Baseline` while keeping `MovementProcessorState::Active` for the
    /// next valid frame and recording `reason` in [`Self::last_refusal`].
    pub(crate) fn tolerate(&mut self, reason: MovementProcessorRefusal) -> bool {
        self.proposal = None;
        if matches!(self.state, MovementProcessorState::Active) || self.retained_active {
            self.retained_active = false;
            self.state = MovementProcessorState::Active;
            self.last_refusal = Some(reason);
            self.tolerated_handback = true;
            true
        } else {
            self.refuse(reason);
            false
        }
    }

    /// Restore the retained `Active` processor after a staged candidate failed on the host while
    /// the host kept a different-digest active processor. Records `reason` in
    /// [`Self::last_refusal`] and hands this frame back to `Baseline`.
    pub(crate) fn retain_active_on_refusal(&mut self, reason: MovementProcessorRefusal) -> bool {
        self.proposal = None;
        if self.retained_active || matches!(self.state, MovementProcessorState::Active) {
            self.retained_active = false;
            self.state = MovementProcessorState::Active;
            self.last_refusal = Some(reason);
            self.tolerated_handback = true;
            true
        } else {
            self.refuse(reason);
            false
        }
    }

    /// Non-destructive per-frame handback (e.g. `MovementRefusal::FrameInput`): baseline owns this
    /// frame while `Staged`/`Active` selection and `retained_active` stay intact for the next frame.
    pub(crate) fn handback_frame(&mut self) {
        self.proposal = None;
        if matches!(
            self.state,
            MovementProcessorState::Staged | MovementProcessorState::Active
        ) {
            self.tolerated_handback = true;
        }
    }

    /// The module went away (unloaded, invalid output, failed candidate). Returns whether a module
    /// was held, so the caller's handback happens exactly once. A prior refusal is kept: it is the
    /// typed record the UI/log reads, and the next `stage` accepts from it.
    pub(crate) fn unload(&mut self) -> bool {
        let held = matches!(
            self.state,
            MovementProcessorState::Staged | MovementProcessorState::Active
        ) || self.retained_active;
        self.proposal = None;
        self.retained_active = false;
        self.tolerated_handback = false;
        self.last_outcome = MovementProcessorOutcome::None;
        if held {
            self.state = MovementProcessorState::Baseline;
            self.last_refusal = None;
        }
        held
    }

    /// Record a refusal. If a staged candidate was refused while a prior active processor was
    /// retained, falls back to `Active` with `reason` recorded in [`Self::last_refusal`];
    /// otherwise the slot stays `Refused(reason)` until a new candidate stages.
    pub(crate) fn refuse(&mut self, reason: MovementProcessorRefusal) {
        self.proposal = None;
        self.last_refusal = Some(reason);
        if let MovementProcessorRefusal::Intent(rejection) = reason {
            self.last_outcome = match self.last_outcome {
                MovementProcessorOutcome::Promoted => {
                    MovementProcessorOutcome::StagedRejected(rejection)
                }
                _ => MovementProcessorOutcome::ActiveRejected(rejection),
            };
        }
        if std::mem::take(&mut self.retained_active) {
            self.state = MovementProcessorState::Active;
            self.tolerated_handback = true;
        } else {
            self.tolerated_handback = false;
            self.state = MovementProcessorState::Refused(reason);
        }
    }

    /// Drop this tick's proposal without changing the selection (a frame the module may not
    /// steer: settle hold, root-gated step, knockback arc).
    pub(crate) fn drain(&mut self) {
        if self.proposal.take().is_some() {
            self.last_outcome = MovementProcessorOutcome::Drained;
        }
        self.tolerated_handback = false;
    }

    /// Select the tick's owner exactly once. The proposal is consumed here, so a correction can
    /// never be applied twice.
    pub(crate) fn take_tick(&mut self, baseline: &MovementBaseline) -> MovementProcessorTick {
        let proposal = self.proposal.take();
        let tolerated_handback = std::mem::replace(&mut self.tolerated_handback, false);
        self.last_outcome = MovementProcessorOutcome::None;
        match self.state {
            // Disabled and refused never consult a proposal; baseline keeps the body playable.
            MovementProcessorState::Disabled | MovementProcessorState::Refused(_) => {
                MovementProcessorTick::Baseline
            }
            MovementProcessorState::Baseline => {
                if proposal.is_some() {
                    // A proposal with no staged candidate would be an unowned intent.
                    let refusal = MovementProcessorRefusal::NotStaged;
                    self.last_refusal = Some(refusal);
                    self.state = MovementProcessorState::Refused(refusal);
                }
                MovementProcessorTick::Baseline
            }
            MovementProcessorState::Staged | MovementProcessorState::Active => {
                let was_staged = matches!(self.state, MovementProcessorState::Staged);
                let Some(proposal) = proposal else {
                    if !tolerated_handback && matches!(self.state, MovementProcessorState::Active) {
                        // The active module produced nothing this tick with no tolerated/frame
                        // handback: hand prediction back to the baseline once rather than holding
                        // a stale owner.
                        self.state = MovementProcessorState::Baseline;
                        self.last_refusal = None;
                    }
                    return MovementProcessorTick::Baseline;
                };
                match evaluate_movement_intent(&proposal, baseline) {
                    MovementIntentVerdict::Accepted(correction) => {
                        self.state = MovementProcessorState::Active;
                        self.retained_active = false;
                        self.last_refusal = None;
                        self.last_outcome = if was_staged {
                            MovementProcessorOutcome::Promoted
                        } else {
                            MovementProcessorOutcome::ActiveAccepted
                        };
                        MovementProcessorTick::Corrected(correction)
                    }
                    MovementIntentVerdict::Rejected(reason) => {
                        let refusal = MovementProcessorRefusal::Intent(reason);
                        self.last_refusal = Some(refusal);
                        if was_staged {
                            self.last_outcome = MovementProcessorOutcome::StagedRejected(reason);
                            if std::mem::take(&mut self.retained_active) {
                                self.state = MovementProcessorState::Active;
                            } else {
                                self.state = MovementProcessorState::Refused(refusal);
                            }
                        } else {
                            self.last_outcome = MovementProcessorOutcome::ActiveRejected(reason);
                            self.state = MovementProcessorState::Refused(refusal);
                        }
                        MovementProcessorTick::Baseline
                    }
                }
            }
        }
    }
}

/// Our create's transport block, the [`crate::transport::TransportRider`] tail on the body we
/// steer, and the streamed transport it names, as [`wire_in::apply_server_moves`] seats a login
/// aboard it.
#[allow(clippy::type_complexity)]
fn self_seat(
    q: &(
        Query<&crate::transport::TransportRider, With<Embodied>>,
        Res<crate::net::GuidIndex>,
        Query<&Transform, (Without<Embodied>, Without<FlyCam>, Without<Camera>)>,
        Query<&crate::net::ObjectStore>,
    ),
) -> Option<wire_in::SelfSeat> {
    let rider = q.0.single().ok()?;
    let entity = *q.1 .0.get(&rider.transport_guid)?;
    // 0x630a80: the lookup `0x468460` with the game object mask 0x20, then the transport bit 0x8
    // of its flags.
    let fields = &q.3.get(entity).ok()?.0;
    if fields.object_type_mask().unwrap_or(0) & 0x20 == 0 || fields.gameobject_flags() & 0x8 == 0 {
        return None;
    }
    Some(wire_in::SelfSeat {
        entity,
        guid: rider.transport_guid,
        local_pos: rider.local_pos,
        local_orientation: rider.local_orientation,
        deck: *q.2.get(entity).ok()?,
    })
}

#[cfg(test)]
mod movement_module_tests {
    use super::*;

    /// A grounded, at-speed frame: seven yards a second forward, one tick of 20 ms.
    fn baseline() -> MovementBaseline {
        MovementBaseline {
            position: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.0, 0.0, 7.0),
            facing: 0.0,
            speed: 7.0,
            dt_seconds: 0.02,
            turn_rate: TURN_RATE,
            may_translate: true,
            may_turn: true,
        }
    }

    /// A coherent proposal: one tick of forward travel with the forward bit.
    fn proposal() -> MovementProposal {
        MovementProposal {
            position: Vec3::new(1.0, 2.0, 3.1),
            velocity: Vec3::new(0.0, 0.0, 7.0),
            facing: 0.05,
            pitch: 0.0,
            move_flags: move_flags::FORWARD,
        }
    }

    fn correction_of(tick: MovementProcessorTick) -> MovementCorrection {
        match tick {
            MovementProcessorTick::Corrected(correction) => correction,
            MovementProcessorTick::Baseline => panic!("expected a correction, got baseline"),
        }
    }

    fn armed() -> MovementProcessor {
        let mut processor = MovementProcessor::default();
        processor.enable();
        assert!(processor.stage(), "a fresh seam stages a candidate");
        processor
    }

    /// Baseline -> alternate -> baseline: staged without a valid intent, promoted on the first
    /// accepted one, and handed back to the baseline when the adapter reports the module gone.
    #[test]
    fn baseline_then_alternate_then_baseline_selection_is_explicit() {
        let mut processor = MovementProcessor::default();
        assert_eq!(
            processor.state(),
            MovementProcessorState::Disabled,
            "the seam is off by default"
        );
        processor.offer(proposal());
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline,
            "a disabled seam never consults a proposal"
        );
        assert_eq!(processor.state(), MovementProcessorState::Disabled);

        processor.enable();
        assert_eq!(processor.state(), MovementProcessorState::Baseline);
        assert!(processor.stage());
        assert_eq!(processor.state(), MovementProcessorState::Staged);
        processor.offer(proposal());
        let correction = correction_of(processor.take_tick(&baseline()));
        assert!(
            (correction.position_delta.z - 0.1).abs() < 1e-5,
            "the accepted delta is the module's, {correction:?}"
        );
        assert_eq!(
            processor.state(),
            MovementProcessorState::Active,
            "the first accepted intent activates the candidate"
        );

        assert!(processor.unload(), "the adapter reports the module gone");
        assert_eq!(processor.state(), MovementProcessorState::Baseline);
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline
        );
        assert!(
            processor.stage(),
            "the baseline slot may stage the next candidate"
        );
    }

    /// A refused proposal leaves the baseline owning the tick, once, and is not replayed.
    #[test]
    fn an_invalid_proposal_leaves_the_baseline_once() {
        let mut processor = armed();
        let mut bad = proposal();
        bad.position = Vec3::new(1.0, 2.0, 400.0);
        processor.offer(bad);
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline
        );
        assert_eq!(
            processor.state(),
            MovementProcessorState::Refused(MovementProcessorRefusal::Intent(
                MovementIntentRejection::TeleportJump
            ))
        );
        // The next tick stays baseline: nothing is retried and nothing is applied.
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline
        );
        assert_eq!(
            processor.state(),
            MovementProcessorState::Refused(MovementProcessorRefusal::Intent(
                MovementIntentRejection::TeleportJump
            ))
        );
    }

    /// A mid-fall unload returns prediction to the baseline exactly once; a fall's own velocity is
    /// inside the contract.
    #[test]
    fn a_mid_fall_unload_hands_back_once() {
        let mut processor = armed();
        let falling = MovementBaseline {
            velocity: Vec3::new(0.0, -20.0, 4.0),
            ..baseline()
        };
        let mut falling_proposal = proposal();
        falling_proposal.velocity = Vec3::new(0.0, -20.0, 4.0);
        falling_proposal.position = falling.position + Vec3::new(0.0, -0.4, 0.08);
        processor.offer(falling_proposal);
        let correction = correction_of(processor.take_tick(&falling));
        assert_eq!(correction.velocity.y, -20.0);
        assert_eq!(
            processor.state(),
            MovementProcessorState::Active,
            "the fall is coherent under the module"
        );
        assert!(processor.unload());
        assert_eq!(processor.state(), MovementProcessorState::Baseline);
        assert!(
            !processor.unload(),
            "the handback must not happen twice for one fall"
        );
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline
        );
    }

    /// Non-finite values, a teleport-like jump, over-cap speed, an over-rate facing turn, a flag
    /// the seam does not own and motion under a root are all refusals, never silent clamps.
    #[test]
    fn non_finite_facing_speed_and_flag_proposals_are_refused() {
        let cases: [(fn(&mut MovementProposal), MovementIntentRejection); 7] = [
            (
                |p| p.position.x = f32::NAN,
                MovementIntentRejection::NonFinitePosition,
            ),
            (
                |p| p.velocity.y = f32::NAN,
                MovementIntentRejection::NonFiniteVelocity,
            ),
            (
                |p| p.facing = f32::NAN,
                MovementIntentRejection::NonFiniteFacing,
            ),
            (
                |p| p.pitch = f32::NAN,
                MovementIntentRejection::NonFinitePitch,
            ),
            (
                |p| p.position = p.position + Vec3::new(0.0, 0.0, 5.0),
                MovementIntentRejection::TeleportJump,
            ),
            (
                |p| p.velocity = Vec3::new(0.0, 0.0, 400.0),
                MovementIntentRejection::SpeedCap,
            ),
            (
                |p| p.facing = std::f32::consts::PI,
                MovementIntentRejection::FacingRate,
            ),
        ];
        for (mutate, expected) in cases {
            let mut probe = proposal();
            mutate(&mut probe);
            assert_eq!(
                evaluate_movement_intent(&probe, &baseline()),
                MovementIntentVerdict::Rejected(expected),
                "{expected:?}"
            );
        }
        // A server-owned flag is refused, not masked.
        let mut flagged = proposal();
        flagged.move_flags = move_flags::ROOT;
        assert_eq!(
            evaluate_movement_intent(&flagged, &baseline()),
            MovementIntentVerdict::Rejected(MovementIntentRejection::DisallowedFlags(
                move_flags::ROOT
            ))
        );
        // A rooted baseline refuses any motion, even with no steering bit set.
        let rooted = MovementBaseline {
            may_translate: false,
            ..baseline()
        };
        let mut moving = proposal();
        moving.move_flags = 0;
        assert_eq!(
            evaluate_movement_intent(&moving, &rooted),
            MovementIntentVerdict::Rejected(MovementIntentRejection::MotionWhileStopped)
        );
        // …while a zero-delta, zero-velocity proposal is still acceptable under the root.
        let mut standing = proposal();
        standing.position = rooted.position;
        standing.velocity = Vec3::ZERO;
        standing.facing = rooted.facing;
        standing.move_flags = 0;
        assert!(matches!(
            evaluate_movement_intent(&standing, &rooted),
            MovementIntentVerdict::Accepted(_)
        ));
    }

    /// One proposal can produce at most one correction, and a second offer in the same tick is
    /// dropped, so no duplicate movement intent reaches the flags or the wire.
    #[test]
    fn one_proposal_cannot_produce_two_intents() {
        let mut processor = armed();
        assert!(processor.offer(proposal()));
        assert!(
            !processor.offer(proposal()),
            "a second proposal in the same tick is dropped"
        );
        assert!(matches!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Corrected(_)
        ));
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline,
            "the one proposal is consumed; the next tick has only baseline"
        );
    }

    /// The applied correction is bounded and finite; a non-finite result writes nothing; the flag
    /// merge keeps every baseline/server bit and takes only the delegated ones.
    #[test]
    fn an_accepted_correction_stays_finite_and_the_flag_merge_is_bounded() {
        let mut player = Player::default();
        player.pos = Vec3::new(1.0, 2.0, 3.0);
        let correction = MovementCorrection {
            position_delta: Vec3::new(0.1, 0.0, 0.0),
            velocity: Vec3::new(0.0, -2.0, 5.0),
            facing: 0.5,
            move_flags: move_flags::BACKWARD,
        };
        assert!(apply_movement_correction(&mut player, &correction));
        assert_eq!(player.pos, Vec3::new(1.1, 2.0, 3.0));
        assert!(player.pos.is_finite());
        assert_eq!(player.face_yaw, 0.5);
        assert_eq!(player.vel_y, -2.0);
        assert_eq!(player.horiz_vel, Vec3::new(0.0, 0.0, 5.0));

        let before = player.pos;
        let bad = MovementCorrection {
            position_delta: Vec3::new(f32::NAN, 0.0, 0.0),
            ..correction
        };
        assert!(
            !apply_movement_correction(&mut player, &bad),
            "a NaN result is refused before it reaches the transform"
        );
        assert_eq!(player.pos, before);

        assert_eq!(
            merge_movement_flags(move_flags::FALLING | move_flags::FORWARD, &correction),
            move_flags::FALLING | move_flags::BACKWARD,
            "the arc bit stays the baseline's; only the steering bit is delegated"
        );
    }

    #[test]
    fn tolerated_refusal_and_retained_active_keep_the_active_processor_across_handback() {
        let mut processor = armed();
        processor.offer(proposal());
        assert!(matches!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Corrected(_)
        ));
        assert_eq!(processor.take_outcome(), MovementProcessorOutcome::Promoted);
        assert_eq!(processor.state(), MovementProcessorState::Active);
        assert_eq!(processor.last_refusal(), None);

        // A tolerated active failure hands this tick back to baseline while retaining Active
        // and recording the refusal beside it.
        assert!(processor.tolerate(MovementProcessorRefusal::Host));
        assert_eq!(processor.state(), MovementProcessorState::Active);
        assert_eq!(
            processor.last_refusal(),
            Some(MovementProcessorRefusal::Host)
        );
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline,
            "tolerated refusal hands this single tick back to baseline"
        );
        assert_eq!(
            processor.state(),
            MovementProcessorState::Active,
            "the active processor stays selected for the next valid tick"
        );

        // Next valid proposal clears the tolerated refusal and applies normally.
        processor.offer(proposal());
        assert!(matches!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Corrected(_)
        ));
        assert_eq!(
            processor.take_outcome(),
            MovementProcessorOutcome::ActiveAccepted
        );
        assert_eq!(processor.last_refusal(), None);

        // Stage a replacement candidate over the active processor; when that candidate's intent is
        // rejected, the retained active processor is restored with the refusal recorded.
        assert!(processor.stage_over_active());
        assert_eq!(processor.state(), MovementProcessorState::Staged);
        assert!(processor.has_retained_active());
        let mut bad = proposal();
        bad.position = Vec3::new(1.0, 2.0, 400.0);
        processor.offer(bad);
        assert_eq!(
            processor.take_tick(&baseline()),
            MovementProcessorTick::Baseline
        );
        assert_eq!(
            processor.take_outcome(),
            MovementProcessorOutcome::StagedRejected(MovementIntentRejection::TeleportJump)
        );
        assert_eq!(
            processor.state(),
            MovementProcessorState::Active,
            "retained active processor resumes when the staged replacement is rejected"
        );
        assert_eq!(
            processor.last_refusal(),
            Some(MovementProcessorRefusal::Intent(
                MovementIntentRejection::TeleportJump
            ))
        );
    }
}
