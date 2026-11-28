//! editor_commit 端到端回归：修复「提交没反应 / 草稿被清空导致空文本提交」的可验证证据。
//!
//! 用合成的无压缩内容流 PDF（Type0/Identity-H + ToUnicode 双字节字体）走完整编辑链：
//!   open → hit_test → select → beginEdit → commit(newText) → save → reload
//!
//! 断言覆盖用户可见问题的后端根因：
//!   1. commit 的 text 参数真的落到 WorkingCopy（tree 对象 unicode 立即变成 newText），
//!      且覆盖 beginEdit 时传入的旧文本；
//!   2. EditingCommit.before_version / after_version 正确递增；
//!   3. document.content_patches() 中目标页补丁存在且 operators > 0（与 IPC 的
//!      CommitInfo.operators 同源）；
//!   4. Save 后 reload / 独立 Parser 重新解析，磁盘文本仍是 newText（页面文字真的变了）。
//!
//! 不使用真实 FlateDecode 内容流 PDF：既有 save.rs 的 set_content 未清理 /Filter 是已知遗留 bug，
//! 真实压缩 PDF 保存会损坏；合成无压缩流足以证明 commit→save→reload 的文本闭环。

use std::path::{Path, PathBuf};

use dw_core::content::PdfOperand;
use dw_core::document::{DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::driver::ISave;
use dw_core::eventbus::EventBus;
use dw_core::object::ObjectKind;
use dw_core::page::PageId;
use dw_core::session::Session;
use dw_pdf::content_rewrite::decode_operators;
use dw_pdf::editing_service::ObjectEditingService;
use dw_pdf::object_tree_view::project_object_tree;
use dw_pdf::parser::PdfParser;
use dw_pdf::save::PdfSaver;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

const PAGE_W: f64 = 595.0;
const PAGE_H: f64 = 842.0;
/// 原始文本 ABCD（字形码 0x0041..0x0044），提交后反转，四个字符都在 ToUnicode 里。
const ORIGINAL_TEXT: &str = "ABCD";
const COMMITTED_TEXT: &str = "DCBA";

/// 单页 PDF：内容流不压缩；ToUnicode 必须 FlateDecode（资源解析器用 decompressed_content 读取）。
fn write_pdf(path: &Path) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CMapName /DwCommitE2E def\n/CMapType 2 def\n\
1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
4 beginbfchar\n<0041> <0041>\n<0042> <0042>\n<0043> <0043>\n<0044> <0044>\nendbfchar\n\
endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n%",
    );
    cmap.push_str(&"padding-for-compression ".repeat(24));
    cmap.push('\n');
    let mut cmap_stream = Stream::new(dictionary! {}, cmap.into_bytes());
    cmap_stream.compress().expect("ToUnicode 应能压缩");
    let cmap_id = pdf.add_object(cmap_stream);
    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "DwCommitE2E",
        "Encoding" => "Identity-H",
        "ToUnicode" => cmap_id,
    });
    let resources_id = pdf.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let content_id = pdf.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 720 Td <0041004200430044> Tj ET".to_vec(),
    ));
    let page_id = pdf.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
    });
    pdf.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(path).expect("样例 PDF 应保存成功");
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dw-commit-e2e-{}.pdf", name))
}

/// 会话 + 文档 + 编辑服务：等价于 IPC 的 open_document + editor_open。
fn setup(path: &Path) -> (Session, EventBus, DocumentId, ObjectEditingService) {
    let mut session = Session::create();
    let capabilities = DocumentKind::Pdf.declared_capabilities();
    let id = session.documents.create(DocumentKind::Pdf, capabilities);
    {
        let document = session.documents.get(id).expect("文档刚创建");
        document.set_source_path(path);
        let size = std::fs::metadata(path).map(|item| item.len()).unwrap_or(0);
        document.mutate(|inner| inner.metadata.file_size = size);
        document.initialize_page_layout(PageLayout::from_count(1));
        document.set_state(DocumentState::Editing);
    }
    let mut events = EventBus::new();
    let mut service = ObjectEditingService::new();
    service
        .open(&mut session, id, &mut events)
        .expect("编辑服务绑定成功");
    (session, events, id, service)
}

/// UI 同款命中：bbox 中心 → 屏幕坐标（y 轴向下，page_width_px = 页宽）。
fn select_text_object(
    service: &mut ObjectEditingService,
    session: &mut Session,
    events: &mut EventBus,
    id: DocumentId,
) -> u64 {
    let document = session.documents.get(id).expect("文档存在");
    let info = project_object_tree(id.0, document).expect("对象树投影成功");
    let node = info.pages[0]
        .objects
        .iter()
        .find(|node| node.kind == "Text")
        .expect("样例应含文本对象");
    let bbox = node.bbox;
    let object_id = node.object_id;
    let hit = service
        .hit_test(
            1,
            bbox[0] + bbox[2] / 2.0,
            PAGE_H - (bbox[1] + bbox[3] / 2.0),
            PAGE_W,
            0,
        )
        .expect("UI 坐标应命中文本对象");
    assert_eq!(hit.object_id, object_id, "命中对象与对象树主选一致");
    assert_eq!(hit.kind, "Text");
    service
        .select(session, events, 1, object_id, false, false, false)
        .expect("选择成功");
    object_id
}

/// 从磁盘重新解析，返回第一页唯一文本对象的 Unicode（不依赖任何内存态）。
fn disk_text(path: &Path) -> String {
    let document = dw_core::document::Document::new(
        DocumentId(99),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(path);
    let result = PdfParser::new().parse(&document).expect("落盘 PDF 可解析");
    let page = result.tree.get_page(PageId::new(1)).expect("第一页存在");
    let texts: Vec<String> = page
        .objects
        .values()
        .filter(|object| object.kind == ObjectKind::Text)
        .filter_map(|object| object.as_text().and_then(|text| text.unicode.clone()))
        .collect();
    assert_eq!(texts.len(), 1, "落盘后应恰好一个文本对象");
    texts.into_iter().next().expect("已断言数量")
}

#[test]
fn commit_lands_text_in_working_copy_with_version_and_patch() {
    let path = temp_path("commit-lands");
    write_pdf(&path);
    let (mut session, mut events, id, mut service) = setup(&path);
    let object_id = select_text_object(&mut service, &mut session, &mut events, id);

    // 进入编辑态前：文本是原文，且能力投影点亮文本编辑。
    {
        let document = session.documents.get(id).expect("文档存在");
        let snapshot = service.snapshot(&session, document, 1, 1.0);
        assert_eq!(snapshot.selection.primary, Some(object_id));
        assert!(snapshot.capability.can_edit_text);
        let before = service.inspector(object_id).expect("Inspector 可读");
        assert_eq!(
            before.text.expect("文本分区").unicode.as_deref(),
            Some(ORIGINAL_TEXT)
        );
        service
            .begin_edit(&session, document, &mut events, object_id, ORIGINAL_TEXT.to_string())
            .expect("进入编辑态成功");
        let editing = service.snapshot(&session, document, 1, 1.0);
        assert!(editing.editing.active);
        assert_eq!(editing.editing.object_id, Some(object_id));
    }

    // commit 的 text 参数覆盖 beginEdit 初值，落到 WorkingCopy。
    let commit = {
        let document = session.documents.get(id).expect("文档存在");
        service
            .commit(&session, document, &mut events, Some(COMMITTED_TEXT.to_string()))
            .expect("提交成功")
    };
    assert_eq!(commit.object.object_id.0, object_id);
    assert_eq!(commit.after_version, commit.before_version + 1, "版本号应递增一级");
    assert_eq!(
        commit.object.as_text().expect("提交对象仍是文本").unicode.as_deref(),
        Some(COMMITTED_TEXT),
        "commit(text) 必须覆盖 beginEdit 的旧文本"
    );

    // ContentPatch 与 IPC 的 CommitInfo.operators 同源：目标页补丁存在且 operators > 0。
    let document = session.documents.get(id).expect("文档存在");
    let patches: Vec<_> = document
        .content_patches()
        .into_iter()
        .filter(|patch| patch.page_id == PageId::new(1))
        .collect();
    assert_eq!(patches.len(), 1, "提交后目标页应恰好一条待保存补丁");
    let patch = &patches[0];
    assert!(!patch.operators.is_empty(), "补丁 operators 必须大于 0");
    assert_eq!(patch.operator_count(), patch.operators.len());
    let tj = patch
        .operators
        .iter()
        .find(|operator| operator.name == "Tj")
        .expect("补丁应含 Tj 绘制操作符");
    match &tj.operands[0] {
        // DCBA 的 Identity-H 双字节大端字形码：0044 0043 0042 0041。
        PdfOperand::String(bytes) => assert_eq!(
            bytes.as_slice(),
            &[0x00, 0x44, 0x00, 0x43, 0x00, 0x42, 0x00, 0x41],
            "Tj 操作数应是提交文本 DCBA 的双字节字形码"
        ),
        other => panic!("Tj 操作数应为字符串：{:?}", other),
    }
    assert_eq!(
        decode_operators(&dw_pdf::content_rewrite::encode_operators(&patch.operators).expect("补丁可编码"))
            .expect("补丁可再次解码")
            .len(),
        patch.operators.len(),
        "补丁 operators 可无损往返"
    );
    assert_eq!(document.pending_content_patch_count(), 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn commit_then_save_then_reload_shows_new_text_on_page() {
    let path = temp_path("commit-save-reload");
    write_pdf(&path);
    let (mut session, mut events, id, mut service) = setup(&path);
    let object_id = select_text_object(&mut service, &mut session, &mut events, id);
    assert_eq!(disk_text(&path), ORIGINAL_TEXT, "保存前磁盘仍是原文");

    {
        let document = session.documents.get(id).expect("文档存在");
        service
            .begin_edit(&session, document, &mut events, object_id, ORIGINAL_TEXT.to_string())
            .expect("进入编辑态成功");
        service
            .commit(&session, document, &mut events, Some(COMMITTED_TEXT.to_string()))
            .expect("提交成功");
        // 提交不立即改变渲染：此时磁盘仍是原文，页面文字要等 Save 才更新。
        assert_eq!(disk_text(&path), ORIGINAL_TEXT, "commit 只入队补丁，不写盘");

        let saver = PdfSaver::new();
        saver.apply_changes(document).expect("应用变更");
        saver.save(document, &path, true).expect("保存成功");
        assert_eq!(document.pending_content_patch_count(), 0, "保存后补丁清空");
    }

    // 硬证据 A：完全绕过编辑服务，直接从磁盘解析——页面文字真的变了。
    assert_eq!(disk_text(&path), COMMITTED_TEXT, "保存后磁盘文本应为新文本");

    // 硬证据 B：编辑服务重新加载后 Inspector 也显示新文本。
    {
        let document = session.documents.get(id).expect("文档存在");
        service.reload(document).expect("重新解析成功");
        let inspector = service.inspector(object_id).expect("对象仍在");
        assert_eq!(
            inspector.text.expect("文本分区").unicode.as_deref(),
            Some(COMMITTED_TEXT),
            "重新加载后 Inspector 文本应更新"
        );
    }
    let _ = std::fs::remove_file(&path);
}
