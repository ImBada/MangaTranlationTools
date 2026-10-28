//! Translation block groups (`blockGroups.ts`, `blockGroupEffects.ts`).

use serde_json::Value;

use crate::geometry::is_hex_color;
use crate::model::{PageRecord, TranslationBlock, TranslationBlockGroup, TranslationBlockGroupEffect};

pub const DROP_SHADOW_EFFECT_TYPE: &str = "drop-shadow";

#[derive(Debug, Clone, PartialEq)]
pub struct DropShadowSettings {
    pub angle_deg: f64,
    pub blur_px: f64,
    pub color: String,
    pub distance_px: f64,
    pub opacity: f64,
}

impl Default for DropShadowSettings {
    fn default() -> Self {
        Self {
            angle_deg: 45.0,
            blur_px: 0.0,
            color: "#000000".into(),
            distance_px: crate::font_presets::DEFAULT_ENABLED_SHADOW_DISTANCE_PX,
            opacity: 1.0,
        }
    }
}

/// Groups with at least two existing, de-duplicated block ids (`resolveValidTranslationBlockGroups`).
pub fn valid_groups(page: &PageRecord) -> Vec<TranslationBlockGroup> {
    let ids: std::collections::HashSet<&str> = page.blocks.iter().map(|block| block.id.as_str()).collect();
    page.block_groups
        .iter()
        .flatten()
        .filter_map(|group| {
            let mut seen = std::collections::HashSet::new();
            let block_ids: Vec<String> = group
                .block_ids
                .iter()
                .filter(|id| ids.contains(id.as_str()) && seen.insert(id.as_str()))
                .cloned()
                .collect();
            (block_ids.len() >= 2).then(|| TranslationBlockGroup { block_ids, ..group.clone() })
        })
        .collect()
}

pub enum ListItem<'a> {
    Block { block: &'a TranslationBlock, index: usize },
    Group { group: TranslationBlockGroup, blocks: Vec<(usize, &'a TranslationBlock)> },
}

/// Blocks in page order with grouped blocks collapsed at their first member
/// (`resolveTranslationBlockListItems`).
pub fn list_items(page: &PageRecord) -> Vec<ListItem<'_>> {
    let groups = valid_groups(page);
    let mut group_of: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (group_index, group) in groups.iter().enumerate() {
        for id in &group.block_ids {
            group_of.entry(id.as_str()).or_insert(group_index);
        }
    }
    let mut rendered = vec![false; groups.len()];
    let mut items = Vec::new();
    for (index, block) in page.blocks.iter().enumerate() {
        match group_of.get(block.id.as_str()) {
            None => items.push(ListItem::Block { block, index }),
            Some(&group_index) if !rendered[group_index] => {
                rendered[group_index] = true;
                let group = groups[group_index].clone();
                let blocks = group
                    .block_ids
                    .iter()
                    .filter_map(|id| page.blocks.iter().position(|b| &b.id == id).map(|i| (i, &page.blocks[i])))
                    .collect();
                items.push(ListItem::Group { group, blocks });
            }
            Some(_) => {}
        }
    }
    items
}

/// Blocks of the group containing `block_id` (`resolveTranslationBlockGroupBlockIds`).
pub fn group_block_ids(page: &PageRecord, block_id: &str) -> Option<Vec<String>> {
    valid_groups(page)
        .into_iter()
        .find(|group| group.block_ids.iter().any(|id| id == block_id))
        .map(|group| group.block_ids)
}

/// The group whose members are exactly `selected` (any order).
pub fn selected_group(page: &PageRecord, selected: &[String]) -> Option<TranslationBlockGroup> {
    let wanted: std::collections::HashSet<&str> = selected.iter().map(String::as_str).collect();
    valid_groups(page).into_iter().find(|group| {
        group.block_ids.len() == wanted.len() && group.block_ids.iter().all(|id| wanted.contains(id.as_str()))
    })
}

/// `resolveTranslationBlockGroupsAfterGrouping`: groups the selected blocks (page order),
/// taking them out of other groups. `None` when fewer than two blocks are selected.
pub fn groups_after_grouping(
    page: &PageRecord,
    selected: &[String],
    group_id: &str,
    now: &str,
) -> Option<Vec<TranslationBlockGroup>> {
    let ids: Vec<String> = page.blocks.iter().filter(|b| selected.contains(&b.id)).map(|b| b.id.clone()).collect();
    if ids.len() < 2 {
        return None;
    }
    let groups = valid_groups(page);
    if let Some(existing) = selected_group(page, &ids) {
        return Some(
            groups
                .into_iter()
                .map(|group| {
                    if group.id == existing.id && group.block_ids != ids {
                        TranslationBlockGroup { block_ids: ids.clone(), updated_at: now.to_owned(), ..group }
                    } else {
                        group
                    }
                })
                .collect(),
        );
    }
    let mut next: Vec<TranslationBlockGroup> = groups
        .into_iter()
        .filter_map(|group| {
            let remaining: Vec<String> = group.block_ids.iter().filter(|id| !ids.contains(id)).cloned().collect();
            (remaining.len() >= 2).then(|| {
                let updated_at =
                    if remaining.len() == group.block_ids.len() { group.updated_at.clone() } else { now.to_owned() };
                TranslationBlockGroup { block_ids: remaining, updated_at, ..group }
            })
        })
        .collect();
    next.push(TranslationBlockGroup {
        id: group_id.to_owned(),
        block_ids: ids,
        effects: Vec::new(),
        created_at: now.to_owned(),
        updated_at: now.to_owned(),
        ..TranslationBlockGroup::default()
    });
    Some(next)
}

/// `resolveTranslationBlockGroupsAfterUngrouping`: removes the group the selection
/// belongs to. `None` when the selection is not a group.
pub fn groups_after_ungrouping(page: &PageRecord, selected: &[String]) -> Option<Option<Vec<TranslationBlockGroup>>> {
    let group = selected_group(page, selected).or_else(|| {
        selected.first().and_then(|id| valid_groups(page).into_iter().find(|g| g.block_ids.contains(id)))
    })?;
    let rest: Vec<TranslationBlockGroup> = valid_groups(page).into_iter().filter(|g| g.id != group.id).collect();
    Some((!rest.is_empty()).then_some(rest))
}

/// Enables/updates the drop shadow effect (`upsertTranslationBlockGroupDropShadowEffect`).
pub fn set_drop_shadow(group: &mut TranslationBlockGroup, enabled: bool, settings: &DropShadowSettings) {
    let value = serde_json::json!({
        "angleDeg": settings.angle_deg.clamp(-360.0, 360.0),
        "blurPx": settings.blur_px.clamp(0.0, 80.0),
        "color": if is_hex_color(&settings.color) { settings.color.clone() } else { "#000000".into() },
        "distancePx": settings.distance_px.clamp(0.0, 80.0),
        "opacity": settings.opacity.clamp(0.0, 1.0),
    });
    let map = value.as_object().cloned();
    match group.effects.iter_mut().find(|effect| effect.effect_type == DROP_SHADOW_EFFECT_TYPE) {
        Some(effect) => {
            effect.enabled = enabled;
            effect.settings = map;
        }
        None if enabled => group.effects.push(TranslationBlockGroupEffect {
            id: format!("text-block-group-effect-{}", uuid::Uuid::new_v4()),
            effect_type: DROP_SHADOW_EFFECT_TYPE.into(),
            enabled,
            settings: map,
            ..Default::default()
        }),
        None => {}
    }
}

pub fn drop_shadow_effect(group: &TranslationBlockGroup) -> Option<&TranslationBlockGroupEffect> {
    group.effects.iter().find(|effect| effect.effect_type == DROP_SHADOW_EFFECT_TYPE)
}

/// `resolveTranslationBlockGroupDropShadowSettings`, including legacy `offsetX/offsetY`.
pub fn drop_shadow_settings(effect: Option<&TranslationBlockGroupEffect>) -> DropShadowSettings {
    let defaults = DropShadowSettings::default();
    let settings = effect.and_then(|effect| effect.settings.as_ref());
    let get = |key: &str| settings.and_then(|map| map.get(key));
    let offset_x = get("offsetX").and_then(finite_number);
    let offset_y = get("offsetY").and_then(finite_number);
    let legacy = (offset_x.is_some() || offset_y.is_some()).then(|| {
        let (dx, dy) = (offset_x.unwrap_or(0.0), offset_y.unwrap_or(0.0));
        (dy.atan2(dx).to_degrees(), dx.hypot(dy))
    });
    DropShadowSettings {
        angle_deg: read_number(get("angleDeg"), legacy.map(|l| l.0).unwrap_or(defaults.angle_deg), -360.0, 360.0),
        blur_px: read_number(get("blurPx"), defaults.blur_px, 0.0, 80.0),
        color: get("color")
            .and_then(Value::as_str)
            .filter(|c| is_hex_color(c))
            .map(str::to_owned)
            .unwrap_or(defaults.color),
        distance_px: read_number(get("distancePx"), legacy.map(|l| l.1).unwrap_or(defaults.distance_px), 0.0, 80.0),
        opacity: read_number(get("opacity"), defaults.opacity, 0.0, 1.0),
    }
}

fn finite_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|n: &f64| n.is_finite())
}

fn read_number(value: Option<&Value>, fallback: f64, min: f64, max: f64) -> f64 {
    value.and_then(finite_number).map(|n| n.clamp(min, max)).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page_with_groups() -> PageRecord {
        let block = |id: &str| TranslationBlock { id: id.into(), ..TranslationBlock::default() };
        PageRecord {
            blocks: vec![block("a"), block("b"), block("c"), block("d")],
            block_groups: Some(vec![
                TranslationBlockGroup {
                    id: "g1".into(),
                    block_ids: vec!["c".into(), "a".into(), "a".into()],
                    ..Default::default()
                },
                TranslationBlockGroup {
                    id: "g2".into(),
                    block_ids: vec!["d".into(), "missing".into()],
                    ..Default::default()
                },
            ]),
            ..PageRecord::default()
        }
    }

    #[test]
    fn invalid_groups_are_dropped_and_duplicates_removed() {
        let groups = valid_groups(&page_with_groups());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].block_ids, ["c", "a"]);
    }

    #[test]
    fn list_items_collapse_group_at_first_member() {
        let page = page_with_groups();
        let kinds: Vec<String> = list_items(&page)
            .iter()
            .map(|item| match item {
                ListItem::Block { block, .. } => block.id.clone(),
                ListItem::Group { group, .. } => format!("[{}]", group.id),
            })
            .collect();
        assert_eq!(kinds, ["[g1]", "b", "d"]);
    }

    #[test]
    fn grouping_takes_blocks_out_of_other_groups() {
        let page = page_with_groups();
        let next = groups_after_grouping(&page, &["a".into(), "b".into()], "g3", "now").unwrap();
        // g1 (c, a) loses a and has one member left, so it disappears.
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].block_ids, ["a", "b"]);
        assert!(groups_after_grouping(&page, &["a".into()], "g4", "now").is_none());

        let page = PageRecord { block_groups: Some(next), ..page };
        assert_eq!(group_block_ids(&page, "b").unwrap(), ["a", "b"]);
        assert!(selected_group(&page, &["b".into(), "a".into()]).is_some());
        assert_eq!(groups_after_ungrouping(&page, &["a".into(), "b".into()]), Some(None));
    }

    #[test]
    fn drop_shadow_upsert() {
        let mut group = TranslationBlockGroup::default();
        set_drop_shadow(&mut group, false, &DropShadowSettings::default());
        assert!(group.effects.is_empty());
        let settings = DropShadowSettings { blur_px: 500.0, ..DropShadowSettings::default() };
        set_drop_shadow(&mut group, true, &settings);
        let effect = drop_shadow_effect(&group).unwrap();
        assert!(effect.enabled);
        assert_eq!(drop_shadow_settings(Some(effect)).blur_px, 80.0);
    }

    #[test]
    fn legacy_offsets_become_angle_and_distance() {
        let effect = TranslationBlockGroupEffect {
            effect_type: DROP_SHADOW_EFFECT_TYPE.into(),
            enabled: true,
            settings: Some(json!({ "offsetX": 3, "offsetY": 4, "color": "nope" }).as_object().unwrap().clone()),
            ..Default::default()
        };
        let settings = drop_shadow_settings(Some(&effect));
        assert!((settings.distance_px - 5.0).abs() < 1e-9);
        assert!((settings.angle_deg - 53.130102).abs() < 1e-4);
        assert_eq!(settings.color, "#000000");
    }
}
