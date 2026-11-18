//! `LearnTalent` and `ConfirmTalentWipe` through the whole client's host: each sends inside its
//! call (`0x4f37e0`, `0x5dfad6`), so a chunk's packets leave in call order with every other verb's.

use benilla_protocol::ObjectFields;
use benilla_ui::script::UiScript;
use crossbeam_channel::Receiver;

use super::*;
use crate::net::{Guid, GuidIndex};
use crate::script_calls::with_hosted_vm;

const ME: u64 = 0x10;
const TRAINER: u64 = 0xF130_0000_0000_0042;
/// `PLAYER_FIELD_COINAGE`.
const COINAGE: u16 = 1176;

/// The whole client, headless, the player holding `money` copper and a trainer's respec question
/// latched at 1000, its VM seated with one tab: talent 100 at rank 2 of 5, talent 101 at 5 of 5.
fn client(money: u32) -> (App, Receiver<ClientCommand>) {
    let mut app = crate::game_plugins::schedule_tests::headless_client_unrun();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    let world = app.world_mut();
    let me = world
        .spawn((
            SelfPlayer,
            Guid(ME),
            ObjectStore(ObjectFields::from_pairs(&[(2, 0x19), (COINAGE, money)])),
        ))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(ME, me);
    world.insert_resource(crate::net::SelfGuid(Some(ME)));
    world
        .resource_mut::<crate::ui_talent_wipe::TalentWipeState>()
        .ask(TRAINER, 1000);
    let mut script = UiScript::new().expect("a VM");
    script.set_talents(TalentUiState {
        tabs: vec![TalentTabView {
            name: "Arms".into(),
            ..Default::default()
        }],
        talents: vec![vec![
            TalentView {
                talent_id: 100,
                rank: 2,
                max_rank: 5,
                ..Default::default()
            },
            TalentView {
                talent_id: 101,
                rank: 5,
                max_rank: 5,
                ..Default::default()
            },
        ]],
        points: (1, 0),
    });
    world.insert_non_send_resource(script);
    (app, rx)
}

fn run(app: &mut App, lua: &str) {
    with_hosted_vm(app.world_mut(), |vm| vm.expect("a VM").run(lua)).expect("the chunk runs");
}

fn sent(rx: &Receiver<ClientCommand>) -> Vec<String> {
    rx.try_iter()
        .map(|c| format!("{c:?}"))
        .filter(|c| !c.contains("Query"))
        .collect()
}

fn roll(min: u32, max: u32) -> String {
    format!("{:?}", ClientCommand::RandomRoll { min, max })
}

/// `LearnTalent` sends the talent id and its rank count (`0x4f37c7`, `0x4f37d1`) inside the call,
/// and nothing for a talent at its maximum (`0x4f378f`) or past the tab.
#[test]
fn learn_talent_sends_inside_its_call() {
    let (mut app, rx) = client(5000);
    let _ = sent(&rx);
    run(
        &mut app,
        "LearnTalent(1, 1) RandomRoll(1, 2) LearnTalent(1, 2) LearnTalent(1, 9) RandomRoll(3, 4)",
    );
    assert_eq!(
        sent(&rx),
        [
            format!(
                "{:?}",
                ClientCommand::LearnTalent {
                    talent_id: 100,
                    rank: 2
                }
            ),
            roll(1, 2),
            roll(3, 4),
        ]
    );
}

/// `LearnPreviewTalents(pet)` sends the previewed pairs inside its call (`0x5c6a10`): the
/// player packet is `count` + pairs, the pet packet prefixes the summoned pet's guid
/// (`PetHandler.cpp:1119`). The fixture also sets `UNIT_FIELD_CHARM`, whose guid must not win:
/// `Player::LearnPetTalent` compares against `GetPet()` (`Player.cpp:14120`).
#[test]
fn learn_preview_talents_sends_the_counted_pairs_inside_its_call() {
    const PET: u64 = 0xF140_0000_0000_0021;
    const CHARM: u64 = 0xF130_0000_0000_0032;
    let (mut app, rx) = client(5000);
    let _ = sent(&rx);

    let mut script = UiScript::with_target(benilla_ui::script::UiTarget::Wrath335).expect("a VM");
    script.set_wrath_talents(WrathTalentUiState {
        active_group: 1,
        groups: vec![WrathTalentGroupView {
            tabs: vec![TalentTabView {
                name: "Beast Mastery".into(),
                ..Default::default()
            }],
            talents: vec![vec![TalentView {
                talent_id: 200,
                rank: 1,
                max_rank: 3,
                meets_prereq: true,
                ..Default::default()
            }]],
            unspent_points: 5,
            glyphs: [0; 6],
        }],
        pet_talent_views: vec![vec![TalentView {
            talent_id: 201,
            rank: 0,
            max_rank: 2,
            meets_prereq: true,
            ..Default::default()
        }]],
        ..Default::default()
    });
    let world = app.world_mut();
    let me = world
        .resource::<GuidIndex>()
        .0
        .get(&ME)
        .copied()
        .expect("the player");
    *world
        .get_mut::<ObjectStore>(me)
        .expect("the player's store") = ObjectStore(ObjectFields::from_pairs(&[
        (2, 0x19),
        (COINAGE, 5000),
        (6, CHARM as u32),
        (7, (CHARM >> 32) as u32),
        (8, PET as u32),
        (9, (PET >> 32) as u32),
    ]));
    world.insert_non_send_resource(script);

    run(
        &mut app,
        r#"AddPreviewTalentPoints(1, 1, 2) LearnPreviewTalents(false) RandomRoll(1, 2)
        AddPreviewTalentPoints(1, 1, 2, true) LearnPreviewTalents(true) RandomRoll(3, 4)"#,
    );
    assert_eq!(
        sent(&rx),
        [
            format!(
                "{:?}",
                ClientCommand::LearnPreviewTalents {
                    pet: false,
                    pet_guid: 0,
                    entries: vec![(200, 3)],
                }
            ),
            roll(1, 2),
            format!(
                "{:?}",
                ClientCommand::LearnPreviewTalents {
                    pet: true,
                    pet_guid: PET,
                    entries: vec![(201, 2)],
                }
            ),
            roll(3, 4),
        ]
    );
}

/// The Accept sends the latched trainer's guid inside its call (`0x5dfad6`); over budget it shows
/// `ERR_NOT_ENOUGH_MONEY` there instead (`0x5dfa81`).
#[test]
fn confirm_talent_wipe_answers_inside_its_call() {
    let (mut app, rx) = client(5000);
    let _ = sent(&rx);
    run(&mut app, "ConfirmTalentWipe() RandomRoll(1, 2)");
    assert_eq!(
        sent(&rx),
        [
            format!(
                "{:?}",
                ClientCommand::TalentWipeConfirm { trainer: TRAINER }
            ),
            roll(1, 2),
        ]
    );

    let (mut app, rx) = client(10);
    let _ = sent(&rx);
    run(
        &mut app,
        r#"ERR_NOT_ENOUGH_MONEY = "You don't have enough money."
        local f = CreateFrame("Frame") f:RegisterEvent("UI_ERROR_MESSAGE")
        f:SetScript("OnEvent", function() Refused = arg1 end)
        ConfirmTalentWipe() InCall = Refused RandomRoll(1, 2)"#,
    );
    assert_eq!(sent(&rx), [roll(1, 2)]);
    let in_call = app
        .world_mut()
        .non_send_resource_mut::<UiScript>()
        .eval::<String>("return InCall")
        .expect("the refusal was heard inside the call");
    assert_eq!(in_call, "You don't have enough money.");
}
