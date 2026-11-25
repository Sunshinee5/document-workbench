//! 17 章（18A）Desktop Integration 桥接测试：DT-01 ~ DT-12。
//!
//! 只验证「桥接」：命中 / 选择 / 编辑 / 提交 / 保存 / 回放 / 关闭，
//! 编辑能力本身由 18 章测试覆盖，这里不重复验证。

use std::path::{Path, PathBuf};

use dw_core::capability::CapabilitySet;
use dw_core::document::{DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::driver::ISave;
use dw_core::eventbus::EventBus;
use dw_core::session::Session;
use dw_pdf::editing_service::ObjectEditingService;
use dw_pdf::object_tree_view::project_object_tree;
use dw_pdf::save::PdfSaver;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

/// 固定临时路径，避免并行测试互相覆盖。
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dw-bridge-{}.pdf", name))
}

/// 生成样例：每页一个 `ABCD` 文本对象，字体带 ToUnicode（Identity-H 双字节编码）。
///
/// 18-08 的内容流重写按 **2 字节大端** 写字形码，因此样例必须是双字节编码字体，
/// 否则重新解析时无法还原 Unicode（简单字体的单字节码会被写成含 NUL 的字节串）。
fn write_pdf(path: &Path, page_count: usize) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();

    // ToUnicode 必须带 FlateDecode：资源解析器用 decompressed_content() 读取，
    // 无 Filter 的流在 lopdf 里会解出空字节。这里塞一段重复注释保证压缩率足够。
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CMapName /DwTest def\n/CMapType 2 def\n\
1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
4 beginbfchar\n<0041> <0041>\n<0042> <0042>\n<0043> <0043>\n<0044> <0044>\nendbfchar\n\
endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n",
    );
    cmap.push('%');
    cmap.push_str(&"padding-for-compression ".repeat(24));
    cmap.push('\n');
    let mut cmap_stream = Stream::new(dictionary! {}, cmap.into_bytes());
    cmap_stream.compress().expect("ToUnicode 应能压缩");
    let cmap_id = pdf.add_object(cmap_stream);
    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "DwTest",
        "Encoding" => "Identity-H",
        "ToUnicode" => cmap_id,
    });
    let resources_id = pdf.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_ids = Vec::new();
    for _ in 0..page_count {
        let content = pdf.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 72 720 Td <0041004200430044> Tj ET".to_vec(),
        ));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        page_ids.push(page_id);
    }
    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    pdf.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_ids.len() as i64,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(path).expect("样例 PDF 应能保存");
}

/// 建会话并把文档注册进去（服务通过 `session.documents` 取 Document）。
fn setup(name: &str, page_count: usize) -> (PathBuf, Session, EventBus, DocumentId) {
    let path = temp_path(name);
    write_pdf(&path, page_count);
    let mut session = Session::create();
    let capabilities = DocumentKind::Pdf.declared_capabilities();
    let id = session.documents.create(DocumentKind::Pdf, capabilities.clone());
    let document = session.documents.get(id).expect("文档刚创建");
    document.set_source_path(&path);
    let size = std::fs::metadata(&path).map(|item| item.len()).unwrap_or(0);
    document.mutate(|inner| inner.metadata.file_size = size);
    document.initialize_page_layout(PageLayout::from_count(page_count));
    document.set_state(DocumentState::Editing);
    (path, session, EventBus::new(), id)
}

/// 取第一页第一个对象的 (object_id, bbox)。
fn first_object(session: &Session, id: DocumentId) -> (u64, [f64; 4]) {
    let document = session.documents.get(id).expect("文档存在");
    let info = project_object_tree(id.0, document).expect("对象树投影成功");
    let node = &info.pages[0].objects[0];
    (node.object_id, node.bbox)
}

/// DT-01：命中测试能把屏幕坐标映射回对象。
#[test]
fn dt01_hit_test_maps_screen_to_object() {
    let (_path, mut session, mut events, id) = setup("dt01", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, bbox) = first_object(&session, id);
    let center_x = bbox[0] + bbox[2] / 2.0;
    let center_y = bbox[1] + bbox[3] / 2.0;
    // zoom = 1：page_width_px 等于页宽（595），屏幕 y 轴向下。
    let hit = service.hit_test(1, center_x, 842.0 - center_y, 595.0, 0);
    let hit = hit.expect("页面中心附近应命中文本对象");
    assert_eq!(hit.object_id, object_id);
    assert_eq!(hit.kind, "Text");
}

/// DT-02 ~ DT-04：能力投影按对象类型切换。
#[test]
fn dt02_capability_follows_selection_kind() {
    let (_path, mut session, mut events, id) = setup("dt02", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);

    let document = session.documents.get(id).expect("文档存在");
    let before = service.snapshot(&session, document, 1, 1.0);
    assert!(!before.capability.can_edit_text, "未选中时文字编辑应为灰");

    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");
    let document = session.documents.get(id).expect("文档存在");
    let after = service.snapshot(&session, document, 1, 1.0);
    assert_eq!(after.selection.primary, Some(object_id));
    assert!(after.capability.can_edit_text, "文本对象应点亮文字编辑");
    assert!(!after.capability.can_edit_image);
    assert!(!after.capability.can_edit_path);
}

/// DT-05 / DT-06：BeginEdit → Editing；Commit → Dirty + 入队补丁。
#[test]
fn dt05_begin_and_commit() {
    let (_path, mut session, mut events, id) = setup("dt05", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");

    let document = session.documents.get(id).expect("文档存在");
    service
        .begin_edit(&session, document, &mut events, object_id, "ABBA".to_string())
        .expect("开始编辑");
    let document = session.documents.get(id).expect("文档存在");
    let editing = service.snapshot(&session, document, 1, 1.0);
    assert!(editing.editing.active, "BeginEdit 后应处于编辑态");
    assert_eq!(editing.editing.object_id, Some(object_id));

    let commit = service
        .commit(&session, document, &mut events, None)
        .expect("提交成功");
    assert_eq!(commit.after_version, commit.before_version + 1);
    let document = session.documents.get(id).expect("文档存在");
    let after = service.snapshot(&session, document, 1, 1.0);
    assert!(!after.editing.active, "提交后应回到空闲");
    assert!(after.dirty, "提交后文档应为 Dirty");
    assert_eq!(after.pending_patches, 1, "提交应产生 1 条待保存补丁");
}

/// DT-07 / DT-12：保存 → 补丁清空 → 重新解析后文本已更新。
#[test]
fn dt07_save_persists_and_reloads() {
    let (path, mut session, mut events, id) = setup("dt07", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");
    let document = session.documents.get(id).expect("文档存在");
    service
        .begin_edit(&session, document, &mut events, object_id, "ABBA".to_string())
        .expect("开始编辑");
    service.commit(&session, document, &mut events, None).expect("提交成功");

    // Save：走 17 章冻结的 ISave 入口，待保存补丁由 apply_changes 统一入暂存。
    let saver = PdfSaver::new();
    saver.apply_changes(document).expect("应用变更");
    saver.save(document, &path, true).expect("保存成功");
    assert_eq!(
        document.pending_content_patch_count(),
        0,
        "保存成功后待保存补丁必须清空"
    );

    // 重新解析：磁盘文本已更新。
    service.reload(document).expect("重新解析成功");
    let inspector = service.inspector(object_id).expect("对象仍在");
    assert_eq!(inspector.text.expect("文本对象").unicode.as_deref(), Some("ABBA"));
}

/// DT-08 / DT-09：对象 Diff 反向 / 正向回放。
#[test]
fn dt08_object_diff_replays_both_ways() {
    let (_path, mut session, mut events, id) = setup("dt08", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");
    let document = session.documents.get(id).expect("文档存在");
    service
        .begin_edit(&session, document, &mut events, object_id, "ABBA".to_string())
        .expect("开始编辑");
    let commit = service
        .commit(&session, document, &mut events, None)
        .expect("提交成功");
    let document = session.documents.get(id).expect("文档存在");
    assert_eq!(
        service
            .inspector(object_id)
            .and_then(|info| info.text)
            .and_then(|text| text.unicode)
            .as_deref(),
        Some("ABBA")
    );

    // Undo：回到 before 快照。
    service
        .replay_object(document, &mut events, &commit.diff, false)
        .expect("撤销回放成功");
    assert_eq!(
        service
            .inspector(object_id)
            .and_then(|info| info.text)
            .and_then(|text| text.unicode)
            .as_deref(),
        Some("ABCD")
    );

    // Redo：再回到 after 快照。
    service
        .replay_object(document, &mut events, &commit.diff, true)
        .expect("重做回放成功");
    assert_eq!(
        service
            .inspector(object_id)
            .and_then(|info| info.text)
            .and_then(|text| text.unicode)
            .as_deref(),
        Some("ABBA")
    );
}

/// DT-10：切页选择不跨页残留。
#[test]
fn dt10_selection_resets_on_page_change() {
    let (_path, mut session, mut events, id) = setup("dt10", 2);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (first, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, first, false, false, false)
        .expect("第 1 页选择成功");

    let document = session.documents.get(id).expect("文档存在");
    let info = project_object_tree(id.0, document).expect("对象树投影成功");
    let second = info.pages[1].objects[0].object_id;
    service
        .select(&mut session, &mut events, 2, second, false, false, false)
        .expect("第 2 页选择成功");

    let document = session.documents.get(id).expect("文档存在");
    let snapshot = service.snapshot(&session, document, 2, 1.0);
    assert_eq!(snapshot.selection.page, Some(2));
    assert_eq!(snapshot.selection.selected, vec![second]);
}

/// DT-11：关闭文档后所有状态恢复默认。
#[test]
fn dt11_close_resets_state() {
    let (_path, mut session, mut events, id) = setup("dt11", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");

    service.close(&mut session, &mut events);
    let document = session.documents.get(id).expect("文档存在");
    let snapshot = service.snapshot(&session, document, 1, 1.0);
    assert_eq!(service.document_id(), None);
    assert!(snapshot.selection.selected.is_empty());
    assert!(!snapshot.editing.active);
    assert!(!snapshot.capability.can_edit_text);
}

/// 关闭文档后继续操作必须报中文错误，而不是 panic。
#[test]
fn dt11b_operations_require_open() {
    let (_path, session, mut events, id) = setup("dt11b", 1);
    let mut service = ObjectEditingService::new();
    let document = session.documents.get(id).expect("文档存在");
    let error = service
        .begin_edit(&session, document, &mut events, 1, "AB".to_string())
        .expect_err("未绑定文档应失败");
    assert!(error.message.contains("editor_open"));
}

/// DT-12：全链路 Canvas → HitTest → Selection → Editing → Save → Reload。
#[test]
fn dt12_full_chain() {
    let (path, mut session, mut events, id) = setup("dt12", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");

    let (object_id, bbox) = first_object(&session, id);
    let hit = service
        .hit_test(
            1,
            bbox[0] + bbox[2] / 2.0,
            842.0 - (bbox[1] + bbox[3] / 2.0),
            595.0,
            0,
        )
        .expect("命中文本对象");
    assert_eq!(hit.object_id, object_id);

    service
        .select(&mut session, &mut events, 1, hit.object_id, false, false, false)
        .expect("选择成功");
    let document = session.documents.get(id).expect("文档存在");
    service
        .begin_edit(
            &session,
            document,
            &mut events,
            hit.object_id,
            "DCBA".to_string(),
        )
        .expect("开始编辑");
    let commit = service
        .commit(&session, document, &mut events, None)
        .expect("提交成功");
    assert_eq!(commit.object.page_id.raw(), 1);

    let saver = PdfSaver::new();
    saver.apply_changes(document).expect("应用变更");
    saver.save(document, &path, true).expect("保存成功");
    service.reload(document).expect("重新解析成功");

    let inspector = service.inspector(object_id).expect("对象仍在");
    let text = inspector.text.expect("文本对象");
    assert_eq!(text.unicode.as_deref(), Some("DCBA"));
    // 重新解析后对象是全新的 Loaded 实例：版本号从 1 起算，与提交时的内存版本无关。
    assert_eq!(inspector.lifecycle, "已加载");
    assert_eq!(commit.after_version, 2);
}

/// 撤销后未保存时，补丁队列必须同步更新（否则保存会把旧文本写回）。
#[test]
fn dt08b_undo_replaces_pending_patch() {
    let (_path, mut session, mut events, id) = setup("dt08b", 1);
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("绑定成功");
    let (object_id, _) = first_object(&session, id);
    service
        .select(&mut session, &mut events, 1, object_id, false, false, false)
        .expect("选择成功");
    let document = session.documents.get(id).expect("文档存在");
    service
        .begin_edit(&session, document, &mut events, object_id, "ABBA".to_string())
        .expect("开始编辑");
    let commit = service
        .commit(&session, document, &mut events, None)
        .expect("提交成功");
    assert_eq!(document.pending_content_patch_count(), 1);

    service
        .replay_object(document, &mut events, &commit.diff, false)
        .expect("撤销回放成功");
    // 同页覆盖：仍是 1 条补丁，但内容已回到 ABCD。
    assert_eq!(document.pending_content_patch_count(), 1);
    assert_eq!(
        service
            .inspector(object_id)
            .and_then(|info| info.text)
            .and_then(|text| text.unicode)
            .as_deref(),
        Some("ABCD")
    );
}
