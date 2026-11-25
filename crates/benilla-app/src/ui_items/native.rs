use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use benilla_ui::order::{Strata, ZKey};
use benilla_ui::script::{ContainerSlot, ContainerState, WrathCurrencyState};

use crate::items::Items;
use crate::local_state;
use crate::net::{ObjectStore, SelfPlayer};
use crate::query_cache::Ask;
use crate::ui_pass::{UiQuad, UiQuadAppend, UiQuads};
use crate::ui_script::{CursorPayloadHeld, PlayerUiPressConsumed, UiKeyboardCapture};

const SLOT: f32 = 40.0;
const GAP: f32 = 5.0;
const PADDING: f32 = 14.0;
const DEFAULT_COLS: usize = 8;
const MIN_COLS: usize = 4;
const MAX_COLS: usize = 12;
const DEFAULT_ROWS: usize = 4;
const MIN_ROWS: usize = 2;
const MAX_ROWS: usize = 10;
const TITLE_HEIGHT: f32 = 32.0;
const BAG_ORDER: [i64; 5] = [0, 1, 2, 3, 4];
const DIGIT_WIDTH: f32 = 4.0;
const DIGIT_HEIGHT: f32 = 7.0;
const DIGIT_GAP: f32 = 1.0;
const DIGIT_SEGMENT: f32 = 1.0;
const COPPER_ICON: &str = "Interface\\MoneyFrame\\UI-CopperIcon";
const SILVER_ICON: &str = "Interface\\MoneyFrame\\UI-SilverIcon";
const GOLD_ICON: &str = "Interface\\MoneyFrame\\UI-GoldIcon";
const PANEL_COUNT: usize = 3;
const DEFAULT_ALPHA: f32 = 0.97;
const MIN_ALPHA: f32 = 0.25;
const MAX_ALPHA: f32 = 1.0;
const PANEL_GAP: f32 = 18.0;
const BAG_BUTTON: f32 = 14.0;
const BAG_BUTTON_GAP: f32 = 4.0;
const SORT_BUTTON_GAP: f32 = 3.0;
const SORT_BUTTONS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortMode {
    Slot,
    Price,
    Name,
    Weight,
    Category,
}

impl SortMode {
    fn parse(value: &str) -> Self {
        match value {
            "price" => Self::Price,
            "name" => Self::Name,
            "weight" => Self::Weight,
            "category" => Self::Category,
            _ => Self::Slot,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Slot => "slot",
            Self::Price => "price",
            Self::Name => "name",
            Self::Weight => "weight",
            Self::Category => "category",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BagPanel {
    open: bool,
    position: Option<Vec2>,
    alpha: f32,
    dragging: bool,
    drag_offset: Vec2,
}

impl Default for BagPanel {
    fn default() -> Self {
        Self {
            open: true,
            position: None,
            alpha: DEFAULT_ALPHA,
            dragging: false,
            drag_offset: Vec2::ZERO,
        }
    }
}

#[derive(Resource)]
pub(crate) struct NativeBagWindow {
    open: bool,
    columns: usize,
    rows: usize,
    split: usize,
    sort: SortMode,
    panels: [BagPanel; PANEL_COUNT],
    money: u64,
    currency: WrathCurrencyState,
    hovered: Option<HoveredBagSlot>,
    loaded: bool,
    dirty: bool,
}

impl NativeBagWindow {
    pub(crate) fn covers_pointer(&self, viewport: Vec2, cursor: Vec2, count: usize) -> bool {
        if !self.open {
            return false;
        }
        let part_count = count.div_ceil(self.split.max(1));
        self.panels.iter().enumerate().any(|(index, panel)| {
            if index >= self.split || !panel.open {
                return false;
            }
            let size = window_size(part_count, self.columns, self.rows);
            let origin = panel_origin(viewport, size, panel, index, self.split);
            cursor.x >= origin.x
                && cursor.x < origin.x + size.x
                && cursor.y >= origin.y
                && cursor.y < origin.y + size.y
        })
    }
}

impl Default for NativeBagWindow {
    fn default() -> Self {
        Self {
            open: false,
            columns: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            split: 1,
            sort: SortMode::Slot,
            panels: [BagPanel::default(); PANEL_COUNT],
            money: 0,
            currency: WrathCurrencyState::default(),
            hovered: None,
            loaded: false,
            dirty: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BagSlot {
    bag: i64,
    slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoveredBagSlot {
    bag: i64,
    slot: u32,
}

pub(crate) fn combined_slot_count(bags: &std::collections::HashMap<i64, ContainerState>) -> usize {
    slots(bags).len()
}

#[derive(Clone, Debug)]
struct SortData {
    name: String,
    sell_price: u32,
    item_level: u32,
    class: u32,
    subclass: u32,
}

fn sorted_slots<F>(
    bags: &std::collections::HashMap<i64, ContainerState>,
    sort: SortMode,
    template: F,
) -> Vec<BagSlot>
where
    F: Fn(&ContainerSlot) -> Option<SortData>,
{
    let mut result = slots(bags);
    result.sort_by(|a, b| {
        let slot = |cell: &BagSlot| {
            bags.get(&cell.bag)
                .and_then(|bag| bag.slots.get(&cell.slot))
        };
        let ai = slot(a).and_then(&template);
        let bi = slot(b).and_then(&template);
        let empty = ai.is_none().cmp(&bi.is_none());
        if empty != std::cmp::Ordering::Equal {
            return empty;
        }
        let primary = match sort {
            SortMode::Slot => std::cmp::Ordering::Equal,
            SortMode::Price => bi
                .as_ref()
                .map_or(0, |v| v.sell_price)
                .cmp(&ai.as_ref().map_or(0, |v| v.sell_price)),
            SortMode::Name => ai
                .as_ref()
                .map_or("", |v| v.name.as_str())
                .cmp(bi.as_ref().map_or("", |v| v.name.as_str())),
            SortMode::Weight => bi
                .as_ref()
                .map_or(0_u64, |v| {
                    u64::from(slot(b).map_or(0, |s| s.count)) * u64::from(v.item_level)
                })
                .cmp(&ai.as_ref().map_or(0_u64, |v| {
                    u64::from(slot(a).map_or(0, |s| s.count)) * u64::from(v.item_level)
                })),
            SortMode::Category => (
                ai.as_ref().map_or(u32::MAX, |v| v.class),
                ai.as_ref().map_or(u32::MAX, |v| v.subclass),
            )
                .cmp(&(
                    bi.as_ref().map_or(u32::MAX, |v| v.class),
                    bi.as_ref().map_or(u32::MAX, |v| v.subclass),
                )),
        };
        primary.then_with(|| (a.bag, a.slot).cmp(&(b.bag, b.slot)))
    });
    result
}

fn template_data(items: &Items, slot: &ContainerSlot) -> Option<SortData> {
    items
        .template(slot.item_id, slot.guid, Ask::Read)
        .map(|item| SortData {
            name: item.name.to_ascii_lowercase(),
            sell_price: item.sell_price,
            item_level: item.item_level,
            class: item.class,
            subclass: item.subclass,
        })
}

fn slots(bags: &std::collections::HashMap<i64, ContainerState>) -> Vec<BagSlot> {
    BAG_ORDER
        .iter()
        .filter_map(|bag| bags.get(bag).map(|state| (*bag, state.num_slots)))
        .flat_map(|(bag, count)| (1..=count).map(move |slot| BagSlot { bag, slot }))
        .collect()
}

fn grid_rows(count: usize, columns: usize, minimum_rows: usize) -> usize {
    minimum_rows.max(count.div_ceil(columns))
}

fn coin_denominations(money: u64) -> [(u64, &'static str); 3] {
    [
        (money / 10_000, GOLD_ICON),
        ((money / 100) % 100, SILVER_ICON),
        (money % 100, COPPER_ICON),
    ]
}

fn window_size(count: usize, columns: usize, minimum_rows: usize) -> Vec2 {
    let rows = grid_rows(count, columns, minimum_rows);
    Vec2::new(
        PADDING * 2.0 + columns as f32 * SLOT + (columns - 1) as f32 * GAP,
        PADDING * 2.0
            + rows as f32 * SLOT
            + rows.saturating_sub(1) as f32 * GAP
            + TITLE_HEIGHT
            + 24.0,
    )
}

fn window_origin(viewport: Vec2, size: Vec2, position: Option<Vec2>) -> Vec2 {
    let origin = position.unwrap_or_else(|| (viewport - size) * 0.5);
    Vec2::new(
        origin.x.clamp(0.0, (viewport.x - size.x).max(0.0)),
        origin.y.clamp(0.0, (viewport.y - size.y).max(0.0)),
    )
}

fn panel_origin(viewport: Vec2, size: Vec2, panel: &BagPanel, index: usize, split: usize) -> Vec2 {
    let fallback = (viewport - size) * 0.5
        + Vec2::new(
            (index as f32 - (split.saturating_sub(1) as f32 / 2.0)) * (size.x + PANEL_GAP),
            0.0,
        );
    window_origin(viewport, size, panel.position.or(Some(fallback)))
}

fn load_layout(state: &mut NativeBagWindow) {
    let Some(path) = local_state::native_bags_path() else {
        state.loaded = true;
        return;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        state.loaded = true;
        return;
    };
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else { continue };
        let value = fields.next().unwrap_or_default();
        match key {
            "columns" => {
                state.columns = value
                    .parse()
                    .unwrap_or(DEFAULT_COLS)
                    .clamp(MIN_COLS, MAX_COLS)
            }
            "rows" => {
                state.rows = value
                    .parse()
                    .unwrap_or(DEFAULT_ROWS)
                    .clamp(MIN_ROWS, MAX_ROWS)
            }
            "split" => state.split = value.parse().unwrap_or(1).clamp(1, PANEL_COUNT),
            "sort" => state.sort = SortMode::parse(value),
            _ if key.starts_with("panel") => {
                let Ok(index) = key[5..].parse::<usize>() else {
                    continue;
                };
                let Some(panel) = state.panels.get_mut(index) else {
                    continue;
                };
                let values: Vec<_> = line.split_whitespace().skip(1).collect();
                if values.len() >= 4 {
                    panel.open = values[0] == "1";
                    panel.alpha = values[1]
                        .parse()
                        .unwrap_or(DEFAULT_ALPHA)
                        .clamp(MIN_ALPHA, MAX_ALPHA);
                    panel.position = match (values[2].parse(), values[3].parse()) {
                        (Ok(x), Ok(y)) => Some(Vec2::new(x, y)),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
    }
    state.loaded = true;
}

fn save_layout(state: &mut NativeBagWindow) {
    if !state.dirty {
        return;
    }
    let Some(path) = local_state::native_bags_path() else {
        state.dirty = false;
        return;
    };
    let mut text = String::from("version 1\n");
    text.push_str(&format!(
        "columns {}\nrows {}\nsplit {}\nsort {}\n",
        state.columns,
        state.rows,
        state.split,
        state.sort.as_str()
    ));
    for (index, panel) in state.panels.iter().enumerate() {
        let (x, y) = panel.position.map_or(("none".into(), "none".into()), |p| {
            (p.x.to_string(), p.y.to_string())
        });
        text.push_str(&format!(
            "panel{index} {} {} {x} {y}\n",
            u8::from(panel.open),
            panel.alpha
        ));
    }
    if let Err(error) = local_state::write_atomic(&path, &text) {
        warn!("native bag layout save failed: {error}");
    }
    state.dirty = false;
}

fn native_bag_z(offset: u128) -> u128 {
    ZKey::frame(Strata::Tooltip, 0, 0)
        .raw()
        .saturating_sub(32)
        .saturating_add(offset)
}

fn layout_button_x(left: f32, width: f32, index: usize) -> f32 {
    left + width - PADDING - (5 - index) as f32 * BAG_BUTTON - (4 - index) as f32 * BAG_BUTTON_GAP
}

fn sort_button_x(left: f32, index: usize) -> f32 {
    left + PADDING + index as f32 * (BAG_BUTTON + SORT_BUTTON_GAP)
}

fn sort_mode_index(mode: SortMode) -> usize {
    match mode {
        SortMode::Slot => 0,
        SortMode::Price => 1,
        SortMode::Name => 2,
        SortMode::Weight => 3,
        SortMode::Category => 4,
    }
}

fn sort_mode_at(index: usize) -> SortMode {
    match index {
        1 => SortMode::Price,
        2 => SortMode::Name,
        3 => SortMode::Weight,
        4 => SortMode::Category,
        _ => SortMode::Slot,
    }
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    z: u128,
    texture: Option<Handle<bevy::image::Image>>,
) -> UiQuad {
    UiQuad {
        rect,
        color,
        z_key: z,
        texture,
        ..default()
    }
}

fn append_native_bag_window(
    window: Query<&Window, With<PrimaryWindow>>,
    bags: Res<super::feed::BagView>,
    items: Res<Items>,
    mut assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<bevy::image::Image>>,
    mut quads: ResMut<UiQuads>,
    state: Res<NativeBagWindow>,
) {
    if !state.open {
        return;
    }
    let all_entries = sorted_slots(bags.snapshot(), state.sort, |slot| {
        template_data(&items, slot)
    });
    let part_size = all_entries.len().div_ceil(state.split.max(1));
    let viewport = window.single().map_or(Vec2::new(1024.0, 768.0), |w| {
        Vec2::new(w.width(), w.height())
    });
    for panel_index in 0..state.split {
        let panel = &state.panels[panel_index];
        let panel_start = quads.overlays.len();
        if !panel.open {
            continue;
        }
        let entries: Vec<BagSlot> = all_entries
            .iter()
            .skip(panel_index * part_size)
            .take(part_size)
            .copied()
            .collect();
        let size = window_size(entries.len(), state.columns, state.rows);
        let origin = panel_origin(viewport, size, panel, panel_index, state.split);
        let panel_alpha = panel.alpha;
        let left = origin.x.round();
        let top = origin.y.round();
        quads.overlays.push(quad(
            Rect::new(left, top, left + size.x, top + size.y),
            [0.055, 0.07, 0.075, 0.97],
            native_bag_z(0),
            None,
        ));
        quads.overlays.push(quad(
            Rect::new(left, top, left + size.x, top + TITLE_HEIGHT),
            [0.12, 0.15, 0.16, 1.0],
            native_bag_z(1),
            None,
        ));
        quads.overlays.push(quad(
            Rect::new(left + 1.0, top + 1.0, left + size.x - 1.0, top + 3.0),
            [0.75, 0.55, 0.22, 1.0],
            native_bag_z(2),
            None,
        ));
        let control_y = top + 9.0;
        let active_sort = sort_mode_index(state.sort);
        for index in 0..SORT_BUTTONS {
            let x = sort_button_x(left, index);
            let active = index == active_sort;
            quads.overlays.push(quad(
                Rect::new(x, control_y, x + BAG_BUTTON, control_y + BAG_BUTTON),
                if active {
                    [0.72, 0.52, 0.2, 1.0]
                } else {
                    [0.28, 0.34, 0.35, 1.0]
                },
                native_bag_z(3),
                None,
            ));
            let icon = [0.04, 0.05, 0.05, 1.0];
            match index {
                0 => {
                    for (dx, dy) in [(3.0, 3.0), (8.0, 3.0), (3.0, 8.0), (8.0, 8.0)] {
                        quads.overlays.push(quad(
                            Rect::new(x + dx, control_y + dy, x + dx + 3.0, control_y + dy + 3.0),
                            icon,
                            native_bag_z(4),
                            None,
                        ));
                    }
                }
                1 => {
                    quads.overlays.push(quad(
                        Rect::new(x + 3.0, control_y + 3.0, x + 11.0, control_y + 11.0),
                        [0.9, 0.78, 0.32, 1.0],
                        native_bag_z(4),
                        None,
                    ));
                    quads.overlays.push(quad(
                        Rect::new(x + 5.0, control_y + 5.0, x + 9.0, control_y + 9.0),
                        [0.4, 0.25, 0.08, 1.0],
                        native_bag_z(5),
                        None,
                    ));
                }
                2 => {
                    for (dy, width) in [(3.0, 8.0), (7.0, 6.0), (11.0, 8.0)] {
                        quads.overlays.push(quad(
                            Rect::new(
                                x + 3.0,
                                control_y + dy,
                                x + 3.0 + width,
                                control_y + dy + 1.5,
                            ),
                            icon,
                            native_bag_z(4),
                            None,
                        ));
                    }
                }
                3 => {
                    for (dy, width) in [(3.0, 3.0), (7.0, 6.0), (11.0, 8.0)] {
                        quads.overlays.push(quad(
                            Rect::new(
                                x + 3.0,
                                control_y + dy,
                                x + 3.0 + width,
                                control_y + dy + 1.5,
                            ),
                            icon,
                            native_bag_z(4),
                            None,
                        ));
                    }
                }
                _ => {
                    for (dy, width) in [(3.0, 8.0), (7.0, 5.0), (11.0, 8.0)] {
                        quads.overlays.push(quad(
                            Rect::new(
                                x + 3.0,
                                control_y + dy,
                                x + 3.0 + width,
                                control_y + dy + 1.5,
                            ),
                            icon,
                            native_bag_z(4),
                            None,
                        ));
                    }
                }
            }
        }
        for (color, index) in [
            ([0.4, 0.46, 0.47, 1.0], 0),
            ([0.7, 0.7, 0.68, 1.0], 1),
            ([0.4, 0.46, 0.47, 1.0], 2),
            ([0.7, 0.7, 0.68, 1.0], 3),
            ([0.8, 0.35, 0.3, 1.0], 4),
        ] {
            let x = layout_button_x(left, size.x, index);
            quads.overlays.push(quad(
                Rect::new(x, control_y, x + BAG_BUTTON, control_y + BAG_BUTTON),
                color,
                native_bag_z(3),
                None,
            ));
        }
        for (index, (x, y, width, height)) in [
            (0, (3.0, 6.0, 5.0, 2.0)),
            (0, (9.0, 3.0, 2.0, 8.0)),
            (1, (3.0, 6.0, 5.0, 2.0)),
            (1, (6.0, 4.0, 2.0, 4.0)),
            (2, (3.0, 6.0, 5.0, 2.0)),
            (2, (9.0, 3.0, 2.0, 8.0)),
            (3, (3.0, 6.0, 5.0, 2.0)),
            (3, (7.0, 4.0, 2.0, 4.0)),
            (4, (5.0, 3.0, 2.0, 8.0)),
            (4, (9.0, 3.0, 2.0, 8.0)),
        ] {
            let button_x = layout_button_x(left, size.x, index);
            quads.overlays.push(quad(
                Rect::new(
                    button_x + x,
                    control_y + y,
                    button_x + x + width,
                    control_y + y + height,
                ),
                [0.04, 0.05, 0.05, 1.0],
                native_bag_z(4),
                None,
            ));
        }
        let purse = state.money;
        for (index, (value, icon)) in coin_denominations(purse).into_iter().enumerate() {
            let x0 = left + PADDING + index as f32 * 68.0;
            if let Some(handle) = assets
                .as_deref_mut()
                .and_then(|a| a.sprite_texture(icon, &mut images))
            {
                quads.overlays.push(quad(
                    Rect::new(x0, top + size.y - 20.0, x0 + 16.0, top + size.y - 4.0),
                    [1.0; 4],
                    native_bag_z(6),
                    Some(handle),
                ));
            }
            for (digit, ch) in value.to_string().chars().enumerate() {
                for (segment, on) in digit_segments(ch).into_iter().enumerate() {
                    if on {
                        let (sx, sy, width, height) = digit_segment_rect(segment);
                        let x = x0 + 18.0 + digit as f32 * (DIGIT_WIDTH + DIGIT_GAP) + sx;
                        let y = top + size.y - 13.0 + sy;
                        quads.overlays.push(quad(
                            Rect::new(x, y, x + width, y + height),
                            [0.9, 0.9, 0.84, 1.0],
                            native_bag_z(6),
                            None,
                        ));
                    }
                }
            }
        }
        let currency_rows = state
            .currency
            .rows
            .iter()
            .filter(|row| !row.is_header && row.count > 0)
            .chain(state.currency.backpack.iter().filter(|row| row.count > 0));
        for (index, row) in currency_rows.take(4).enumerate() {
            let x = left + size.x - 24.0 - index as f32 * 26.0;
            if let Some(handle) = row.icon.as_deref().and_then(|path| {
                assets
                    .as_deref_mut()
                    .and_then(|a| a.sprite_texture(path, &mut images))
            }) {
                quads.overlays.push(quad(
                    Rect::new(x, top + size.y - 24.0, x + 20.0, top + size.y - 4.0),
                    [1.0; 4],
                    native_bag_z(7),
                    Some(handle),
                ));
            }
        }
        for (index, cell) in entries.iter().enumerate() {
            let col = index % state.columns;
            let row = index / state.columns;
            let x = left + PADDING + col as f32 * (SLOT + GAP);
            let y = top + 32.0 + PADDING + row as f32 * (SLOT + GAP);
            let rect = Rect::new(x, y, x + SLOT, y + SLOT);
            let item = bags
                .snapshot()
                .get(&cell.bag)
                .and_then(|b| b.slots.get(&cell.slot));
            let quality = item.and_then(|v| v.quality).unwrap_or(0).min(6) as usize;
            let colors = [
                [0.28, 0.30, 0.30, 1.0],
                [0.62, 0.62, 0.62, 1.0],
                [0.20, 0.78, 0.24, 1.0],
                [0.18, 0.55, 0.88, 1.0],
                [0.68, 0.35, 0.90, 1.0],
                [0.95, 0.52, 0.12, 1.0],
                [0.90, 0.76, 0.20, 1.0],
            ];
            let border = colors[quality];
            quads
                .overlays
                .push(quad(rect, [0.015, 0.02, 0.025, 1.0], native_bag_z(8), None));
            for edge in [
                Rect::new(x, y, x + SLOT, y + 2.0),
                Rect::new(x, y + SLOT - 2.0, x + SLOT, y + SLOT),
                Rect::new(x, y, x + 2.0, y + SLOT),
                Rect::new(x + SLOT - 2.0, y, x + SLOT, y + SLOT),
            ] {
                quads
                    .overlays
                    .push(quad(edge, border, native_bag_z(9), None));
            }
            let Some(item) = item else { continue };
            let Some(path) = item.texture.as_deref() else {
                continue;
            };
            if let Some(handle) = assets
                .as_deref_mut()
                .and_then(|a| a.sprite_texture(path, &mut images))
            {
                quads.overlays.push(quad(
                    Rect::new(x + 4.0, y + 4.0, x + SLOT - 4.0, y + SLOT - 4.0),
                    [1.0, 1.0, 1.0, 1.0],
                    native_bag_z(10),
                    Some(handle),
                ));
            }
            if item.count > 1 {
                let count = item.count.min(99_999).to_string();
                let total_width = count.len() as f32 * DIGIT_WIDTH
                    + count.len().saturating_sub(1) as f32 * DIGIT_GAP;
                for (digit, ch) in count.chars().enumerate() {
                    let bars = digit_segments(ch);
                    let bx =
                        x + SLOT - 3.0 - total_width + digit as f32 * (DIGIT_WIDTH + DIGIT_GAP);
                    for (segment, on) in bars.into_iter().enumerate() {
                        if !on {
                            continue;
                        }
                        let (sx, sy, width, height) = digit_segment_rect(segment);
                        quads.overlays.push(quad(
                            Rect::new(
                                bx + sx,
                                y + SLOT - 2.0 - DIGIT_HEIGHT + sy,
                                bx + sx + width,
                                y + SLOT - 2.0 - DIGIT_HEIGHT + sy + height,
                            ),
                            [1.0, 1.0, 0.88, 1.0],
                            native_bag_z(11),
                            None,
                        ));
                    }
                }
            }
        }
        for quad in quads.overlays.iter_mut().skip(panel_start) {
            quad.color[3] *= panel_alpha;
        }
        let alpha_track = Rect::new(
            left + size.x - 112.0,
            top + size.y - 8.0,
            left + size.x - 18.0,
            top + size.y - 5.0,
        );
        quads.overlays.push(quad(
            alpha_track,
            [0.18, 0.20, 0.20, panel_alpha],
            native_bag_z(12),
            None,
        ));
        quads.overlays.push(quad(
            Rect::new(
                alpha_track.min.x,
                alpha_track.min.y,
                alpha_track.min.x + alpha_track.width() * panel_alpha,
                alpha_track.max.y,
            ),
            [0.78, 0.58, 0.24, 1.0],
            native_bag_z(13),
            None,
        ));
    }
}

fn digit_segment_rect(segment: usize) -> (f32, f32, f32, f32) {
    match segment {
        0 => (0.0, 0.0, DIGIT_WIDTH, DIGIT_SEGMENT),
        1 => (
            DIGIT_WIDTH - DIGIT_SEGMENT,
            DIGIT_SEGMENT,
            DIGIT_SEGMENT,
            DIGIT_HEIGHT / 2.0 - DIGIT_SEGMENT,
        ),
        2 => (
            DIGIT_WIDTH - DIGIT_SEGMENT,
            DIGIT_HEIGHT / 2.0,
            DIGIT_SEGMENT,
            DIGIT_HEIGHT / 2.0 - DIGIT_SEGMENT,
        ),
        3 => (
            0.0,
            DIGIT_HEIGHT - DIGIT_SEGMENT,
            DIGIT_WIDTH,
            DIGIT_SEGMENT,
        ),
        4 => (
            0.0,
            DIGIT_HEIGHT / 2.0,
            DIGIT_SEGMENT,
            DIGIT_HEIGHT / 2.0 - DIGIT_SEGMENT,
        ),
        5 => (
            0.0,
            DIGIT_SEGMENT,
            DIGIT_SEGMENT,
            DIGIT_HEIGHT / 2.0 - DIGIT_SEGMENT,
        ),
        6 => (
            0.0,
            DIGIT_HEIGHT / 2.0 - DIGIT_SEGMENT / 2.0,
            DIGIT_WIDTH,
            DIGIT_SEGMENT,
        ),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

fn digit_segments(ch: char) -> [bool; 7] {
    match ch {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => [false; 7],
    }
}

fn close_stock_bag_frames(
    state: Res<NativeBagWindow>,
    mut hosted_vm: crate::script_calls::HostedVm,
) {
    if !state.open {
        return;
    }
    hosted_vm.run(|script| {
        if let Some(script) = script {
            // The ordinary binding dispatch follows native input in this frame. Keep its stock
            // ContainerFrame presentation suppressed while the replacement window is open.
            let _ = script.run("CloseAllBags()");
        }
    });
}

fn native_bag_hover(
    window: Query<&Window, With<PrimaryWindow>>,
    bags: Res<super::feed::BagView>,
    items: Res<Items>,
    mut state: ResMut<NativeBagWindow>,
    mut hosted_vm: crate::script_calls::HostedVm,
) {
    let previous = state.hovered;
    let mut hovered = None;
    if state.open {
        if !state.loaded {
            load_layout(&mut state);
        }
        if let Ok(window) = window.single() {
            if let Some(cursor) = window.cursor_position() {
                let all_entries = sorted_slots(bags.snapshot(), state.sort, |slot| {
                    template_data(&items, slot)
                });
                let part_size = all_entries.len().div_ceil(state.split.max(1));
                let viewport = Vec2::new(window.width(), window.height());
                'panels: for panel_index in 0..state.split {
                    let panel = &state.panels[panel_index];
                    if !panel.open {
                        continue;
                    }
                    let entries: Vec<BagSlot> = all_entries
                        .iter()
                        .skip(panel_index * part_size)
                        .take(part_size)
                        .copied()
                        .collect();
                    let size = window_size(entries.len(), state.columns, state.rows);
                    let origin = panel_origin(viewport, size, panel, panel_index, state.split);
                    let x = cursor.x - origin.x - PADDING;
                    let y = cursor.y - origin.y - TITLE_HEIGHT - PADDING;
                    if x < 0.0 || y < 0.0 || y >= size.y - PADDING * 2.0 - TITLE_HEIGHT {
                        continue;
                    }
                    let col = (x / (SLOT + GAP)) as usize;
                    let row = (y / (SLOT + GAP)) as usize;
                    if col >= state.columns
                        || row >= grid_rows(entries.len(), state.columns, state.rows)
                        || x % (SLOT + GAP) >= SLOT
                        || y % (SLOT + GAP) >= SLOT
                    {
                        continue;
                    }
                    if let Some(cell) = entries.get(row * state.columns + col) {
                        if bags
                            .snapshot()
                            .get(&cell.bag)
                            .and_then(|bag| bag.slots.get(&cell.slot))
                            .is_some()
                        {
                            hovered = Some(HoveredBagSlot {
                                bag: cell.bag,
                                slot: cell.slot,
                            });
                            break 'panels;
                        }
                    }
                }
            }
        }
    }
    if previous == hovered {
        return;
    }
    state.hovered = hovered;
    hosted_vm.run(|script| {
        if let Some(script) = script {
            match hovered {
                Some(cell) => {
                    let _ = script.run(&format!(
                        "GameTooltip:SetOwner(UIParent, \"ANCHOR_CURSOR\"); GameTooltip:SetBagItem({}, {}); GameTooltip:Show()",
                        cell.bag, cell.slot
                    ));
                }
                None if previous.is_some() => {
                    let _ = script.run("GameTooltip:Hide()");
                }
                None => {}
            }
        }
    });
}

fn native_bag_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    window: Query<&Window, With<PrimaryWindow>>,
    bags: Res<super::feed::BagView>,
    items: Res<Items>,
    player: Query<&ObjectStore, With<SelfPlayer>>,
    mut state: ResMut<NativeBagWindow>,
    mut capture: ResMut<UiKeyboardCapture>,
    mut press: ResMut<PlayerUiPressConsumed>,
    payload_held: Res<CursorPayloadHeld>,
    mut hosted_vm: crate::script_calls::HostedVm,
) {
    if keys.just_pressed(KeyCode::Escape)
        && state.open
        && !capture.typing
        && !capture.consumed.contains(&KeyCode::Escape)
    {
        state.open = false;
        state.dirty = true;
        hosted_vm.run(|script| {
            if let Some(script) = script {
                let _ = script.run("CloseAllBags()");
            }
        });
        return;
    }
    if keys.just_pressed(KeyCode::KeyB)
        && !capture.typing
        && !capture.consumed.contains(&KeyCode::KeyB)
    {
        state.open = !state.open;
        if state.open {
            let split = state.split;
            for panel in state.panels.iter_mut().take(split) {
                panel.open = true;
            }
        }
        state.dirty = true;
        capture.consumed.push(KeyCode::KeyB);
        hosted_vm.run(|script| {
            if let Some(script) = script {
                // Keep the compatibility API and its event behavior, but remove stock container
                // frames before the native combined window becomes visible.
                let _ = script.run("CloseAllBags()");
            }
        });
    }
    if !state.open {
        return;
    }
    state.money = player
        .single()
        .ok()
        .and_then(|p| p.0.player_money())
        .unwrap_or(0) as u64;
    hosted_vm.run(|script| {
        if let Some(script) = script {
            state.currency = script.wrath_currency_view();
        }
    });
    let Ok(window) = window.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if !state.loaded {
        load_layout(&mut state);
    }
    let all_entries = sorted_slots(bags.snapshot(), state.sort, |slot| {
        template_data(&items, slot)
    });
    let part_size = all_entries.len().div_ceil(state.split.max(1));
    let viewport = Vec2::new(window.width(), window.height());
    let wheel_delta: f32 = wheel.read().map(|event| event.y).sum();
    for panel_index in 0..state.split {
        if !state.panels[panel_index].open {
            continue;
        }
        let entries: Vec<BagSlot> = all_entries
            .iter()
            .skip(panel_index * part_size)
            .take(part_size)
            .copied()
            .collect();
        let size = window_size(entries.len(), state.columns, state.rows);
        let origin = panel_origin(
            viewport,
            size,
            &state.panels[panel_index],
            panel_index,
            state.split,
        );
        let left = origin.x.round();
        let top = origin.y.round();
        let inside = cursor.x >= left
            && cursor.x < left + size.x
            && cursor.y >= top
            && cursor.y < top + size.y;
        if !inside {
            continue;
        }
        press.0 = true;
        if wheel_delta != 0.0 {
            let panel = &mut state.panels[panel_index];
            panel.alpha = (panel.alpha + wheel_delta * 0.05).clamp(MIN_ALPHA, MAX_ALPHA);
            state.dirty = true;
            return;
        }
        if buttons.just_pressed(MouseButton::Left)
            && cursor.y >= top + size.y - 14.0
            && cursor.x >= left + size.x - 112.0
            && cursor.x <= left + size.x - 18.0
            && !payload_held.0
        {
            state.panels[panel_index].alpha =
                ((cursor.x - (left + size.x - 112.0)) / 94.0).clamp(MIN_ALPHA, MAX_ALPHA);
            state.dirty = true;
            return;
        }
        let panel = &mut state.panels[panel_index];
        if panel.dragging {
            if buttons.pressed(MouseButton::Left) {
                panel.position = Some(cursor - panel.drag_offset);
                state.dirty = true;
                return;
            }
            panel.dragging = false;
        }
        if cursor.y < top + TITLE_HEIGHT {
            let relative_x = cursor.x - (left + size.x);
            let sort_x = cursor.x - (left + PADDING);
            if buttons.just_pressed(MouseButton::Left)
                && !payload_held.0
                && sort_x >= 0.0
                && sort_x < SORT_BUTTONS as f32 * (BAG_BUTTON + SORT_BUTTON_GAP)
            {
                let index = (sort_x / (BAG_BUTTON + SORT_BUTTON_GAP)) as usize;
                if index < SORT_BUTTONS && sort_x % (BAG_BUTTON + SORT_BUTTON_GAP) < BAG_BUTTON {
                    state.sort = sort_mode_at(index);
                    state.dirty = true;
                }
            } else if buttons.just_pressed(MouseButton::Right) && !payload_held.0 {
                state.sort = match state.sort {
                    SortMode::Slot => SortMode::Price,
                    SortMode::Price => SortMode::Name,
                    SortMode::Name => SortMode::Weight,
                    SortMode::Weight => SortMode::Category,
                    SortMode::Category => SortMode::Slot,
                };
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Middle) && !payload_held.0 {
                state.split = if state.split == PANEL_COUNT {
                    1
                } else {
                    state.split + 1
                };
                state.dirty = true;
                return;
            } else if buttons.just_pressed(MouseButton::Left)
                && (-100.0..=-86.0).contains(&relative_x)
                && !payload_held.0
            {
                state.columns = (state.columns - 1).max(MIN_COLS);
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Left)
                && (-80.0..=-66.0).contains(&relative_x)
                && !payload_held.0
            {
                state.columns = (state.columns + 1).min(MAX_COLS);
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Left)
                && (-60.0..=-46.0).contains(&relative_x)
                && !payload_held.0
            {
                state.rows = (state.rows - 1).max(MIN_ROWS);
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Left)
                && (-40.0..=-26.0).contains(&relative_x)
                && !payload_held.0
            {
                state.rows = (state.rows + 1).min(MAX_ROWS);
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Left)
                && (-28.0..=-14.0).contains(&relative_x)
                && !payload_held.0
            {
                panel.open = false;
                state.dirty = true;
            } else if buttons.just_pressed(MouseButton::Left) && !payload_held.0 {
                panel.dragging = true;
                panel.drag_offset = cursor - origin;
            }
            return;
        }
        let x = cursor.x - left - PADDING;
        let y = cursor.y - top - 32.0 - PADDING;
        if x < 0.0 || y < 0.0 || y >= size.y - PADDING * 2.0 - 32.0 {
            return;
        }
        let col = (x / (SLOT + GAP)) as usize;
        let row = (y / (SLOT + GAP)) as usize;
        if col >= state.columns || x % (SLOT + GAP) >= SLOT || y % (SLOT + GAP) >= SLOT {
            return;
        }
        let index = row * state.columns + col;
        let Some(cell) = entries.get(index).copied() else {
            return;
        };
        if buttons.just_pressed(MouseButton::Left) && !payload_held.0 {
            let chat_linked = hosted_vm.run(|script| {
                let Some(script) = script else { return false };
                script
                    .eval::<bool>(&format!(
                        "local e = ChatEdit_GetActiveWindow and ChatEdit_GetActiveWindow() or ChatFrameEditBox; return IsShiftKeyDown() and e and e:IsVisible() and GetContainerItemLink({}, {}) ~= nil",
                        cell.bag, cell.slot
                    ))
                    .unwrap_or(false)
            });
            if chat_linked {
                hosted_vm.run(|script| {
                    if let Some(script) = script {
                        let _ = script.run(&format!(
                            "local e = ChatEdit_GetActiveWindow and ChatEdit_GetActiveWindow() or ChatFrameEditBox; if e then e:Insert(GetContainerItemLink({}, {})) end",
                            cell.bag, cell.slot
                        ));
                    }
                });
            } else {
                let verb = format!("PickupContainerItem({}, {})", cell.bag, cell.slot);
                hosted_vm.run(|script| {
                    if let Some(script) = script {
                        if let Err(error) = script.run(&verb) {
                            warn!("native bag action failed: {error}");
                        }
                    }
                });
            }
        } else if buttons.just_pressed(MouseButton::Right)
            || (buttons.just_released(MouseButton::Left) && payload_held.0)
        {
            let verb = if buttons.just_pressed(MouseButton::Right) {
                format!("UseContainerItem({}, {})", cell.bag, cell.slot)
            } else {
                format!("PickupContainerItem({}, {})", cell.bag, cell.slot)
            };
            hosted_vm.run(|script| {
                if let Some(script) = script {
                    if let Err(error) = script.run(&verb) {
                        warn!("native bag action failed: {error}");
                    }
                }
            });
        }
        return;
    }
    save_layout(&mut state);
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<NativeBagWindow>()
        .add_systems(
            Update,
            native_bag_hover
                .in_set(crate::ui_script::UiInput)
                .after(crate::ui_script::UiKeyFeed)
                .before(native_bag_input),
        )
        .add_systems(
            Update,
            native_bag_input
                .in_set(crate::ui_script::UiInput)
                .after(crate::ui_script::UiKeyFeed)
                .before(crate::bindings::BindingSet),
        )
        .add_systems(
            Update,
            close_stock_bag_frames
                .in_set(crate::ui_script::UiInput)
                .after(crate::bindings::BindingSet),
        )
        .add_systems(Update, append_native_bag_window.in_set(UiQuadAppend))
        .add_systems(Update, save_native_bag_layout.after(native_bag_input));
}

fn save_native_bag_layout(mut state: ResMut<NativeBagWindow>) {
    save_layout(&mut state);
}

#[cfg(test)]
mod tests {
    use super::{
        coin_denominations, digit_segment_rect, digit_segments, grid_rows, native_bag_z, slots,
        sort_mode_at, sort_mode_index, window_origin, window_size, Strata, ZKey, COPPER_ICON,
        DEFAULT_COLS, DEFAULT_ROWS, DIGIT_SEGMENT, DIGIT_WIDTH, GOLD_ICON, SILVER_ICON,
        SORT_BUTTONS,
    };
    use benilla_ui::script::ContainerState;
    use bevy::prelude::Vec2;
    use std::collections::HashMap;

    #[test]
    fn combines_backpack_and_equipped_bag_slots_in_stable_order() {
        let bags = HashMap::from([
            (
                0,
                ContainerState {
                    num_slots: 16,
                    ..Default::default()
                },
            ),
            (
                2,
                ContainerState {
                    num_slots: 6,
                    ..Default::default()
                },
            ),
        ]);
        let result = slots(&bags);
        assert_eq!(result.len(), 22);
        assert_eq!((result[15].bag, result[15].slot), (0, 16));
        assert_eq!((result[16].bag, result[16].slot), (2, 1));
        let narrow = window_size(result.len(), 4, 6);
        let wide = window_size(result.len(), 10, 4);
        assert!(narrow.x < wide.x && narrow.y > wide.y);
        assert_eq!(grid_rows(result.len(), 4, 2), 6);
        assert_eq!(grid_rows(result.len(), 10, 4), 4);
        assert!(window_size(result.len(), DEFAULT_COLS, DEFAULT_ROWS).x > 0.0);
        let origin = window_origin(Vec2::new(800.0, 600.0), wide, Some(Vec2::new(900.0, -20.0)));
        assert_eq!(origin.x, (800.0 - wide.x).max(0.0));
        assert_eq!(origin.y, 0.0);
    }

    #[test]
    fn grid_reflow_maps_every_slot_once_for_each_column_count() {
        for columns in 4..=12 {
            let rows = grid_rows(22, columns, 2);
            assert_eq!(rows, 22_usize.div_ceil(columns));
            assert_eq!(rows * columns >= 22, true);
        }
    }

    #[test]
    fn sort_modes_reorder_slots_without_changing_slot_identity() {
        use super::{sorted_slots, SortMode};
        use benilla_ui::script::ContainerSlot;

        let bags = HashMap::from([(
            0,
            ContainerState {
                name: None,
                num_slots: 3,
                slots: HashMap::from([
                    (
                        1,
                        ContainerSlot {
                            item_id: 1,
                            link: Some("|Hitem:1|h[Zinc]|h".into()),
                            count: 2,
                            ..Default::default()
                        },
                    ),
                    (
                        2,
                        ContainerSlot {
                            item_id: 2,
                            link: Some("|Hitem:2|h[Apple]|h".into()),
                            count: 1,
                            ..Default::default()
                        },
                    ),
                ]),
            },
        )]);
        let data = |slot: &ContainerSlot| match slot.item_id {
            1 => Some(super::SortData {
                name: "zinc".into(),
                sell_price: 5,
                item_level: 10,
                class: 4,
                subclass: 1,
            }),
            2 => Some(super::SortData {
                name: "apple".into(),
                sell_price: 20,
                item_level: 3,
                class: 0,
                subclass: 0,
            }),
            _ => None,
        };
        assert_eq!(sorted_slots(&bags, SortMode::Slot, data)[0].slot, 1);
        assert_eq!(sorted_slots(&bags, SortMode::Price, data)[0].slot, 2);
        assert_eq!(sorted_slots(&bags, SortMode::Name, data)[0].slot, 2);
        assert_eq!(sorted_slots(&bags, SortMode::Weight, data)[0].slot, 1);
        assert_eq!(sorted_slots(&bags, SortMode::Category, data)[0].slot, 2);
    }

    #[test]
    fn split_partitions_keep_every_slot_exactly_once() {
        let all = (1..=22).collect::<Vec<_>>();
        for parts in 1..=3 {
            let part_size = all.len().div_ceil(parts);
            let partitioned = (0..parts)
                .flat_map(|index| all.iter().skip(index * part_size).take(part_size).copied())
                .collect::<Vec<_>>();
            assert_eq!(partitioned, all);
        }
    }

    #[test]
    fn native_bag_quads_stay_below_scripted_tooltips() {
        let tooltip = ZKey::frame(Strata::Tooltip, 0, 0).raw();
        assert!(native_bag_z(13) < tooltip);
    }

    #[test]
    fn sort_buttons_cover_each_sort_mode_once() {
        for index in 0..SORT_BUTTONS {
            assert_eq!(sort_mode_index(sort_mode_at(index)), index);
        }
    }

    #[test]
    fn purse_denominations_are_gold_silver_copper_left_to_right() {
        assert_eq!(
            coin_denominations(12_345),
            [(1, GOLD_ICON), (23, SILVER_ICON), (45, COPPER_ICON)]
        );
    }

    #[test]
    fn count_digit_segments_cover_decimal_digits() {
        assert_eq!(digit_segments('8'), [true; 7]);
        assert_eq!(
            digit_segments('1'),
            [false, true, true, false, false, false, false]
        );
        assert_eq!(
            digit_segment_rect(0),
            (0.0, 0.0, DIGIT_WIDTH, DIGIT_SEGMENT)
        );
        assert_eq!(digit_segment_rect(1).2, DIGIT_SEGMENT);
        assert!(digit_segment_rect(4).1 > digit_segment_rect(0).1);
    }
}
