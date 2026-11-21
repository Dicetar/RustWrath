//! Project the 3.3.5 currency fields, item templates, and local display masks into TokenFrame.

use std::collections::HashMap;

use benilla_assets::{AssetSet, LockRecover};
use benilla_protocol::ObjectFields;
use benilla_protocol_wrath::fields;
use benilla_ui::script::{
    build_currency_state, CurrencyCategoryInput, CurrencyTypeInput, UiTarget,
};
use bevy::prelude::*;

use crate::cvars::Cvars;
use crate::entities::ItemDisplays;
use crate::items::Items;
use crate::net::{NetCommands, Objects, SelfPlayer};
use crate::query_cache::Ask;
use crate::script_calls::VmWith;
use crate::ui_script::{UiFeed, VmMemo};
use crate::wrath::WrathPlayerRaw;

#[derive(Resource)]
pub(crate) struct CurrencyTables {
    pub(crate) types: benilla_formats::CurrencyTypeCatalog,
    pub(crate) categories: benilla_formats::CurrencyCategoryCatalog,
}

#[derive(Default)]
struct CurrencyMemo(Option<benilla_ui::script::WrathCurrencyState>);

type CurrencyFeedResources = (
    Option<Res<'static, CurrencyTables>>,
    Res<'static, Items>,
    Option<Res<'static, ItemDisplays>>,
    Option<Res<'static, Cvars>>,
    Res<'static, NetCommands>,
    Query<'static, 'static, &'static WrathPlayerRaw, With<SelfPlayer>>,
    Objects<'static, 'static>,
);

fn mask(cvars: Option<&Cvars>, name: &str) -> u32 {
    cvars
        .and_then(|c| c.get(name))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
}

fn object_entry(fields: &ObjectFields) -> Option<u32> {
    fields.object_entry()
}

fn feed(mut vm: VmWith<CurrencyFeedResources>, mut memo: Local<VmMemo<CurrencyMemo>>) {
    vm.turn(
        |script, (tables, items, displays, cvars, commands, raw, objects)| {
            let Some(script) = script else { return };
            if script.target() != UiTarget::Wrath335 {
                return;
            }
            let Some(tables) = tables.as_deref() else {
                return;
            };
            let Some(raw) = raw.iter().next() else { return };
            let known = [
                raw.0
                    .get(fields::PLAYER_FIELD_KNOWN_CURRENCIES)
                    .unwrap_or(0),
                raw.0
                    .get(fields::PLAYER_FIELD_KNOWN_CURRENCIES + 1)
                    .unwrap_or(0),
            ];
            let unused = [
                mask(cvars.as_deref(), "currencyTokensUnused1"),
                mask(cvars.as_deref(), "currencyTokensUnused2"),
            ];
            let backpack = [
                mask(cvars.as_deref(), "currencyTokensBackpack1"),
                mask(cvars.as_deref(), "currencyTokensBackpack2"),
            ];
            let mut counts = HashMap::new();
            for slot in 0..32u16 {
                let base = fields::PLAYER_FIELD_CURRENCYTOKEN_SLOT_1 + slot * 2;
                let Some(guid) = raw.0.guid(base) else {
                    continue;
                };
                if guid == 0 {
                    continue;
                }
                if let Some(item) = objects.object(guid) {
                    if let Some(entry) = object_entry(item) {
                        counts.insert(entry, item.item_stack_count().unwrap_or(1));
                    }
                }
            }
            let honor = raw.0.get(fields::PLAYER_FIELD_HONOR_CURRENCY).unwrap_or(0);
            let arena = raw.0.get(fields::PLAYER_FIELD_ARENA_CURRENCY).unwrap_or(0);
            let item = |entry: u32| {
                let template = items.template(entry, 0, Ask::New(&commands))?;
                let icon = displays
                    .as_deref()
                    .and_then(|d| d.catalog.get(template.display_info_id))
                    .and_then(|d| d.icon.clone());
                Some((
                    Some(template.name.clone()),
                    icon,
                    match entry {
                        benilla_ui::script::ARENA_ITEM => arena,
                        benilla_ui::script::HONOR_ITEM => honor,
                        _ => counts.get(&entry).copied().unwrap_or(0),
                    },
                ))
            };
            let state = build_currency_state(
                tables.types.iter().map(|t| CurrencyTypeInput {
                    item_id: t.item_id,
                    category_id: t.category_id,
                    bit_index: t.bit_index,
                }),
                tables.categories.rows().map(|c| CurrencyCategoryInput {
                    id: c.id,
                    flags: c.flags,
                    name: c.name.clone(),
                }),
                known,
                unused,
                backpack,
                |category_id| script.wrath_currency_expanded(category_id),
                item,
            );
            let memo = memo.get(script);
            if memo.0.as_ref() != Some(&state) {
                script.set_wrath_currency(state.clone());
                script.fire_event("CURRENCY_DISPLAY_UPDATE", Vec::new());
                memo.0 = Some(state);
            }
        },
    );
}

pub(crate) fn load_tables(
    mut commands: Commands,
    assets: Option<Res<benilla_assets::WorldAssets>>,
) {
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    let Ok(types) = benilla_formats::load_currency_type_catalog(&mut chain) else {
        warn!("currency: CurrencyTypes.dbc unavailable");
        return;
    };
    let Ok(categories) = benilla_formats::load_currency_category_catalog(&mut chain) else {
        warn!("currency: CurrencyCategory.dbc unavailable");
        return;
    };
    info!(
        "currency: loaded {} types and {} categories",
        types.len(),
        categories.len()
    );
    commands.insert_resource(CurrencyTables { types, categories });
}

pub(crate) struct UiWrathCurrencyPlugin;

impl Plugin for UiWrathCurrencyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_tables.after(AssetSet::Open))
            .add_systems(Update, feed.in_set(UiFeed));
    }
}
