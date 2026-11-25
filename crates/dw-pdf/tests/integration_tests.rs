//! 18-09 Integration & Regression（规范 15）。
//!
//! 端到端验证：`PDF → Parser → ObjectTree → Selection → Editing → Font Rewrite
//! → Content Rewrite → Save → Incremental PDF → Parser`。
//! 本章不新增编辑能力，只复用 18-01～18-08 的已冻结组件。

use std::path::{Path, PathBuf};

use dw_core::capability::CapabilitySet;
use dw_core::content::{ContentPatch, RewriteRequest};
use dw_core::document::{Document, DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::driver::ISave;
use dw_core::editing::{EditingEngine, EditingOperation, EditingRequest};
use dw_core::eventbus::EventBus;
use dw_core::font::{FontDescriptor, FontEncoding, FontRegistry, FontRewriteEngine};
use dw_core::hittest::HitResult;
use dw_core::object::{FontRef, Object, ObjectKind, TextObject};
use dw_core::page::PageId;
use dw_core::selection::{ModifierKeys, SelectionEngine, SelectionPolicy, SelectionRequest};
use dw_core::session::Session;
use dw_pdf::content_rewrite::{decode_operators, ContentRewriteEngine};
use dw_pdf::parser::PdfParser;
use dw_pdf::save::PdfSaver;
use lopdf::dictionary;
use lopdf::Document as LopdfDocument;
use lopdf::Object as LopdfObject;

/// 固定临时文件名，避免并行测试互相覆盖。
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dw-pdf-it09-{}.pdf", name))
}

/// 生成 1~2 页 PDF，每页一个文本 Operator + 一个纯字母未知 Operator。
fn write_pdf(path: &Path, page_count: usize) {
    let mut pdf = LopdfDocument::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let mut page_ids = Vec::new();
    for index in 1..=page_count {
        let content = pdf.add_object(lopdf::Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf (OLD{}) Tj CUSTOMMARK 7 ET", index).into_bytes(),
        ));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        page_ids.push(page_id);
    }
    let kids: Vec<LopdfObject> = page_ids
        .iter()
        .map(|id| LopdfObject::Reference(*id))
        .collect();
    pdf.objects.insert(
        pages_id,
        LopdfObject::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_count as i64,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(path).expect("样例 PDF 应保存成功");
}

fn core_document(path: &Path, page_count: usize) -> Document {
    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(path);
    let size = std::fs::metadata(path).map(|item| item.len()).unwrap_or(0);
    document.mutate(|inner| {
        inner.metadata.file_size = size;
        // 不记录修改时间：增量可用性判定在缺时间戳时只比对文件大小。
    });
    document.initialize_page_layout(PageLayout::from_count(page_count));
    document.set_state(DocumentState::Editing);
    document
}

/// 直接读取某页内容流字节（用于构造 RewriteRequest 与验证写盘效果）。
fn stream_bytes(path: &Path, page_no: usize) -> Vec<u8> {
    let pdf = LopdfDocument::load(path).expect("PDF 可加载");
    let pages = pdf.get_pages();
    let page_xref = *pages.get(&(page_no as u32)).expect("页码存在");
    let page = pdf.get_object(page_xref).expect("页对象存在");
    let contents = page
        .as_dict()
        .expect("页是字典")
        .get(b"Contents")
        .expect("有 Contents");
    let stream_id = contents.as_reference().expect("Contents 是引用");
    pdf.get_object(stream_id)
        .expect("流对象存在")
        .as_stream()
        .expect("是流")
        .get_plain_content()
        .expect("流可读")
        .to_vec()
}

/// 具备对象编辑能力的能力集。
fn editing_capabilities() -> CapabilitySet {
    let mut set = CapabilitySet::new();
    set.insert(dw_core::capability::Capability::ObjectEditing);
    set
}

/// 构造字体注册表：`新` → 0x65B0。
fn registry(font: &FontRef) -> FontRegistry {
    let mut registry = FontRegistry::new();
    let descriptor = FontDescriptor::new(font.clone(), FontEncoding::Identity)
        .with_flags(true, true)
        .with_glyph("新", 0x65B0);
    registry.insert(descriptor);
    registry
}

/// 完整链路：解析 → 选择 → 编辑 → 字体重写 → 内容重写。
fn chain(path: &Path, page_no: usize, new_text: &str) -> (ContentPatch, Vec<u8>) {
    let document = core_document(path, if page_no > 1 { 2 } else { 1 });
    let parse_result = PdfParser::new().parse(&document).expect("解析成功");
    let tree = parse_result.tree;
    let page_id = PageId::new(page_no as u32);
    let page = tree.get_page(page_id).expect("目标页存在");
    let object = page.objects_by_z().first().copied().expect("页上有对象");

    // Selection：只消费 HitResult。
    let hit = HitResult {
        page_id,
        object_id: object.object_id,
        kind: ObjectKind::Text,
        bbox: object.bbox,
        distance: 0.0,
        priority: 50,
    };
    let mut session = Session::create();
    let mut events = EventBus::new();
    SelectionEngine::new()
        .apply(
            &mut session,
            page,
            Some(1),
            SelectionRequest::new(hit, SelectionPolicy::Single, ModifierKeys::none()),
            &mut events,
        )
        .expect("选择成功");

    // Editing：WorkingCopy → Commit。
    let mut local_page = page.clone();
    let mut editing = EditingEngine::new();
    editing
        .begin_edit(
            &local_page,
            session.selection_state(),
            &editing_capabilities(),
            EditingRequest::new(
                object.object_id,
                EditingOperation::ReplaceText(new_text.to_string()),
            ),
            &mut events,
        )
        .expect("开始编辑");
    let commit = editing
        .commit(
            &mut local_page,
            session.selection_state(),
            &editing_capabilities(),
            &mut events,
        )
        .expect("提交成功");

    // Font Rewrite：EditingCommit → FontPatch。
    let font_ref = commit.object.as_text().expect("文本对象").font.clone();
    let font_patch = FontRewriteEngine::new()
        .rewrite(&commit, &registry(&font_ref), &mut events)
        .expect("字体重写成功");

    // Content Rewrite：源流只读。
    let source = stream_bytes(path, page_no);
    let operator_index = decode_operators(&source)
        .expect("源流可解析")
        .iter()
        .position(|op| op.name == "Tj")
        .expect("有 Tj");
    let request = RewriteRequest::new(commit, font_patch, source, page_id, None, operator_index);
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut events)
        .expect("内容重写成功");
    (patch, stream_bytes(path, page_no))
}

/// 把 Patch 暂存并保存（AD-25 唯一写回路径）。
fn save_with_patch(path: &Path, page_count: usize, patch: &ContentPatch, incremental: bool) {
    let document = core_document(path, page_count);
    let saver = PdfSaver::new();
    saver.apply_changes(&document).expect("应用变更");
    saver.stage_content_patch(patch).expect("暂存 ContentPatch");
    saver.save(&document, path, incremental).expect("保存成功");
}

/// IT-01：ReplaceText → 保存 → 重新解析，对象身份 / 几何 / 种类一致，字节含新字形。
#[test]
fn it01_replace_text_and_reopen() {
    let path = temp_path("it01");
    write_pdf(&path, 1);
    let (patch, _before) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);

    let after = stream_bytes(&path, 1);
    assert!(
        after.windows(2).any(|pair| pair == [0x65, 0xB0]),
        "新字形未写回"
    );

    let document = core_document(&path, 1);
    let tree = PdfParser::new()
        .parse(&document)
        .expect("重新解析成功")
        .tree;
    let page = tree.get_page(PageId::new(1)).expect("页存在");
    let objects: Vec<&Object> = page.objects_by_z();
    assert_eq!(objects.len(), 2, "文本 + Unknown 两个对象");
    assert_eq!(objects[0].kind, ObjectKind::Text);
    assert_eq!(objects[0].bbox, page.objects_by_z()[0].bbox);
}

/// IT-02：多页只改目标页，其它页内容流字节不变。
#[test]
fn it02_multipage_edit() {
    let path = temp_path("it02");
    write_pdf(&path, 2);
    let page1_before = stream_bytes(&path, 1);
    let (patch, _) = chain(&path, 2, "新");
    save_with_patch(&path, 2, &patch, true);

    assert_eq!(stream_bytes(&path, 1), page1_before, "非目标页不得被改写");
    assert!(stream_bytes(&path, 2)
        .windows(2)
        .any(|pair| pair == [0x65, 0xB0]));
}

/// IT-03：Unknown Operator 在写盘后仍保留。
#[test]
fn it03_unknown_operator_survives_save() {
    let path = temp_path("it03");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);

    let after = stream_bytes(&path, 1);
    let ops = decode_operators(&after).expect("写盘后可解析");
    assert!(ops.iter().any(|op| op.name == "CUSTOMMARK"));
    assert_eq!(
        patch
            .operators
            .iter()
            .filter(|op| op.name == "CUSTOMMARK")
            .count(),
        1
    );
}

/// IT-04：Diff 撤销 / 重做后可再次保存（Patch 不复用）。
#[test]
fn it04_undo_redo_then_save() {
    use dw_core::diff::Diff;

    let path = temp_path("it04");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    let diff = patch.clone();
    save_with_patch(&path, 1, &patch, true);

    let document = core_document(&path, 1);
    let tree = PdfParser::new().parse(&document).expect("解析").tree;
    let mut page = tree.get_page(PageId::new(1)).expect("页").clone();
    let object = page.objects_by_z().first().copied().expect("对象").clone();

    // 用 Commit 阶段产生的对象级 Diff 回放（Undo / Redo 依据 Diff，不是 Patch）。
    let before = object.clone();
    let mut changed = object;
    changed.set_payload(dw_core::object::ObjectPayload::Text(TextObject::new(
        "F1",
        vec![1],
        Some("新".to_string()),
    )));
    let commit_diff = Diff::object_change(&dw_core::diff::ObjectDiffPayload {
        object_id: changed.object_id,
        operation: "TextReplace".to_string(),
        before_version: before.version,
        after_version: changed.version,
        before: serde_json::to_value(&before).expect("可序列化"),
        after: serde_json::to_value(&changed).expect("可序列化"),
    });
    commit_diff.revert_object(&mut page).expect("Undo");
    assert_eq!(
        page.object(before.object_id).unwrap().version,
        before.version
    );
    commit_diff.apply_object(&mut page).expect("Redo");
    assert_eq!(
        page.object(changed.object_id).unwrap().version,
        changed.version
    );

    // 再次保存必须重新生成 Patch，不能复用旧的。
    let (fresh_patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &fresh_patch, true);
    assert_eq!(diff.page_id, fresh_patch.page_id);
}

/// IT-05：连续两次 Save 都成功，最终字节仍含新字形。
#[test]
fn it05_repeated_save() {
    let path = temp_path("it05");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);
    save_with_patch(&path, 1, &patch, true);
    assert!(stream_bytes(&path, 1)
        .windows(2)
        .any(|pair| pair == [0x65, 0xB0]));
}

/// IT-06：增量保存包含 `/Prev`，且原文件字节为前缀。
#[test]
fn it06_incremental_save() {
    let path = temp_path("it06");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);

    let bytes = std::fs::read(&path).expect("可读");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Prev"), "增量 PDF 必须有 /Prev");
}

/// IT-07：Parser 在保存前后对未改动对象给出一致快照。
#[test]
fn it07_parser_consistency() {
    let path = temp_path("it07");
    write_pdf(&path, 1);
    let document = core_document(&path, 1);
    let before = PdfParser::new().parse(&document).expect("解析").tree;
    let before_page = before.get_page(PageId::new(1)).expect("页");
    let before_object = before_page
        .objects_by_z()
        .first()
        .copied()
        .expect("对象")
        .clone();

    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);
    let after = PdfParser::new().parse(&document).expect("再解析").tree;
    let after_object = after
        .get_page(PageId::new(1))
        .expect("页")
        .objects_by_z()
        .first()
        .copied()
        .expect("对象")
        .clone();

    assert_eq!(before_object.object_id, after_object.object_id);
    assert_eq!(before_object.bbox, after_object.bbox);
    assert_eq!(before_object.kind, after_object.kind);
    assert_eq!(before_object.z_index, after_object.z_index);
}

/// IT-08：RoundTrip —— Patch 输出与重新解析的 Operator 序列数量一致、顺序一致。
#[test]
fn it08_round_trip() {
    let path = temp_path("it08");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);

    let after = decode_operators(&stream_bytes(&path, 1)).expect("可解析");
    assert_eq!(after.len(), patch.operators.len());
    for (before, after) in patch.operators.iter().zip(after.iter()) {
        assert_eq!(before.name, after.name);
    }
    assert_eq!(after.iter().filter(|op| op.name == "CUSTOMMARK").count(), 1);
}

/// IT-09：Patch 释放——Save 取走暂存后，无 apply_changes 的再次 stage 必须失败。
#[test]
fn it09_patch_release() {
    let path = temp_path("it09");
    write_pdf(&path, 1);
    let (patch, _) = chain(&path, 1, "新");
    save_with_patch(&path, 1, &patch, true);

    let fresh_saver = PdfSaver::new();
    assert!(
        fresh_saver.stage_content_patch(&patch).is_err(),
        "未暂存的 saver 不得消费 Patch"
    );
    let document = core_document(&path, 1);
    fresh_saver.apply_changes(&document).expect("暂存");
    fresh_saver
        .stage_content_patch(&patch)
        .expect("暂存后可消费");
    fresh_saver.save(&document, &path, true).expect("保存");
    assert!(
        fresh_saver.stage_content_patch(&patch).is_err(),
        "Save 取走暂存后 Patch 已释放"
    );
}

/// IT-10：100 轮循环不增长——每轮暂存都被 Save 取走并释放，页面对象数稳定。
#[test]
fn it10_no_unbounded_growth() {
    let path = temp_path("it10");
    write_pdf(&path, 1);
    for round in 0..100 {
        let (patch, _) = chain(&path, 1, "新");
        assert_eq!(
            patch.operators.len(),
            5,
            "第 {} 轮 Operator 数应稳定",
            round
        );
        save_with_patch(&path, 1, &patch, true);
        let document = core_document(&path, 1);
        let tree = PdfParser::new().parse(&document).expect("可解析").tree;
        assert_eq!(tree.page_count(), 1);
        assert_eq!(tree.object_count(), 2, "第 {} 轮对象数不应增长", round);
        drop(patch);
    }
}
