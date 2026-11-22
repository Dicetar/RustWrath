//! `benilla`: a from-scratch World of Warcraft 1.12.1 client on Bevy. It opens the patch chain from
//! the install (`$WOW_DATA`, the project folder on a dev build, else beside the binary), streams
//! the world around the player through the `benilla-assets` `mpq://` pipeline, and runs the network
//! session on a background thread ([`net`]).
//!
//! The world is loaded when a character enters it and released when they leave; the glue screens
//! have no world behind them, so with no server the client sits at the login screen, as the
//! reference does. The capture harness (`$WOW_CAPTURE`) boots straight in-world.

// A player build's dead code is the seam working: with `--no-default-features` every symbol only an
// instrument calls goes unused, and a `cfg` per item would spread seam knowledge into gameplay
// modules. Dev builds warn normally.
#![cfg_attr(not(feature = "dev"), allow(dead_code))]
#![recursion_limit = "256"]

/// The realtime-audio allocation tripwire: in debug builds an allocation inside `sound::output`'s
/// `no_alloc` scopes (the output IO callback, the render pass) aborts the process; release builds
/// carry no wrapper.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

pub mod addon_harness;
mod area;
mod area_poi;
mod area_trigger;
#[cfg(feature = "dev")]
mod asset_churn;
mod aura_visual;
mod bindings;
mod blob_shadow;
mod bowstring;
mod camera_shake;
#[cfg(feature = "dev")]
mod capture;
mod chat_bubble;
mod chr_classes;
mod cinematic;
pub mod client_state;
mod combat_kit;
mod combat_log;
mod combat_mode;
mod combat_playback;
mod combat_profiles;
mod combat_text;
mod console;
pub mod crash;
mod creature_anim;
mod creature_type;
mod cursor;
mod cvars;
mod death;
#[cfg(feature = "dev")]
mod debug_panel;
/// The dev/player seam: the instrument groups and the boundary rule. Always compiled; what it holds
/// is not.
pub mod dev;
#[cfg(feature = "hotpatch")]
pub mod hotpatch;
/// Whole-client native patches into the running process; inert without `live_code`.
mod live_code;

#[cfg(feature = "dev")]
mod devtools;
mod doodad_events;
mod entities;
mod field_watch;
mod fishing_line;
mod footprints;
pub mod game_plugins;
mod glue_session;
mod go_anim;
mod go_templates;
#[cfg(feature = "dev")]
mod hover_log;
mod item_lane;
mod item_names;
mod item_watch;
mod items;
mod lanes;
mod loading_screen;
pub mod local_state;
mod minimap;
mod nameplates;
mod names;
mod net;
pub mod no_install;
mod npc_text;
/// Ships in part (the FPS journal and the clocks it reads); the rest is `dev`.
pub mod perf;
mod pipe_warm;
mod player;
mod player_commands;
mod poi_marker;
mod portrait;
#[cfg(feature = "dev")]
mod preflight;
#[cfg(feature = "dev")]
mod probe_shield;
mod query_cache;
mod quest_markers;
mod raid_marks;
mod ranged_flex;
mod realmlist;
pub mod run_mode;
mod screen_fade;
mod screenshot;
mod script_calls;
mod shaders;
mod wrath_equipment;
mod wrath_inventory;
mod wrath_item;

mod game_tip;
mod multiclass;
mod opaque2d;
mod quest_trace;
/// Where "the client is going down" is observed, in `Last`; every system that persists state on the
/// way out registers through it.
pub mod shutdown;
mod smart_rect;
/// Souls (Elden Ring–style) combat, hot-toggled with `F10`.
pub mod souls;
mod sound;
/// The two talent spell-modifier tables (`SMSG_SET_FLAT_/PCT_SPELL_MODIFIER`) and the read that
/// puts them on a number.
mod spell;
/// The melee swing refusal's latch + 4 s repeat (`SMSG_ATTACKSWING_*`).
mod swing_refusal;
mod target;
#[cfg(test)]
pub(crate) mod test_support;
mod text_filter;
mod textinput;
mod transport;
mod tutorial;
mod ui_action;
mod ui_auction;
mod ui_aura;
mod ui_bank;
mod ui_battlefield;
mod ui_battlefield_positions;
mod ui_battlefield_score;
mod ui_billing;
mod ui_bind_confirm;
mod ui_binder;
mod ui_cast;
mod ui_char;
mod ui_chat;
mod ui_craft;
mod ui_dialog_verbs;
mod ui_dressup;
mod ui_duel;
mod ui_follow;
mod ui_gamma;
mod ui_gm_ticket;
mod ui_gossip;
mod ui_guild;
mod ui_honor;
mod ui_inspect;
mod ui_instance;
mod ui_item_text;
mod ui_items;
mod ui_layout;
mod ui_logout;
mod ui_loot;
mod ui_loot_roll;
mod ui_macro;
mod ui_mail;
mod ui_memory;
mod ui_merchant;
mod ui_mirror;
mod ui_models;
mod ui_net;
mod ui_panes;
mod ui_party;
mod ui_pass;
mod ui_pet;
mod ui_pet_book;
mod ui_pet_stats;
mod ui_petition;
mod ui_quest;
mod ui_quest_log;
mod ui_quest_share;
mod ui_reputation;
mod ui_saved;
mod ui_script;
#[path = "ui_script/wrath_currency_feed.rs"]
mod ui_script_wrath_currency_feed;
mod ui_session;
mod ui_shapeshift;
mod ui_social;
mod ui_spellbook;
mod ui_stable;
mod ui_summon;
mod ui_tabard;
mod ui_talent;
mod ui_talent_wipe;
mod ui_taxi;
mod ui_text;
mod ui_tooltip;
mod ui_trade;
mod ui_tradeskill;
mod ui_trainer;
mod ui_unit;
mod ui_world_map;
pub mod video;
mod vm_pass;
mod vplates;
mod wdb;
mod weapon_trail;
mod world_backdrop;
mod world_console;
mod world_state;
mod world_state_ui;
mod wrath;
mod wrath_field_map;
mod wrath_native;
mod wrath_translate;

// A launcher stamps the build id at compile time and hands it to `benilla_client::run` or
// `run_with`; re-exported so the `benilla` shim needs no bevy dependency of its own.
pub use benilla_world::build_id::BuildId;
/// The world viewer's entry point, the engine with no game attached, called by the
/// `benilla-worldview` shim.
pub use benilla_world::worldview::run as run_worldview;
pub use bevy::app::AppExit;
// A crate on top of benilla reads objects and interacts through these, with no protocol dependency.
pub use benilla_protocol::{messages::ObjectType, ObjectFields};
pub use net::Objects;
pub use target::Interact;
// The explicit Wrath session the assembler's boot reads from the environment.
pub use wrath::WrathSessionConfig;
// Whether `$WOW_WRATH` selects the Wrath session; the Souls server link reads it.
pub use wrath::selected_from_env as wrath_selected_from_env;

/// What `benilla-souls` names in the core, by the module path it names it under. Only these items
/// are reachable from outside the crate: a whole `pub mod` of a hub (`net`, `ui_script`, …) makes
/// every `pub` item in it reachable, which the compiler must then keep, and the Linux CI runner ran
/// out of memory building the result. Regenerate from the crate's `crate::` paths when it changes.
#[doc(hidden)]
pub mod souls_seam {
    pub mod aura_visual {
        pub use crate::aura_visual::AnimRateFreeze;
    }
    pub mod bindings {
        pub use crate::bindings::{BindingsState, Input};
    }
    pub mod camera_shake {
        pub use crate::camera_shake::CameraShakes;
    }
    pub mod combat_kit {
        pub use crate::combat_kit::{
            resolve_dodge, resolve_press, roll_load, PlayerKit, Press, STAMINA_SCALE,
        };
    }
    pub mod combat_mode {
        pub use crate::combat_mode::{CombatMode, CombatModeView};
    }
    pub mod combat_playback {
        pub use crate::combat_playback::{
            kit_table_current, play_clip, play_starts, stock_press, submit_and_play, ActionPhase,
            CombatPlaybackState, Defense, PlaybackSet, PROFILE_TICK_RATE_HZ,
        };
    }
    pub mod combat_profiles {
        pub use crate::combat_profiles::CombatProfiles;
    }
    pub mod creature_anim {
        pub use crate::creature_anim::{
            apply_body_twist, move_flags, ActionAnim, AnimDriver, BodyTwist, CommittedSheath,
            Engaged, PlaySeq, VisualSheath, Wielded,
        };
    }
    pub mod entities {
        pub use crate::entities::{overhead_anchor, BoneAttach, HandKeyBones, OverheadFallback};
        pub mod mount {
            pub use crate::entities::mount::MountChild;
        }
    }
    pub mod names {
        pub use crate::names::NameCache;
    }
    pub mod net {
        pub use crate::net::{
            ChatKind, ClientCommand, Guid, GuidIndex, NetCommands, ObjectStore, RemoteMotion,
            SelfGuid, SelfPlayer,
        };
    }
    pub mod player {
        pub use crate::player::{Player, PlayerControlSet, SoulsDrive};
        pub mod bodies {
            pub use crate::player::bodies::{contact, BODY_HEIGHT_YARDS, GATHER_YARDS};
        }
        pub mod camera {
            pub use crate::player::camera::FlyCam;
        }
    }
    pub mod query_cache {
        pub use crate::query_cache::Ask;
    }
    pub mod target {
        pub use crate::target::{RelationWorld, SelectCommit, Selection};
        pub mod click {
            pub use crate::target::click::Interact;
        }
    }
    pub mod ui_script {
        pub use crate::ui_script::{UiInput, UiKeyboardCapture};
    }
    pub mod vplates {
        pub use crate::vplates::{gx_px, plate_basis, VPlates, PLATE_H, PLATE_LIFT};
    }
}

/// The state folder, `benilla-config/`, where a crate on top keeps its own files: an addon it ships
/// goes in its `AddOns/`, where benilla finds addons and their art and fonts. `None` when
/// persistence is off (a capture run, or no executable path), and it may not exist yet. A dev
/// build resolves it from the launcher's folder, which `benilla_client::run_with` sets, so ask from
/// `extend` or a system, never before.
pub fn config_dir() -> Option<std::path::PathBuf> {
    local_state::home()
}

#[cfg(test)]
mod economy {
    mod tests {
        use benilla_protocol::messages::TRADE_SLOT_COUNT;
        use benilla_protocol_wrath::gameplay::economy::{
            MailAttachment, MailEnchant, MailEntry, MailList, TradeExtended, TradeItem,
        };
        use benilla_ui::script::UiTarget;

        #[test]
        fn mail_list_reads_rows_bodies_and_attachments() {
            let empty_enchant = MailEnchant {
                id: 0,
                duration: 0,
                charges: 0,
            };
            let attachments: Vec<MailAttachment> = (0..12u8)
                .map(|idx| MailAttachment {
                    index: idx,
                    item_guid_low: 1000 + u32::from(idx),
                    item_entry: 40000 + u32::from(idx),
                    enchants: [
                        MailEnchant {
                            id: 3800 + u32::from(idx),
                            duration: 0,
                            charges: 0,
                        },
                        empty_enchant,
                        empty_enchant,
                        empty_enchant,
                        empty_enchant,
                        empty_enchant,
                        empty_enchant,
                    ],
                    random_prop_id: -12,
                    suffix_factor: 100,
                    count: u32::from(idx) + 1,
                    charges: 5,
                    max_durability: 120,
                    durability: 115,
                    unknown: 0,
                })
                .collect();
            let list = MailList {
                real_count: 1,
                mails: vec![MailEntry {
                    size: 0,
                    message_id: 77,
                    message_type: 0,
                    sender_guid: Some(0x1234),
                    sender_entry: None,
                    cod: 500,
                    package: 0,
                    stationery: 41,
                    money: 2500,
                    flags: 0x10,
                    expire_days: 29.5,
                    mail_template_id: 0,
                    subject: "Northrend Supplies".to_string(),
                    body: "Twelve attachments inside.".to_string(),
                    attachments,
                }],
            };

            let mapped = crate::wrath::wrath_mail_list(list);
            assert_eq!(mapped.len(), 1);
            let row = &mapped[0];
            assert_eq!(
                row.inline_body.as_deref(),
                Some("Twelve attachments inside.")
            );
            assert_eq!(row.attachments.len(), 12);
            assert_eq!(row.item.map(|a| a.entry), Some(40000));
            assert_eq!(row.attachments[11].entry, 40011);
            assert_eq!(row.attachments[11].perm_enchant, 3811);
            assert_eq!(row.attachments[11].count, 12);
        }

        #[test]
        fn wrath_trade_extended_preserves_trade_id_and_socket_enchants_in_item_links() {
            let mut slots = [None; TRADE_SLOT_COUNT];
            slots[0] = Some(TradeItem {
                entry: 49623,
                display_id: 65000,
                count: 1,
                wrapped: false,
                gift_creator: 0,
                perm_enchant: 3827,
                socket_enchant: [3520, 3521, 3532],
                creator: 0x99,
                charges: 0,
                suffix_factor: 0,
                random_prop_id: 0,
                lock_id: 0,
                max_durability: 120,
                durability: 120,
            });
            let ext = crate::wrath::wrath_trade_extended(TradeExtended {
                their_window: true,
                trade_id: 0xDEAD_BEEF,
                state_index: 4,
                sequence: 9,
                gold: 10_000,
                enchant_spell_id: 0,
                slots,
            });
            assert_eq!(ext.trade_id, 0xDEAD_BEEF);
            let slot0 = ext.slots[0].expect("slot 0 populated");
            assert_eq!(slot0.socket_enchant, [3520, 3521, 3532]);

            let mut session = crate::ui_trade::TradeSession::default();
            session.set_offer(&ext);
            assert_eq!(session.trade_id, 0xDEAD_BEEF);

            let link = crate::ui_items::item_link_full_with_gems_in_target(
                UiTarget::Wrath335,
                slot0.entry,
                slot0.perm_enchant,
                slot0.socket_enchant,
                slot0.random_prop_id,
                slot0.suffix_factor,
                "Shadowmourne",
                5,
            );
            assert_eq!(
                link,
                "|cffff8000|Hitem:49623:3827:3520:3521:3532:0:0:0:0|h[Shadowmourne]|h|r"
            );
        }
    }
}

#[cfg(test)]
mod social {
    mod tests {
        use benilla_protocol::messages::GROUP_MEMBER_ASSISTANT;
        use benilla_protocol::SessionEvent;
        use benilla_protocol_wrath::gameplay::social::{
            GroupList, GroupLoot, GroupMember, PartyAura, PartyAuras, PartyMemberFields,
            PartyMemberStats,
        };

        #[test]
        fn party_member_stats_full_reads_every_defined_bit() {
            let stats = PartyMemberStats {
                guid: 0x42,
                full: true,
                arena: Some(0),
                fields: PartyMemberFields {
                    mask: 0x000E_6607,
                    status: Some(0x0001),
                    cur_hp: Some(125_400),
                    max_hp: Some(140_000),
                    auras: Some(PartyAuras {
                        mask: (1u64 << 0) | (1u64 << 5) | (1u64 << 55),
                        auras: vec![
                            PartyAura {
                                slot: 0,
                                spell_id: 72_968,
                                flags: 0x10, // AFLAG_POSITIVE
                            },
                            PartyAura {
                                slot: 5,
                                spell_id: 70_842,
                                flags: 0x20, // AFLAG_NEGATIVE in low slot
                            },
                            PartyAura {
                                slot: 55,
                                spell_id: 69_762,
                                flags: 0x20, // AFLAG_NEGATIVE in high slot >= 48
                            },
                        ],
                    }),
                    pet_guid: Some(0xF140_0000_0000_0099),
                    pet_cur_hp: Some(88_500),
                    pet_max_hp: Some(95_000),
                    pet_auras: Some(PartyAuras {
                        mask: 1u64 << 40,
                        auras: vec![PartyAura {
                            slot: 40,
                            spell_id: 63_848,
                            flags: 0x10, // AFLAG_POSITIVE
                        }],
                    }),
                    vehicle_seat: Some(3012),
                    ..Default::default()
                },
            };

            let SessionEvent::PartyMemberStats { guid, full, info } =
                crate::wrath::wrath_party_stats(stats)
            else {
                panic!("expected PartyMemberStats event");
            };
            assert_eq!(guid, 0x42);
            assert!(full);
            assert_eq!(info.cur_hp, Some(125_400));
            assert_eq!(info.max_hp, Some(140_000));
            assert_eq!(info.pet_cur_hp, Some(88_500));
            assert_eq!(info.pet_max_hp, Some(95_000));
            assert_eq!(info.vehicle_seat, Some(3012));
            assert_eq!(info.auras.as_deref(), Some(&[(0, 72_968)][..]));
            assert_eq!(
                info.auras_negative.as_deref(),
                Some(&[(5, 70_842), (55, 69_762)][..])
            );
            assert_eq!(info.pet_auras.as_deref(), Some(&[(40, 63_848)][..]));

            let mut group = crate::ui_party::GroupState::default();
            group.apply_stats(guid, true, *info);
            let stored = group.stats.get(&guid).expect("member record stored");
            assert_eq!(stored.cur_hp, Some(125_400));
            assert_eq!(stored.pet_max_hp, Some(95_000));
            assert_eq!(stored.vehicle_seat, Some(3012));
        }

        #[test]
        fn wrath_group_list_packs_subgroup_and_assistant_flags_for_self_and_members() {
            let list = GroupList {
                group_type: 0x02,
                sub_group: 3,
                flags: 0x01, // MEMBER_FLAG_ASSISTANT
                roles: 0x02,
                lfg: None,
                group_guid: 0x1F50_0000_0000_0001,
                counter: 7,
                members: vec![
                    GroupMember {
                        name: "Tirion".to_string(),
                        guid: 0x10,
                        status: 0x01,
                        sub_group: 2,
                        flags: 0x01, // MEMBER_FLAG_ASSISTANT
                        roles: 0x02,
                    },
                    GroupMember {
                        name: "Darion".to_string(),
                        guid: 0x11,
                        status: 0x01,
                        sub_group: 4,
                        flags: 0x02, // MEMBER_FLAG_MAINTANK (not assistant)
                        roles: 0x08,
                    },
                ],
                leader_guid: 0x10,
                loot: Some(GroupLoot {
                    method: 2,
                    master: 0x10,
                    threshold: 4,
                    dungeon_difficulty: 1,
                    raid_difficulty: 2,
                    dynamic_difficulty: 0,
                }),
            };

            let events = crate::wrath::wrath_group_list_events(list);
            assert_eq!(events.len(), 3);
            let SessionEvent::GroupList {
                group_type,
                own_flags,
                members,
                leader,
                loot,
            } = &events[0]
            else {
                panic!("expected GroupList event");
            };
            assert_eq!(*group_type, 0x02);
            assert_eq!(*own_flags, 3 | GROUP_MEMBER_ASSISTANT);
            assert_eq!(members.len(), 2);
            assert_eq!(members[0].flags, 2 | GROUP_MEMBER_ASSISTANT);
            assert_eq!(members[1].flags, 4);
            assert_eq!(*leader, 0x10);
            assert_eq!(loot.map(|l| (l.method, l.threshold)), Some((2, 4)));

            let mut group = crate::ui_party::GroupState::default();
            for ev in events.into_iter().skip(1) {
                let SessionEvent::PartyMemberStats { guid, full, info } = ev else {
                    panic!("expected PartyMemberStats role delta");
                };
                group.apply_stats(guid, full, *info);
            }
            assert_eq!(group.stats.get(&0x10).and_then(|s| s.lfg_roles), Some(0x02));
            assert_eq!(group.stats.get(&0x11).and_then(|s| s.lfg_roles), Some(0x08));
        }

        #[test]
        fn wrath_pet_order_reaches_handle_pet_action() {
            let pet_guid = 0xF140_0000_0000_0077u64;
            let packed = (0x07u32 << 24) | 0x02u32; // ACT_COMMAND (0x07), COMMAND_ATTACK (2)
            let target_guid = 0xF130_0000_2F00_002Au64;
            let wire =
                benilla_protocol_wrath::gameplay::encode_pet_action(pet_guid, packed, target_guid);
            assert_eq!(wire.len(), 20);
            assert_eq!(u64::from_le_bytes(wire[0..8].try_into().unwrap()), pet_guid);
            assert_eq!(u32::from_le_bytes(wire[8..12].try_into().unwrap()), packed);
            assert_eq!(
                u64::from_le_bytes(wire[12..20].try_into().unwrap()),
                target_guid
            );
        }
    }
}
