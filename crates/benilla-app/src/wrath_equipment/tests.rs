//! The equipment manager through its natives, with a narrow host over a bare world (the
//! `ui_loot` `LootHost` pattern): the set verbs apply through [`EquipmentSteps`] and the getters'
//! item query reads the same inventory. Synthetic guids and fields only.

use super::*;
use benilla_protocol::ObjectFields;
use benilla_protocol_wrath::fields::{
    PLAYER_END, PLAYER_FIELD_INV_SLOT_HEAD, PLAYER_FIELD_PACK_SLOT_1,
};
use benilla_ui::script::{CallHost, CallTail, ScriptCall, UiTarget};
use bevy::ecs::system::SystemState;
use crossbeam_channel::Receiver;

use crate::wrath_inventory::WrathFieldWindow;

const ME: u64 = 0x10;
/// Worn in the head slot (0).
const HELM: u64 = 0x4000_0000_0000_0101;
/// In backpack slot 3, player-array index 26.
const SWORD: u64 = 0x4000_0000_0000_0102;
/// Named by a set, held nowhere.
const LOST: u64 = 0x4000_0000_0000_0103;
const HELM_ENTRY: u32 = 1001;
const SWORD_ENTRY: u32 = 1002;
/// `UNIT_FIELD_FLAGS` in the donor projection the player's store carries.
const FLAGS: u16 = 46;

/// The set verbs and the item query over a bare world, as the engine's host applies them.
struct EquipHost<'a>(&'a mut World);

impl CallHost for EquipHost<'_> {
    fn apply(&mut self, script: &mut UiScript, call: ScriptCall) -> Option<CallTail> {
        let ScriptCall::EquipmentSet(call) = call else {
            return None;
        };
        let mut state = SystemState::<EquipmentSteps>::new(self.0);
        state.get_mut(self.0).apply(script, call);
        state.apply(self.0);
        None
    }

    fn equipment_item(&mut self, guid: u64) -> Option<EquipmentItemView> {
        let mut state = SystemState::<EquipmentSteps>::new(self.0);
        state.get_mut(self.0).item(guid, true)
    }
}

struct Harness {
    world: World,
    rx: Receiver<ClientCommand>,
    script: UiScript,
}

impl Harness {
    fn new(player_flags: u32) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut world = World::new();
        world.insert_resource(NetCommands(tx));
        world.init_resource::<EquipmentManager>();
        let mut values = vec![0u32; usize::from(PLAYER_END)];
        let mut put = |at: u16, guid: u64| {
            values[usize::from(at)] = guid as u32;
            values[usize::from(at) + 1] = (guid >> 32) as u32;
        };
        put(PLAYER_FIELD_INV_SLOT_HEAD, HELM);
        put(PLAYER_FIELD_PACK_SLOT_1 + 2 * 3, SWORD);
        let mut inventory = WrathInventory::default();
        inventory.apply(&WrathFieldWindow::from_dense(values), (1, 1));
        world.insert_resource(inventory);
        world.spawn((
            SelfPlayer,
            Guid(ME),
            ObjectStore(ObjectFields::from_pairs(&[(FLAGS, player_flags)])),
        ));
        let mut index = GuidIndex::default();
        for (guid, entry) in [(HELM, HELM_ENTRY), (SWORD, SWORD_ENTRY)] {
            let e = world
                .spawn(ObjectStore(ObjectFields::from_pairs(&[(3, entry)])))
                .id();
            index.0.insert(guid, e);
        }
        world.insert_resource(index);
        let mut script = UiScript::with_target(UiTarget::Wrath335).expect("a Wrath VM");
        script.seat_player_guid(ME);
        script.register_cvars([("equipmentManager", "1")]);
        script.set_cvar_host("equipmentManager", "1");
        script.set_macro_icons(vec!["Ability_Ambush".into(), "INV_Helmet_01".into()]);
        script
            .run(
                r#"SEEN = {}
                local f = CreateFrame("Frame")
                for _, e in ipairs({"EQUIPMENT_SETS_CHANGED", "EQUIPMENT_SWAP_PENDING",
                        "UI_ERROR_MESSAGE"}) do f:RegisterEvent(e) end
                f:SetScript("OnEvent", function(_, event, a1)
                    table.insert(SEEN, a1 and (event .. ":" .. tostring(a1)) or event)
                end)"#,
            )
            .expect("the listener loads");
        Self { world, rx, script }
    }

    /// Adopt `packet` and copy the owner into the VM, as the drain and the feed do.
    fn packet(&mut self, packet: EquipmentSetPacket) {
        let mut manager = self.world.resource_mut::<EquipmentManager>();
        manager.apply_packet(packet);
        let state = manager.state();
        self.script.set_equipment_sets(state);
    }

    fn run<R>(&mut self, f: impl FnOnce(&mut UiScript) -> R) -> R {
        self.script.hosted(&mut EquipHost(&mut self.world), f)
    }

    fn eval<T: benilla_ui::mlua::FromLuaMulti>(&mut self, lua: &str) -> T {
        self.run(|s| s.eval::<T>(lua)).expect("the chunk runs")
    }

    fn seen(&mut self) -> String {
        self.eval::<String>("local s = table.concat(SEEN, ' ') SEEN = {} return s")
    }

    fn sent(&self) -> Vec<ClientCommand> {
        self.rx.try_iter().collect()
    }
}

fn set(guid: u64, id: u32, name: &str, items: [(usize, u64); 4]) -> EquipmentSet {
    let mut slots = [0u64; EQUIPMENT_SET_SLOTS];
    for (slot, guid) in items {
        slots[slot] = guid;
    }
    EquipmentSet {
        guid,
        id,
        name: name.into(),
        icon: "INV_Chest_Plate01".into(),
        items: slots,
    }
}

fn listed() -> Harness {
    let mut h = Harness::new(0);
    h.packet(EquipmentSetPacket::List(vec![
        set(
            0x70,
            3,
            "Tank",
            [(0, HELM), (1, IGNORED_SLOT_GUID), (4, SWORD), (5, LOST)],
        ),
        set(0x71, 0, "Heal", [(0, HELM), (1, 0), (2, 0), (3, 0)]),
    ]));
    h
}

/// The login list feeds the getters: the count, the indexed and named info with and without the
/// icon directory, and each slot's item entry (0 empty, 1 ignored, -1 not held).
#[test]
fn equipment_list_feeds_the_getters() {
    let mut h = listed();
    assert_eq!(h.eval::<i64>("return GetNumEquipmentSets()"), 2);
    assert_eq!(
        h.eval::<(String, String, i64)>("return GetEquipmentSetInfo(1)"),
        (
            "Tank".into(),
            "Interface\\Icons\\INV_Chest_Plate01".into(),
            3
        )
    );
    assert_eq!(
        h.eval::<(String, i64)>(r#"return GetEquipmentSetInfoByName("Heal")"#),
        ("INV_Chest_Plate01".into(), 0)
    );
    assert_eq!(
        h.eval::<i64>("return select('#', GetEquipmentSetInfo(3))"),
        0
    );
    assert!(h.run(|s| s.run("GetEquipmentSetInfo(11)")).is_err());
    assert_eq!(
        h.eval::<String>(
            r#"local t = GetEquipmentSetItemIDs("Tank")
            return t[1] .. " " .. t[2] .. " " .. t[3] .. " " .. t[5] .. " " .. t[6]"#
        ),
        format!("{HELM_ENTRY} 1 0 {SWORD_ENTRY} -1")
    );
    // A list of the same count after the load is skipped whole (`0x5AF490`).
    h.packet(EquipmentSetPacket::List(vec![
        set(1, 0, "A", [(0, 0); 4]),
        set(2, 1, "B", [(0, 0); 4]),
    ]));
    assert_eq!(h.eval::<String>("return (GetEquipmentSetInfo(1))"), "Tank");
}

/// `GetEquipmentSetLocations`: a worn item packs its slot, a backpack item its bag slot; an
/// ignored slot answers 1 and an item held nowhere -1.
#[test]
fn equipment_set_locations_pack_where_each_item_is() {
    let mut h = listed();
    assert_eq!(
        h.eval::<String>(
            r#"local t = GetEquipmentSetLocations("Tank")
            return string.format("%x %d %d %x %d", t[1], t[2], t[3], t[5], t[6])"#
        ),
        "100001 1 0 300004 -1"
    );
}

/// `SaveEquipmentSet` on a new name takes the lowest free id, records the worn items and the
/// ignored marks, sends the save with no guid, and signals; a save before the id arrives does
/// nothing, and the id's arrival lets the next save carry the guid.
#[test]
fn equipment_set_save_creates_then_waits_for_its_id() {
    let mut h = listed();
    h.eval::<()>(r#"EquipmentManagerIgnoreSlotForSave(2) SaveEquipmentSet("Pvp", 2)"#);
    let sent = h.sent();
    let [ClientCommand::EquipmentSetSave(saved)] = sent.as_slice() else {
        panic!("one save: {sent:?}");
    };
    assert_eq!((saved.guid, saved.id), (0, 1), "id 0 and 3 are taken");
    assert_eq!(saved.icon, "INV_Helmet_01");
    assert_eq!(
        (saved.items[0], saved.items[1], saved.items[4]),
        (HELM, 1, 0)
    );
    assert_eq!(h.seen(), "EQUIPMENT_SETS_CHANGED");
    assert_eq!(h.eval::<i64>("return GetNumEquipmentSets()"), 3);
    h.eval::<()>(r#"SaveEquipmentSet("Pvp", 1)"#);
    assert!(h.sent().is_empty(), "the set awaits its id");
    h.packet(EquipmentSetPacket::Saved(EquipmentSetSaved {
        id: 1,
        guid: 0x99,
    }));
    h.eval::<()>(r#"SaveEquipmentSet("Pvp", 1)"#);
    let sent = h.sent();
    assert!(
        matches!(sent.as_slice(), [ClientCommand::EquipmentSetSave(s)] if s.guid == 0x99 && s.icon == "Ability_Ambush")
    );
}

/// An eleventh set raises; a refused id (guid 0) drops the set.
#[test]
fn equipment_set_limits_and_refusals() {
    let mut h = Harness::new(0);
    h.packet(EquipmentSetPacket::List(
        (0..10)
            .map(|i| set(0x100 + u64::from(i), i, &format!("S{i}"), [(0, 0); 4]))
            .collect(),
    ));
    let err = h
        .run(|s| s.run(r#"SaveEquipmentSet("New", 1)"#))
        .unwrap_err();
    assert!(err.to_string().contains("Too many sets"), "{err}");
    h.packet(EquipmentSetPacket::Saved(EquipmentSetSaved {
        id: 4,
        guid: 0,
    }));
    assert_eq!(h.eval::<i64>("return GetNumEquipmentSets()"), 9);
}

/// `DeleteEquipmentSet` sends the set's guid and signals; an unknown name does nothing.
#[test]
fn equipment_set_delete_sends_its_guid() {
    let mut h = listed();
    h.eval::<()>(r#"DeleteEquipmentSet("Nobody") DeleteEquipmentSet("Heal")"#);
    assert!(matches!(
        h.sent().as_slice(),
        [ClientCommand::EquipmentSetDelete { set_guid: 0x71 }]
    ));
    assert_eq!(h.seen(), "EQUIPMENT_SETS_CHANGED");
    assert_eq!(h.eval::<i64>("return GetNumEquipmentSets()"), 1);
}

/// `UseEquipmentSet` sends each slot's item and place, warns once for an item held nowhere,
/// marks the swap pending and signals it; while it is pending the next use is silently refused.
#[test]
fn equipment_set_use_sends_each_slot_and_pends() {
    let mut h = listed();
    assert!(h.eval::<bool>(r#"return UseEquipmentSet("Tank") == true"#));
    let sent = h.sent();
    let [ClientCommand::EquipmentSetUse(slots)] = sent.as_slice() else {
        panic!("one use: {sent:?}");
    };
    let row = |i: usize| (slots[i].item, slots[i].bag, slots[i].slot);
    assert_eq!(row(0), (HELM, 0xFF, 0));
    assert_eq!(row(1), (1, 0, 0), "ignored");
    assert_eq!(row(2), (0, 0, 0), "emptied");
    assert_eq!(row(4), (SWORD, 0xFF, 26), "backpack slot 3");
    assert_eq!(row(5), (1, 0, 0), "held nowhere");
    let seen = h.seen();
    assert!(seen.starts_with("UI_ERROR_MESSAGE:"), "{seen}");
    assert!(seen.ends_with("EQUIPMENT_SWAP_PENDING"), "{seen}");
    assert_eq!(h.eval::<i64>("return CanUseEquipmentSets()"), 0);
    assert!(h.eval::<bool>(r#"return UseEquipmentSet("Tank") == nil"#));
    assert!(h.sent().is_empty());
    assert_eq!(h.seen(), "", "a pending swap refuses silently");
}

/// In combat only the weapon slots move: every other slot is sent ignored, with one warning.
#[test]
fn equipment_set_use_in_combat_moves_only_weapons() {
    let mut h = Harness::new(UNIT_FLAG_IN_COMBAT);
    h.packet(EquipmentSetPacket::List(vec![set(
        0x70,
        0,
        "Arms",
        [(0, SWORD), (15, SWORD), (1, 0), (2, 0)],
    )]));
    h.eval::<()>(r#"UseEquipmentSet("Arms")"#);
    let sent = h.sent();
    let [ClientCommand::EquipmentSetUse(slots)] = sent.as_slice() else {
        panic!("one use: {sent:?}");
    };
    assert_eq!(slots[0].item, 1, "the head waits");
    assert_eq!((slots[15].item, slots[15].slot), (SWORD, 26));
    assert_eq!(h.seen().matches("UI_ERROR_MESSAGE").count(), 1);
}

/// The use result ends the swap: success or failure with the pending set's name, and the
/// bags-full line for result 4.
#[test]
fn equipment_set_use_result_finishes_the_swap() {
    let mut h = listed();
    h.eval::<()>(r#"UseEquipmentSet("Tank")"#);
    h.packet(EquipmentSetPacket::UseResult(4));
    let manager = h.world.resource::<EquipmentManager>();
    assert!(!manager.swap_pending);
    assert_eq!(
        manager.signals,
        [
            Signal::SwapFinished {
                ok: false,
                name: Some("Tank".into())
            },
            Signal::BagsFull
        ]
    );
    assert_eq!(h.eval::<i64>("return CanUseEquipmentSets()"), 1);
}

/// The CVar gates the swap: off, the use is refused with the disabled line.
#[test]
fn equipment_set_use_honours_the_cvar() {
    let mut h = listed();
    h.script.set_cvar_host("equipmentManager", "0");
    assert_eq!(h.eval::<i64>("return CanUseEquipmentSets()"), 0);
    assert!(h.eval::<bool>(r#"return UseEquipmentSet("Tank") == nil"#));
    assert!(h.sent().is_empty());
    assert!(h.seen().starts_with("UI_ERROR_MESSAGE:"));
}

/// A set with a locked item reports it; the lock gone, nothing.
#[test]
fn equipment_set_contains_locked_items_reads_the_client_lock() {
    let mut h = listed();
    assert_eq!(
        h.eval::<i64>(r#"return select('#', EquipmentSetContainsLockedItems("Tank"))"#),
        0
    );
    h.script.lock_item(SWORD);
    assert!(h.eval::<bool>(r#"return EquipmentSetContainsLockedItems("Tank") == true"#));
}

// ── The action bar's set buttons (12340 `ActionBarFrame.cpp`) ──

/// What one bar run left: the slot writes sent (`CMSG_SET_ACTION_BUTTON`, Lua id and word) and the
/// `UseAction` presses that reached the host.
#[derive(Default)]
struct BarRun {
    sets: Vec<(u32, u32)>,
    uses: usize,
}

/// The tag `PutActionInSlot` (`0x5AB120`) packs a set under, AzerothCore's `ACTION_BUTTON_EQSET`.
const SET_WORD: u32 = 0x2000_0000;

impl Harness {
    /// Listen to the bar's and the cursor's events in `BAR`; a slot signal records the text its
    /// handler reads.
    fn listen_bar(&mut self) {
        self.script
            .run(
                r#"BAR = {}
                local f = CreateFrame("Frame")
                for _, e in ipairs({"ACTIONBAR_SLOT_CHANGED", "WEAR_EQUIPMENT_SET",
                        "ACTIONBAR_SHOWGRID", "ACTIONBAR_HIDEGRID", "CURSOR_UPDATE",
                        "EQUIPMENT_SETS_CHANGED"}) do f:RegisterEvent(e) end
                f:SetScript("OnEvent", function(_, event, a1)
                    if event == "ACTIONBAR_SLOT_CHANGED" then
                        table.insert(BAR, event .. ":" .. a1 .. "=" .. tostring(GetActionText(a1)))
                    elseif a1 then
                        table.insert(BAR, event .. ":" .. tostring(a1))
                    else
                        table.insert(BAR, event)
                    end
                end)"#,
            )
            .expect("the bar listener loads");
    }

    /// Run `lua` with a narrow host: the set verbs through [`EquipmentSteps`], the slot writes and
    /// presses recorded.
    fn bar(&mut self, lua: &str) -> BarRun {
        let world = &mut self.world;
        let mut run = BarRun::default();
        let mut host =
            crate::script_calls::FnHost(|script: &mut UiScript, call: ScriptCall| match call {
                ScriptCall::EquipmentSet(call) => {
                    let mut state = SystemState::<EquipmentSteps>::new(world);
                    state.get_mut(world).apply(script, call);
                    state.apply(world);
                }
                ScriptCall::ActionBar(benilla_ui::script::ActionBarCall::Set { id, packed }) => {
                    run.sets.push((id, packed));
                }
                ScriptCall::UseAction(_) => run.uses += 1,
                _ => {}
            });
        self.script
            .hosted(&mut host, |s| s.run(lua))
            .expect("the chunk runs");
        drop(host);
        run
    }

    fn bar_seen(&mut self) -> String {
        self.eval::<String>("local s = table.concat(BAR, ' ') BAR = {} return s")
    }
}

/// `PickupEquipmentSet` and `PickupEquipmentSetByName` put a listed set on the cursor through
/// `SetCursorEquipmentSet` (`0x520DC0`): its clear, then the grid; `GetCursorInfo` answers
/// `"equipmentset"` and the name (`GameUI_Scripts.cpp:2921`). An unknown name or a free row leaves
/// the cursor as it was.
#[test]
fn a_set_picked_up_goes_on_the_cursor() {
    let mut h = listed();
    h.listen_bar();
    h.bar("PickupEquipmentSet(1)");
    assert_eq!(h.bar_seen(), "CURSOR_UPDATE ACTIONBAR_SHOWGRID");
    assert_eq!(
        h.eval::<(String, String)>("return GetCursorInfo()"),
        ("equipmentset".into(), "Tank".into())
    );
    assert!(matches!(
        h.script.cursor_payload(),
        Some(benilla_ui::script::CursorPayload::EquipmentSet(s))
            if s.id == 3 && s.texture.as_deref() == Some("Interface\\Icons\\INV_Chest_Plate01")
    ));
    h.bar(r#"PickupEquipmentSetByName("Heal") PickupEquipmentSetByName("Nope") PickupEquipmentSet(5)"#);
    assert_eq!(
        h.eval::<String>("return select(2, GetCursorInfo())"),
        "Heal"
    );
    h.bar("ClearCursor()");
    assert_eq!(h.eval::<i64>("return select('#', GetCursorInfo())"), 0);
}

/// `PutActionInSlot` stores `cursorSet | 0x20000000` and sends it; the bar's queries read the set
/// live: `GetActionInfo` and `GetActionText` its name, `GetActionTexture` its icon path, and
/// `IsEquippedAction` nil (no item, `0x5A88B0`).
#[test]
fn a_set_placed_on_the_bar_stores_the_set_word() {
    let mut h = listed();
    h.listen_bar();
    h.bar("PickupEquipmentSet(1)");
    h.bar_seen();
    let run = h.bar("PlaceAction(7)");
    assert_eq!(run.sets, [(7, SET_WORD | 3)]);
    assert_eq!(
        h.bar_seen(),
        "ACTIONBAR_HIDEGRID CURSOR_UPDATE ACTIONBAR_SLOT_CHANGED:7=Tank"
    );
    assert!(h.script.cursor_payload().is_none());
    assert_eq!(
        h.eval::<(String, String)>("return GetActionInfo(7)"),
        ("equipmentset".into(), "Tank".into())
    );
    assert_eq!(
        h.eval::<String>("return GetActionTexture(7)"),
        "Interface\\Icons\\INV_Chest_Plate01"
    );
    assert!(h.eval::<bool>("return HasAction(7) == 1 and IsEquippedAction(7) == nil"));
    // The same set held over its own slot clears the cursor and writes nothing.
    let run = h.bar("PickupEquipmentSet(1) PlaceAction(7)");
    assert!(run.sets.is_empty());
    assert!(h.script.cursor_payload().is_none());
}

/// `UseAction` on a set slot is `EquipSet` (`0x5AE660`): `WEAR_EQUIPMENT_SET` with the set's
/// name, no press for the host and no swap sent.
#[test]
fn a_set_slot_used_signals_wear_equipment_set() {
    let mut h = listed();
    h.listen_bar();
    h.bar("PickupEquipmentSet(1) PlaceAction(7)");
    h.bar_seen();
    let run = h.bar("UseAction(7)");
    assert_eq!(run.uses, 0);
    assert_eq!(h.bar_seen(), "WEAR_EQUIPMENT_SET:Tank");
    assert!(h
        .sent()
        .iter()
        .all(|c| !matches!(c, ClientCommand::EquipmentSetUse(_))));
}

/// `PickupAction` lifts a set slot back onto the cursor and clears the slot on the wire
/// (`0x5ABE70`).
#[test]
fn a_set_slot_picked_up_lifts_the_set() {
    let mut h = listed();
    h.bar("PickupEquipmentSet(2) PlaceAction(9)");
    let run = h.bar("PickupAction(9)");
    assert_eq!(run.sets, [(9, 0)]);
    assert_eq!(
        h.eval::<String>("return select(2, GetCursorInfo())"),
        "Heal"
    );
    assert!(h.eval::<bool>("return HasAction(9) == nil"));
}

/// A rename saves the numbered set, whose slots signal with the new name before the list's
/// `EQUIPMENT_SETS_CHANGED` (`SaveSet` `0x5ADE50` → `UpdateEquipmentSet` `0x5AAA00`); a delete
/// clears every slot holding the set, each sent as 0, before the list changes
/// (`RemoveEquipmentSet` `0x5AAE20`).
#[test]
fn a_rename_repaints_and_a_delete_clears_the_set_slots() {
    let mut h = listed();
    h.listen_bar();
    h.bar("PickupEquipmentSet(1) PlaceAction(9) PickupEquipmentSet(1) PlaceAction(7)");
    h.bar_seen();
    h.sent();
    h.bar(r#"RenameEquipmentSet("Tank", "Bulwark")"#);
    assert_eq!(
        h.bar_seen(),
        // `SaveSet`'s own signal, then `ModifyEquipmentSet`'s (`0x5AF400`).
        "ACTIONBAR_SLOT_CHANGED:7=Bulwark ACTIONBAR_SLOT_CHANGED:9=Bulwark \
         EQUIPMENT_SETS_CHANGED EQUIPMENT_SETS_CHANGED"
    );
    assert!(matches!(
        h.sent().as_slice(),
        [ClientCommand::EquipmentSetSave(s)] if s.name == "Bulwark" && s.id == 3
    ));
    let run = h.bar(r#"DeleteEquipmentSet("Bulwark")"#);
    assert_eq!(run.sets, [(7, 0), (9, 0)]);
    assert_eq!(
        h.bar_seen(),
        "ACTIONBAR_SLOT_CHANGED:7=nil ACTIONBAR_SLOT_CHANGED:9=nil EQUIPMENT_SETS_CHANGED"
    );
    assert!(matches!(
        h.sent().as_slice(),
        [ClientCommand::EquipmentSetDelete { set_guid: 0x70 }]
    ));
}

/// `IsUsableAction` on a set slot: the app's control leg and the set gates of
/// `UpdateUsableAction` (`0x5A9E20`), the manager's CVar and a locked worn-or-carried item.
#[test]
fn a_set_slot_is_usable_through_the_set_gates() {
    let mut h = listed();
    h.bar("PickupEquipmentSet(1) PlaceAction(7)");
    h.script.set_action_state(
        7,
        Some(benilla_ui::script::ActionState {
            usable: true,
            ..Default::default()
        }),
    );
    assert!(h.eval::<bool>("return IsUsableAction(7) == 1"));
    h.script.lock_item(HELM);
    let mut slots: benilla_ui::script::InventorySlots = Default::default();
    slots[1] = Some(benilla_ui::script::InvSlotView {
        item_id: HELM_ENTRY,
        guid: HELM,
        ..Default::default()
    });
    h.script.set_inventory_slots(slots);
    assert!(
        h.eval::<bool>("return IsUsableAction(7) == nil"),
        "a locked worn slot"
    );
    h.script.unlock_item(HELM);
    h.script.set_cvar_host("equipmentManager", "0");
    assert!(
        h.eval::<bool>("return IsUsableAction(7) == nil"),
        "the manager off"
    );
}
