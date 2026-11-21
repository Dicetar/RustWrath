//! Stock 3.3.5 currency list state and Lua-facing row helpers.

use super::model::{WrathCurrencyRow, WrathCurrencyState};

#[derive(Clone, Debug)]
pub struct CurrencyTypeInput {
    pub item_id: u32,
    pub category_id: u32,
    pub bit_index: u32,
}

#[derive(Clone, Debug)]
pub struct CurrencyCategoryInput {
    pub id: u32,
    pub flags: u32,
    pub name: String,
}

pub const ARENA_ITEM: u32 = 43_307;
pub const HONOR_ITEM: u32 = 43_308;

/// Build the display list from the two known bitsets and the four persisted display masks.
/// `expanded` preserves the per-category state held by `CGCurrencyTypes::m_categories`.
pub fn build_currency_state(
    types: impl IntoIterator<Item = CurrencyTypeInput>,
    categories: impl IntoIterator<Item = CurrencyCategoryInput>,
    known: [u32; 2],
    unused: [u32; 2],
    backpack: [u32; 2],
    expanded: impl Fn(u32) -> bool,
    items: impl Fn(u32) -> Option<(Option<String>, Option<String>, u32)>,
) -> WrathCurrencyState {
    let mut categories: Vec<_> = categories.into_iter().collect();
    categories.sort_by(|a, b| {
        (a.flags & 1)
            .cmp(&(b.flags & 1))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let types: Vec<_> = types.into_iter().collect();
    let mut rows = Vec::new();
    for (category_index, category) in categories.iter().enumerate() {
        let is_unused_category = category_index + 1 == categories.len();
        let has_known = types.iter().any(|ty| {
            let (word, bit) = match ty.bit_index {
                1..=32 => (0, ty.bit_index - 1),
                33..=64 => (1, ty.bit_index - 33),
                _ => return false,
            };
            let mask = 1u32 << bit;
            if is_unused_category {
                unused[word] & mask != 0
            } else {
                ty.category_id == category.id && known[word] & mask != 0
            }
        });
        if !has_known {
            continue;
        }
        let mut entries: Vec<_> = types
            .iter()
            .filter_map(|ty| {
                let (word, bit) = if ty.bit_index == 0 || ty.bit_index > 64 {
                    return None;
                } else if ty.bit_index <= 32 {
                    (0, ty.bit_index - 1)
                } else {
                    (1, ty.bit_index - 33)
                };
                let mask = 1u32 << bit;
                if known[word] & mask == 0 {
                    return None;
                }
                let unused_row = unused[word] & mask != 0;
                if is_unused_category {
                    if !unused_row {
                        return None;
                    }
                } else if ty.category_id != category.id || unused_row {
                    return None;
                }
                let (name, icon, count) = items(ty.item_id).unwrap_or_default();
                Some((
                    name.clone().unwrap_or_default(),
                    WrathCurrencyRow {
                        name,
                        item_id: ty.item_id,
                        category_id: ty.category_id,
                        is_watched: backpack[word] & mask != 0,
                        shown: unused_row,
                        icon,
                        count,
                        extra_type: match ty.item_id {
                            ARENA_ITEM => 1,
                            HONOR_ITEM => 2,
                            _ => 0,
                        },
                        bit_index: ty.bit_index,
                        ..Default::default()
                    },
                ))
            })
            .collect();
        entries.sort_by_key(|entry| entry.0.to_lowercase());
        let is_expanded = expanded(category.id);
        rows.push(WrathCurrencyRow {
            name: Some(category.name.clone()),
            category_id: category.id,
            is_header: true,
            is_expanded,
            shown: is_unused_category,
            ..Default::default()
        });
        if is_expanded {
            rows.extend(entries.into_iter().map(|(_, row)| row));
        }
    }
    let backpack_rows = types
        .iter()
        .filter_map(|ty| {
            let (word, bit) = if ty.bit_index == 0 || ty.bit_index > 64 {
                return None;
            } else if ty.bit_index <= 32 {
                (0, ty.bit_index - 1)
            } else {
                (1, ty.bit_index - 33)
            };
            let mask = 1u32 << bit;
            (known[word] & mask != 0 && backpack[word] & mask != 0).then(|| {
                let (name, icon, count) = items(ty.item_id).unwrap_or_default();
                WrathCurrencyRow {
                    name,
                    item_id: ty.item_id,
                    category_id: ty.category_id,
                    is_watched: true,
                    shown: true,
                    icon,
                    count,
                    extra_type: match ty.item_id {
                        ARENA_ITEM => 1,
                        HONOR_ITEM => 2,
                        _ => 0,
                    },
                    bit_index: ty.bit_index,
                    ..Default::default()
                }
            })
        })
        .collect();
    WrathCurrencyState {
        rows,
        backpack: backpack_rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(item_id: u32, category_id: u32, bit_index: u32) -> CurrencyTypeInput {
        CurrencyTypeInput {
            item_id,
            category_id,
            bit_index,
        }
    }

    fn category(id: u32, flags: u32, name: &str) -> CurrencyCategoryInput {
        CurrencyCategoryInput {
            id,
            flags,
            name: name.into(),
        }
    }

    fn items(entry: u32) -> Option<(Option<String>, Option<String>, u32)> {
        Some((Some(format!("item-{entry}")), None, entry))
    }

    #[test]
    fn ordinary_rows_sort_by_item_name_and_expand_under_headers() {
        let state = build_currency_state(
            [ty(20, 1, 2), ty(10, 1, 1)],
            [category(1, 0, "Tokens"), category(2, 1, "Unused")],
            [0b11, 0],
            [0, 0],
            [0, 0],
            |_| true,
            items,
        );
        assert_eq!(state.rows[0].name.as_deref(), Some("Tokens"));
        assert_eq!(state.rows[1].item_id, 10);
        assert_eq!(state.rows[2].item_id, 20);
    }

    #[test]
    fn unused_mask_creates_the_final_header_without_known_bits() {
        let state = build_currency_state(
            [ty(30, 1, 3)],
            [category(1, 0, "Tokens"), category(2, 1, "Unused")],
            [0, 0],
            [0b100, 0],
            [0, 0],
            |_| true,
            items,
        );
        assert_eq!(state.rows.len(), 1);
        assert!(state.rows[0].is_header);
        assert!(state.rows[0].shown);
    }

    #[test]
    fn backpack_rows_keep_dbc_order_and_watched_bits() {
        let state = build_currency_state(
            [ty(20, 1, 2), ty(10, 1, 1)],
            [category(1, 0, "Tokens")],
            [0b11, 0],
            [0, 0],
            [0b10, 0],
            |_| true,
            items,
        );
        assert_eq!(state.backpack.len(), 1);
        assert_eq!(state.backpack[0].item_id, 20);
        assert!(state.backpack[0].is_watched);
    }
}
