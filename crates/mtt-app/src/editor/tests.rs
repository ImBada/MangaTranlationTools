use std::path::PathBuf;

use image::{GrayImage, RgbaImage};
use mtt_core::model::InpaintSettings;

use super::inpaint::{ComputedInpaint, compute_inpaint};
use super::*;

fn chapter_with_pages(count: usize) -> Chapter {
    let pages = (0..count)
        .map(|index| PageRecord {
            id: format!("p{index}"),
            width: 1000,
            height: 1000,
            blocks: vec![TranslationBlock {
                id: format!("b{index}"),
                bbox: BBox::new(100.0, 100.0, 200.0, 100.0),
                ..TranslationBlock::default()
            }],
            ..PageRecord::default()
        })
        .collect::<Vec<_>>();
    Chapter {
        id: "c".into(),
        work_id: "w".into(),
        page_order: pages.iter().map(|p| p.id.clone()).collect(),
        pages,
        ..Chapter::default()
    }
}

#[test]
fn edits_are_undoable_per_commit() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    editor.edit_selected(|block| block.translated_text = "안".into());
    editor.edit_selected(|block| block.translated_text = "안녕".into());
    editor.commit_checkpoint();
    editor.delete_selected();
    assert!(editor.page().unwrap().blocks.is_empty());

    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "안녕");
    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "", "typing merged into one step");
    editor.redo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "안녕");
    assert!(editor.chapter_dirty);
}

#[test]
fn continuous_edits_while_pointer_down_are_one_step() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    let paths = mtt_core::paths::AppPaths::from_data_root(std::env::temp_dir().join("mtt-tick-test"));
    let mut services = Services::new(&ctx, paths);
    let worker = Worker::new(&ctx, 1);
    for size in [10.0, 12.0, 14.0] {
        editor.edit_selected_style(StyleKey::FontSizePx, |block| block.style.font_size_px = Some(size));
        editor.tick(&mut services, &worker, true);
    }
    editor.tick(&mut services, &worker, false);
    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].style.font_size_px, None, "one undo reverts the whole drag");
}

#[test]
fn undo_switches_to_the_edited_page() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(3));
    editor.go_to_page(2);
    editor.toggle_page_completed();
    editor.go_to_page(0);
    editor.undo(&ctx);
    assert_eq!(editor.current, 2);
    assert!(!editor.page().unwrap().is_completed());
}

#[test]
fn style_edits_without_preset_change_only_the_block() {
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    editor.edit_selected_style(StyleKey::TextColor, |block| block.style.text_color = Some("#ff0000".into()));
    let block = &editor.page().unwrap().blocks[0];
    assert_eq!(block.style.text_color.as_deref(), Some("#ff0000"));
    assert_eq!(block.links.text_color_linked_to_preset, None);
    assert!(editor.chapter.font_presets.is_none(), "no preset created");
}

#[test]
fn duplicate_and_paste_create_new_ids() {
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    editor.duplicate_selected();
    assert_eq!(editor.page().unwrap().blocks.len(), 2);
    assert_ne!(editor.selection[0], "b0");
    editor.copy_selected(&egui::Context::default());
    editor.paste(None);
    let ids: HashSet<&str> = editor.page().unwrap().blocks.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids.len(), 3);
}

#[test]
fn range_inpaint_merges_into_existing_layers() {
    let source =
        RgbaImage::from_fn(
            60,
            60,
            |x, _| {
                if x < 30 { image::Rgba([200, 0, 0, 255]) } else { image::Rgba([0, 0, 200, 255]) }
            },
        );
    let mut mask = GrayImage::new(60, 60);
    for (x, y) in [(10, 10), (50, 50)] {
        mask.put_pixel(x, y, image::Luma([255]));
    }
    let mut existing = RgbaImage::new(60, 60);
    existing.put_pixel(50, 50, image::Rgba([1, 2, 3, 255]));
    let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
    let outcome = compute_inpaint(
        InpaintScope::Range,
        &source,
        Some(Arc::new(mask)),
        Some(Arc::new(existing)),
        Some(mtt_inpaint::Rect { x: 0, y: 0, width: 30, height: 30 }),
        None,
        &[],
        InpaintEngine::LocalFillFallback,
        &settings,
        None,
        None,
    )
    .unwrap();
    assert_eq!(outcome.result.get_pixel(10, 10)[3], 255, "range pixel inpainted");
    assert_eq!(outcome.result.get_pixel(50, 50).0, [1, 2, 3, 255], "outside range kept");
    assert_eq!(outcome.mask.get_pixel(50, 50)[0], 255, "existing mask kept");

    let empty_range = compute_inpaint(
        InpaintScope::Range,
        &source,
        Some(Arc::new(GrayImage::new(60, 60))),
        None,
        Some(mtt_inpaint::Rect { x: 0, y: 0, width: 10, height: 10 }),
        None,
        &[],
        InpaintEngine::LocalFillFallback,
        &settings,
        None,
        None,
    );
    assert!(empty_range.is_err());
}

#[test]
fn heal_fills_the_stroke_and_keeps_the_rest() {
    // A dark speck on white paper; the stroke covers only the speck.
    let source = RgbaImage::from_fn(40, 40, |x, y| {
        if (18..22).contains(&x) && (18..22).contains(&y) {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let mut stroke = GrayImage::new(40, 40);
    crate::editor::assets::stroke_mask(&mut stroke, (20.0, 20.0), (20.0, 20.0), 10.0, 255);
    let mut mask = GrayImage::new(40, 40);
    mask.put_pixel(2, 2, image::Luma([255]));
    let mut existing = RgbaImage::new(40, 40);
    existing.put_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
    let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
    let (mask, existing, stroke) = (Arc::new(mask), Arc::new(existing), Arc::new(stroke));
    let heal = |keep_mask| {
        let stroke = HealStroke { page_id: "p".into(), mask: Arc::clone(&stroke), keep_mask };
        compute_inpaint(
            InpaintScope::Heal,
            &source,
            Some(Arc::clone(&mask)),
            Some(Arc::clone(&existing)),
            None,
            Some(&stroke),
            &[],
            InpaintEngine::LocalFillFallback,
            &settings,
            None,
            None,
        )
        .unwrap()
    };
    for keep_mask in [false, true] {
        let outcome = heal(keep_mask);
        let healed = outcome.result.get_pixel(20, 20);
        assert!(healed[3] == 255 && healed[0] > 200, "speck filled with paper: {healed:?}");
        assert_eq!(outcome.result.get_pixel(2, 2).0, [1, 2, 3, 255], "earlier result kept");
        assert_eq!(outcome.mask.get_pixel(2, 2)[0], 255, "earlier mask kept");
        let joined = if keep_mask { 0 } else { 255 };
        assert_eq!(outcome.mask.get_pixel(20, 20)[0], joined, "the stroke joins the mask unless kept");
    }

    let empty = compute_inpaint(
        InpaintScope::Heal,
        &source,
        None,
        None,
        None,
        Some(&HealStroke { page_id: "p".into(), mask: Arc::new(GrayImage::new(40, 40)), keep_mask: false }),
        &[],
        InpaintEngine::LocalFillFallback,
        &settings,
        None,
        None,
    );
    assert!(empty.is_err());
}

#[test]
fn partial_inpaint_sees_what_was_already_erased() {
    // A dark column, erased below row 10 already; re-inpainting the top half must not
    // pull the dark text back in from below.
    let source = RgbaImage::from_fn(20, 20, |x, _| {
        if (8..13).contains(&x) { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
    });
    let mut mask = GrayImage::new(20, 20);
    let mut existing = RgbaImage::new(20, 20);
    for y in 0..20 {
        for x in 8..13 {
            mask.put_pixel(x, y, image::Luma([255]));
            if y >= 10 {
                existing.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
    }
    let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
    let outcome = compute_inpaint(
        InpaintScope::Range,
        &source,
        Some(Arc::new(mask)),
        Some(Arc::new(existing)),
        Some(mtt_inpaint::Rect { x: 0, y: 0, width: 20, height: 10 }),
        None,
        &[],
        InpaintEngine::LocalFillFallback,
        &settings,
        None,
        None,
    )
    .unwrap();
    assert_eq!(outcome.result.get_pixel(10, 9).0, [255, 255, 255, 255]);
}

#[test]
fn inpaint_result_is_one_undo_step() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    let page_id = editor.page().unwrap().id.clone();
    let source = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
    editor
        .assets
        .insert(page_id.clone(), PageAssets::Ready(Box::new(LoadedPage::new(&ctx, &page_id, source, None, None))));
    editor.inpaint_running = Some(page_id.clone());
    let mut mask = GrayImage::new(10, 10);
    mask.put_pixel(3, 3, image::Luma([255]));
    let mut result = RgbaImage::new(10, 10);
    result.put_pixel(3, 3, image::Rgba([9, 9, 9, 255]));
    let computed = ComputedInpaint {
        before_mask: None,
        before_result: None,
        mask,
        result,
        engine: InpaintEngine::LocalFillFallback,
        message: "ok".into(),
    };
    editor.on_inpaint_finished("c", &page_id, Ok(computed.finish(&ctx, &page_id)));
    assert!(!editor.inpaint_busy());
    assert_eq!(editor.page().unwrap().inpaint_status, Some(InpaintStatus::Completed));
    editor.undo(&ctx);
    let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
    assert_eq!(loaded.mask.as_ref().unwrap().get_pixel(3, 3)[0], 0);
    assert_eq!(loaded.result.as_ref().unwrap().get_pixel(3, 3)[3], 0);
}

#[test]
fn layer_undo_waits_for_a_running_inpaint() {
    let ctx = egui::Context::default();
    let worker: Worker<AppEvent> = Worker::new(&ctx, 1);
    let mut editor = Editor::new(chapter_with_pages(1));
    let source = RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));
    editor.assets.insert("p0".into(), PageAssets::Ready(Box::new(LoadedPage::new(&ctx, "p0", source, None, None))));
    editor.brush_size = 10.0;
    editor.paint_stroke(&ctx, (20.0, 20.0), (40.0, 20.0), false);
    editor.start_inpaint(InpaintScope::WholeMask, &worker);
    assert!(editor.inpaint_busy());
    // Block steps still undo; the stroke waits, or the result would bring it back.
    editor.selection = vec!["b0".into()];
    editor.edit_selected(|block| block.translated_text = "x".into());
    editor.commit_checkpoint();
    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
    editor.undo(&ctx);
    assert!(editor.current_mask_has_pixels(), "the stroke stays while the job runs");
    assert!(editor.status.starts_with("인페인트 작업이 끝난 뒤에"));
    editor.redo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "x", "redo isn't stuck behind it");
    let deadline = Instant::now() + Duration::from_secs(30);
    while editor.inpaint_busy() && Instant::now() < deadline {
        for event in worker.drain() {
            if let AppEvent::InpaintFinished { chapter_id, page_id, outcome } = event {
                editor.on_inpaint_finished(&chapter_id, &page_id, outcome);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!editor.inpaint_busy());
    editor.undo(&ctx);
    editor.undo(&ctx);
    editor.undo(&ctx);
    assert!(!editor.current_mask_has_pixels(), "the inpaint, the text edit, then the stroke");
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
}

#[test]
fn psd_export_import_round_trip() {
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    let mut services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir.path()));
    let started = Instant::now();
    while services.fonts().is_none() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut editor = Editor::new(chapter_with_pages(1));
    let page_id = editor.page().unwrap().id.clone();
    editor.page_mut().unwrap().width = 8;
    editor.page_mut().unwrap().height = 6;
    let source = RgbaImage::from_pixel(8, 6, image::Rgba([10, 20, 30, 255]));
    let mut mask = GrayImage::new(8, 6);
    mask.put_pixel(2, 2, image::Luma([255]));
    let mut result = RgbaImage::new(8, 6);
    result.put_pixel(2, 2, image::Rgba([200, 150, 100, 255]));
    result.put_pixel(5, 5, image::Rgba([1, 1, 1, 255])); // hidden by the mask
    editor.assets.insert(
        page_id.clone(),
        PageAssets::Ready(Box::new(LoadedPage::new(&ctx, &page_id, source, Some(mask), Some(result)))),
    );

    let path = dir.path().join("page.psd");
    let bytes = editor.render_psd(&mut services).unwrap();
    std::fs::write(&path, bytes).unwrap();
    if let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page_id) {
        loaded.mask = None;
        loaded.result = None;
    }
    editor.import_psd(&ctx, &path).unwrap();
    let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
    assert_eq!(loaded.result.as_ref().unwrap().get_pixel(2, 2).0, [200, 150, 100, 255]);
    assert_eq!(loaded.result.as_ref().unwrap().get_pixel(5, 5)[3], 0, "masked-out pixel not exported");
    assert_eq!(loaded.mask.as_ref().unwrap().get_pixel(2, 2)[0], 255);
    editor.undo(&ctx);
    let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
    assert_eq!(loaded.result.as_ref().unwrap().get_pixel(2, 2)[3], 0, "undo restores the cleared state");
}

#[test]
fn psd_export_adds_hidden_editable_text_that_import_skips() {
    use mtt_inpaint::psd::{TEXT_GROUP_NAME, import_inpaint_layers, read_psd};
    let ctx = egui::Context::default();
    let dir = tempfile::tempdir().unwrap();
    let mut services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir.path()));
    let started = Instant::now();
    while services.fonts().is_none() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut editor = Editor::new(chapter_with_pages(1));
    let page_id = editor.page().unwrap().id.clone();
    let page = editor.page_mut().unwrap();
    (page.width, page.height) = (400, 300);
    page.blocks[0].translated_text = "안녕하세요".into();
    page.blocks.push(TranslationBlock {
        id: "hidden".into(),
        translated_text: "숨김".into(),
        render_direction: mtt_core::model::RenderTextDirection::Hidden,
        ..page.blocks[0].clone()
    });
    let source = RgbaImage::from_pixel(400, 300, image::Rgba([250, 250, 250, 255]));
    let mut mask = GrayImage::new(400, 300);
    mask.put_pixel(50, 40, image::Luma([255]));
    let mut result = RgbaImage::new(400, 300);
    result.put_pixel(50, 40, image::Rgba([200, 150, 100, 255]));
    editor.assets.insert(
        page_id.clone(),
        PageAssets::Ready(Box::new(LoadedPage::new(&ctx, &page_id, source, Some(mask), Some(result)))),
    );

    let with_text = read_psd(&editor.render_psd(&mut services).unwrap()).unwrap();
    let texts: Vec<_> = with_text.layers.iter().filter(|layer| layer.text).collect();
    assert_eq!(texts.len(), 1, "one visible block");
    assert_eq!(texts[0].name, "안녕하세요");
    assert!(texts[0].hidden, "the group is hidden");
    assert_eq!(texts[0].groups, [TEXT_GROUP_NAME]);
    assert!(texts[0].image.pixels().any(|pixel| pixel[3] > 0), "the block's raster");
    editor.page_mut().unwrap().blocks.clear();
    let without = read_psd(&editor.render_psd(&mut services).unwrap()).unwrap();
    assert!(without.layers.iter().all(|layer| !layer.text && layer.groups.is_empty()), "no empty group");
    assert_eq!(import_inpaint_layers(&with_text, 400, 300), import_inpaint_layers(&without, 400, 300));
}

#[test]
fn group_and_ungroup_are_undoable() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    editor.duplicate_selected();
    editor.select_all();
    editor.group_selected();
    assert_eq!(editor.page().unwrap().block_groups.as_ref().unwrap().len(), 1);
    editor.set_group_drop_shadow(true, &mtt_core::groups::DropShadowSettings::default());
    editor.commit_checkpoint();
    let group = editor.page().unwrap().block_groups.as_ref().unwrap()[0].clone();
    assert!(mtt_core::groups::drop_shadow_effect(&group).unwrap().enabled);
    editor.ungroup_selected();
    assert!(editor.page().unwrap().block_groups.is_none());
    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().block_groups.as_ref().unwrap().len(), 1);
}

#[test]
fn paste_applies_copied_font_settings_or_pastes_blocks() {
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    editor.edit_style(|block| block.style.text_color = Some("#00ff00".into()));
    editor.copy_font_settings(&egui::Context::default());
    editor.duplicate_selected();
    let copy_id = editor.selection[0].clone();
    editor.edit_style(|block| block.style.text_color = Some("#0000ff".into()));
    editor.paste(None);
    let copy = editor.page().unwrap().blocks.iter().find(|b| b.id == copy_id).unwrap();
    assert_eq!(copy.style.text_color.as_deref(), Some("#00ff00"), "font settings applied");
    assert_eq!(editor.page().unwrap().blocks.len(), 2, "no block pasted");

    editor.copy_selected(&egui::Context::default());
    editor.paste(None);
    assert_eq!(editor.page().unwrap().blocks.len(), 3, "block copy clears the font clipboard");
}

#[test]
fn inline_edit_commit_and_cancel() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.set_layer(Layer::InpaintMask);
    editor.start_inline_edit("b0");
    assert!(editor.layer.edits_blocks(), "switches to a block layer");
    editor.inline_edit.as_mut().unwrap().draft = "새 문장".into();
    editor.cancel_inline_edit();
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
    editor.start_inline_edit("b0");
    editor.inline_edit.as_mut().unwrap().draft = "새 문장".into();
    editor.commit_inline_edit();
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "새 문장");
    editor.undo(&ctx);
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
}

fn services_in(dir: &std::path::Path) -> (egui::Context, Services, Worker<AppEvent>) {
    let ctx = egui::Context::default();
    let services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir));
    let worker = Worker::new(&ctx, 1);
    (ctx, services, worker)
}

fn page_with_mask(ctx: &egui::Context, page_id: &str) -> PageAssets {
    let mut mask = GrayImage::new(8, 8);
    mask.put_pixel(1, 1, image::Luma([255]));
    PageAssets::Ready(Box::new(LoadedPage::new(ctx, page_id, RgbaImage::new(8, 8), Some(mask), None)))
}

/// Makes writing a page's mask PNG fail: a folder sits where the file goes.
fn block_mask_path(services: &Services, editor: &Editor, page_id: &str) -> PathBuf {
    let path = services.library.inpaint_layer_path(&editor.chapter, page_id, LayerKind::Mask.page_layer());
    std::fs::create_dir_all(path.join("x")).unwrap();
    path
}

#[test]
fn flush_keeps_the_text_typed_on_the_canvas() {
    let dir = tempfile::tempdir().unwrap();
    let (_ctx, mut services, _worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.start_inline_edit("b0");
    editor.inline_edit.as_mut().unwrap().draft = "고친 문장".into();
    editor.flush(&mut services).unwrap();
    assert!(editor.inline_edit.is_none());
    let saved = services.library.read_chapter("w", "c").unwrap().unwrap();
    assert_eq!(saved.pages[0].blocks[0].translated_text, "고친 문장");
}

#[test]
fn a_failed_flush_keeps_every_unwritten_layer() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, mut services, _worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(2));
    let mut paths = Vec::new();
    for id in ["p0", "p1"] {
        editor.assets.insert(id.into(), page_with_mask(&ctx, id));
        editor.layer_dirty.insert((id.into(), LayerKind::Mask), Instant::now());
        paths.push(block_mask_path(&services, &editor, id));
    }
    assert!(editor.flush(&mut services).is_err());
    assert_eq!(editor.layer_dirty.len(), 2, "both layers are still to save");
    assert!(editor.has_unsaved_changes());
    assert!(editor.notifications.has_save_failure(), "the layer save card is up");
    // A background write the flush made stale doesn't count as saved.
    editor.on_layer_saved("p0", LayerKind::Mask, None, true);
    assert!(editor.notifications.has_save_failure());
    for path in &paths {
        std::fs::remove_dir_all(path).unwrap();
    }
    editor.flush(&mut services).expect("the next flush writes them");
    assert!(paths.iter().all(|path| path.is_file()));
    assert!(!editor.has_unsaved_changes());
    assert!(!editor.notifications.has_save_failure());
}

#[test]
fn a_failed_background_layer_save_stays_dirty_until_written() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, mut services, worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.assets.insert("p0".into(), page_with_mask(&ctx, "p0"));
    let key = ("p0".to_owned(), LayerKind::Mask);
    editor.layer_dirty.insert(key.clone(), Instant::now() - LAYER_SAVE_DELAY);
    let path = block_mask_path(&services, &editor, "p0");
    editor.tick(&mut services, &worker, false);
    assert!(editor.layer_dirty.is_empty(), "handed to the writer");
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut answered = false;
    while !answered && Instant::now() < deadline {
        for event in worker.drain() {
            if let AppEvent::LayerSaved { page_id, layer, error, skipped } = event {
                assert!(error.is_some(), "the write fails");
                editor.on_layer_saved(&page_id, layer, error, skipped);
                answered = true;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(answered);
    assert!(editor.layer_dirty.contains_key(&key), "still unsaved, so the page stays loaded");
    assert!(editor.has_unsaved_changes());
    assert!(editor.layer_save_delay(&key) > LAYER_SAVE_DELAY, "retried after a backoff");
    std::fs::remove_dir_all(&path).unwrap();
    editor.flush(&mut services).expect("leaving the chapter writes it");
    assert!(path.is_file());
    assert!(!editor.layer_dirty.contains_key(&key));
    assert!(!editor.notifications.has_save_failure());
}

#[test]
fn a_new_layer_file_is_recorded_in_the_chapter_right_behind_it() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, mut services, worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.assets.insert("p0".into(), page_with_mask(&ctx, "p0"));
    editor.layer_dirty.insert(("p0".into(), LayerKind::Mask), Instant::now() - LAYER_SAVE_DELAY);
    // One frame queues the PNG and then the chapter that points at it.
    editor.tick(&mut services, &worker, false);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut writes = Vec::new();
    while writes.len() < 2 && Instant::now() < deadline {
        for event in worker.drain() {
            match event {
                AppEvent::LayerSaved { error: None, .. } => writes.push("layer"),
                AppEvent::ChapterSaved { error: None, .. } => writes.push("chapter"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(writes, ["layer", "chapter"]);
    let saved = services.library.read_chapter("w", "c").unwrap().unwrap();
    let path = saved.pages[0].inpaint_mask_path.as_deref().expect("the chapter points at the mask");
    assert!(std::path::Path::new(path).is_file());
}

#[test]
fn opening_a_chapter_resets_jobs_cut_short_by_quitting() {
    let mut chapter = chapter_with_pages(2);
    chapter.pages[0].analysis_status = PageAnalysisStatus::Running;
    chapter.pages[1].analysis_status = PageAnalysisStatus::Completed;
    chapter.pages[1].inpaint_status = Some(InpaintStatus::Running);
    chapter.status = ChapterStatus::Running;
    let editor = Editor::new(chapter);
    assert_eq!(editor.chapter.pages[0].analysis_status, PageAnalysisStatus::Idle, "waits for translation");
    assert_eq!(editor.chapter.pages[1].inpaint_status, Some(InpaintStatus::Failed));
    assert_eq!(editor.chapter.status, ChapterStatus::Partial);
    assert!(editor.chapter_dirty, "saved, so the library stops showing it as running");
    assert!(!Editor::new(chapter_with_pages(1)).chapter_dirty, "nothing to reset, nothing to save");
}

#[test]
fn pending_saves_schedule_a_frame() {
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.selection = vec!["b0".into()];
    assert_eq!(editor.next_tick_due(), None, "nothing to save");
    editor.edit_selected(|block| block.translated_text = "x".into());
    let wait = editor.next_tick_due().expect("the autosave needs a frame");
    assert!(wait <= AUTOSAVE_DELAY + Duration::from_millis(10));
    editor.layer_dirty.insert(("p0".into(), LayerKind::Mask), Instant::now() - LAYER_SAVE_DELAY);
    assert!(editor.next_tick_due().unwrap() <= Duration::from_millis(10), "an overdue layer save comes first");
}

#[test]
fn a_panicking_write_does_not_stop_later_saves() {
    let generation = WriteGeneration::default();
    let queued_at = generation.current();
    let panicked = catch_panic(|| generation.write_if_current(queued_at, || panic!("encoder bug")));
    assert!(panicked.is_err());
    let mut wrote = false;
    let result = generation.write_if_current(queued_at, || {
        wrote = true;
        Ok(())
    });
    assert_eq!(result, Ok(true));
    assert!(wrote, "the poisoned generation lock is recovered");
    generation.advance_while(|| ());
    assert_eq!(generation.write_if_current(queued_at, || Ok(())), Ok(false), "stale after a flush");
    assert_eq!(generation.current(), queued_at + 1);
}

#[test]
fn deleting_the_last_page_leaves_a_usable_editor() {
    let dir = tempfile::tempdir().unwrap();
    let (_ctx, mut services, worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.delete_page("p0", &mut services);
    assert_eq!(editor.page_count(), 0);
    editor.tick(&mut services, &worker, false);
    assert!(editor.page().is_none());
}

#[test]
fn preset_undo_survives_deleting_its_page() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, mut services, _worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(2));
    for page in &mut editor.chapter.pages {
        page.blocks[0].translated_text = "a x".into();
    }
    editor.reset_checkpoint();
    // A chapter-wide step recorded while page p0 is current.
    editor.replace_in_all_pages("x", "y");
    editor.delete_page("p0", &mut services);
    assert_eq!(editor.chapter.pages[0].blocks[0].translated_text, "a y");
    editor.undo(&ctx);
    assert_eq!(editor.chapter.pages[0].blocks[0].translated_text, "a x", "undo still applies");
    assert!(editor.history_can_redo_for_test());
}

#[test]
fn inpaint_queue_skips_deleted_pages() {
    let dir = tempfile::tempdir().unwrap();
    let (_ctx, _services, worker) = services_in(dir.path());
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.inpaint_queue = Some(InpaintQueue { pages: ["gone".to_owned()].into_iter().collect(), total: 1 });
    editor.advance_inpaint_queue(&worker);
    assert!(!editor.inpaint_busy());
}

#[test]
fn undo_restores_the_selection_of_the_step() {
    let ctx = egui::Context::default();
    let mut chapter = chapter_with_pages(1);
    let mut second = chapter.pages[0].blocks[0].clone();
    second.id = "b1".into();
    chapter.pages[0].blocks.push(second);
    let mut editor = Editor::new(chapter);
    editor.set_layer(Layer::Overlay);
    editor.selection = vec!["b0".into()];
    editor.tick_for_test();
    editor.edit_selected(|block| block.translated_text = "x".into());
    editor.commit_checkpoint();
    editor.selection = vec!["b1".into()];
    editor.undo(&ctx);
    assert_eq!(editor.selection, ["b0"], "undo brings back the block it changed");
    assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
}

#[test]
fn block_stepping_follows_output_order() {
    let mut chapter = chapter_with_pages(1);
    let page = &mut chapter.pages[0];
    page.blocks.push(TranslationBlock { id: "x".into(), ..TranslationBlock::default() });
    page.blocks.push(TranslationBlock { id: "y".into(), ..TranslationBlock::default() });
    let mut editor = Editor::new(chapter);
    editor.select_adjacent_block(1);
    assert_eq!(editor.selection, ["b0"], "nothing selected starts at the first block");
    editor.select_adjacent_block(1);
    editor.select_adjacent_block(1);
    editor.select_adjacent_block(1);
    assert_eq!(editor.selection, ["y"], "stops at the last block");
    editor.select_adjacent_block(-1);
    assert_eq!(editor.selection, ["x"]);
    editor.selection.clear();
    editor.select_adjacent_block(-1);
    assert_eq!(editor.selection, ["y"], "Shift+Tab with nothing selected starts at the end");
}

#[test]
fn review_check_off_moves_to_the_next_page_to_review() {
    let mut editor = Editor::new(chapter_with_pages(4));
    editor.chapter.pages[1].progress_completed = Some(true);
    editor.select_layer_shortcut(1);
    editor.toggle_page_completed();
    assert!(editor.chapter.pages[0].is_completed());
    assert_eq!(editor.current, 2, "page 2 is already reviewed, so it goes on to 3");
    editor.toggle_page_completed();
    editor.toggle_page_completed();
    assert_eq!(editor.current, 3, "the last page stays put");
    assert!(editor.chapter.pages[3].is_completed());
    editor.toggle_page_completed();
    assert!(!editor.chapter.pages[3].is_completed(), "unchecking stays on the page");
    assert_eq!(editor.current, 3);

    editor.select_layer_shortcut(2);
    editor.go_to_page(0);
    editor.toggle_page_completed();
    assert_eq!(editor.current, 0, "other modes stay on the page");
}

#[test]
fn layer_shortcuts_and_modes() {
    let mut editor = Editor::new(chapter_with_pages(1));
    editor.select_layer_shortcut(4);
    assert_eq!(editor.layer, Layer::InpaintMask);
    assert_eq!(editor.mode(), Mode::Inpaint);
    editor.toggle_keep_mask();
    assert!(editor.result_brush.keep_mask);
    editor.toggle_keep_mask();
    assert!(!editor.result_brush.keep_mask);
    assert_eq!(editor.tool, Tool::Select, "entering the mask layer keeps the pointer");
    assert_eq!(editor.opacity.inpaint_mask, 1.0);
    editor.select_shared_inpaint_tool(Tool::MaskBrush);
    editor.set_mode(Mode::Translate);
    assert_eq!(editor.layer, Layer::Overlay);
    editor.set_mode(Mode::Inpaint);
    assert_eq!(editor.layer, Layer::InpaintMask, "inpaint mode returns to the last inpaint layer");
    assert_eq!(editor.tool, Tool::MaskBrush, "the mask tool is remembered");
    editor.select_layer_shortcut(5);
    assert_eq!(editor.layer, Layer::Image);
    assert_eq!(editor.mode(), Mode::Inpaint, "the original view stays in its mode");
    editor.select_layer_shortcut(5);
    assert_eq!(editor.layer, Layer::InpaintMask, "5 again goes back");
    editor.select_layer_shortcut(3);
    assert_eq!(editor.tool, Tool::Result, "brush is shared with the result layer");
    editor.select_layer_shortcut(1);
    assert_eq!(editor.opacity, DEFAULT_LAYER_OPACITY);
    editor.focus_mode = false;
    editor.select_layer_shortcut(5);
    assert_eq!(editor.opacity, DEFAULT_LAYER_OPACITY, "focus mode off keeps opacities");
}

#[test]
fn overlay_selection_is_restored_and_groups_select_together() {
    let mut chapter = chapter_with_pages(1);
    let mut second = chapter.pages[0].blocks[0].clone();
    second.id = "b1".into();
    chapter.pages[0].blocks.push(second);
    chapter.pages[0].block_groups = Some(vec![mtt_core::model::TranslationBlockGroup {
        id: "g".into(),
        block_ids: vec!["b0".into(), "b1".into()],
        ..Default::default()
    }]);
    let mut editor = Editor::new(chapter);
    editor.set_layer(Layer::Overlay);
    editor.select_block_or_group("b1");
    assert_eq!(editor.selection, vec!["b1".to_owned(), "b0".to_owned()]);
    editor.select_layer_shortcut(4);
    assert!(editor.selection.is_empty());
    editor.select_layer_shortcut(2);
    assert_eq!(editor.selection.len(), 2, "the overlay selection comes back");
}

#[test]
fn fitting_a_block_to_its_balloon_is_undoable() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(chapter_with_pages(1));
    // A white balloon with a black outline around b0 (100..300 × 100..200) on gray art.
    let source = RgbaImage::from_fn(1000, 1000, |x, y| {
        let (dx, dy) = ((f64::from(x) - 200.0) / 170.0, (f64::from(y) - 150.0) / 110.0);
        let r = (dx * dx + dy * dy).sqrt();
        let text = (190..210).contains(&x) && (120..180).contains(&y);
        let v = if text || (0.98..=1.0).contains(&r) {
            0
        } else if r < 1.0 {
            255
        } else {
            128
        };
        image::Rgba([v, v, v, 255])
    });
    editor.assets.insert("p0".into(), PageAssets::Ready(Box::new(LoadedPage::new(&ctx, "p0", source, None, None))));
    editor.selection = vec!["b0".into()];
    editor.fit_selected_to_balloons();
    editor.commit_checkpoint();
    let block = &editor.page().unwrap().blocks[0];
    let area = block.render_bbox.expect("fitted to the balloon");
    assert!(area.x > 30.0 && area.x + area.w < 370.0 && area.w > 250.0, "{area:?}");
    assert!(block.text_shape.is_some());
    editor.undo(&ctx);
    let block = &editor.page().unwrap().blocks[0];
    assert_eq!((block.render_bbox, block.text_shape.as_ref()), (None, None));
}

#[test]
fn widened_block_inpaint_erases_past_the_glyphs() {
    // A dark glyph in the middle of a block on white paper.
    let source = RgbaImage::from_fn(200, 200, |x, y| {
        if (95..105).contains(&x) && (90..110).contains(&y) {
            image::Rgba([10, 10, 10, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let block = TranslationBlock { bbox: BBox::new(250.0, 250.0, 500.0, 500.0), ..TranslationBlock::default() };
    let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
    let run = |grow_px| {
        compute_inpaint(
            InpaintScope::SelectedBlocks { grow_px },
            &source,
            None,
            None,
            None,
            None,
            std::slice::from_ref(&block),
            InpaintEngine::LocalFillFallback,
            &settings,
            None,
            None,
        )
        .unwrap()
    };
    let covered = |outcome: &ComputedInpaint| outcome.mask.pixels().filter(|p| p[0] > 0).count();
    let (tight, wide) = (run(0), run(6));
    assert!(covered(&wide) > covered(&tight), "the mask grows: {} vs {}", covered(&wide), covered(&tight));
    assert_eq!(wide.mask.get_pixel(100, 100)[0], 255);
    assert_eq!(wide.mask.get_pixel(30, 30)[0], 0, "only around the glyph");
    assert!(wide.message.contains("6px"));
}
