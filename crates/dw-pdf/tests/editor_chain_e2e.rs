//! 编辑链路逐层可观测验证（UI 同款参数链）。
//!
//! 不修改 Parser / HitTest：本测试只把真实 PDF 在
//! `open → hit_test → select → snapshot → begin_edit` 每一层的真实值打印出来，
//! 用来区分「后端链路断点」与「前端投影 / 打包版本问题」。

use std::path::{Path, PathBuf};

use dw_core::document::{DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::eventbus::EventBus;
use dw_core::object::ObjectKind;
use dw_core::page::PageId;
use dw_core::session::SessionManager;
use dw_pdf::editing_service::ObjectEditingService;
use dw_pdf::parser::PdfParser;

const BASE_SCALE: f64 = 1.25;
const UI_ZOOM: f64 = 1.19;

/// 优先环境变量，其次 QA 样本，最后桌面原件。
fn sample_path() -> PathBuf {
    if let Ok(raw) = std::env::var("DW_PDF_SAMPLE") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return path;
        }
    }
    let qa = PathBuf::from(r"C:\Users\Administrator\AppData\Local\Temp\dw-test.pdf");
    if qa.is_file() {
        return qa;
    }
    PathBuf::from(r"C:\Users\Administrator\Desktop\打印准考证详情.pdf")
}

/// 注册文档到活动 Session，模拟 `open_document` 的段1数据分配。
fn register_document(session: &mut dw_core::session::Session, path: &Path) -> DocumentId {
    let id = session
        .documents
        .create(DocumentKind::Pdf, DocumentKind::Pdf.declared_capabilities());
    {
        let document = session.documents.get(id).expect("文档刚创建");
        document.set_source_path(path);
        document.initialize_page_layout(PageLayout::from_count(2));
        document.set_state(DocumentState::Editing);
    }
    session.workspace.add_tab(id, "打印准考证详情.pdf");
    session.workspace.set_active_document(id);
    id
}

#[test]
fn editor_chain_prints_every_step_with_ui_parameters() {
    let path = sample_path();
    assert!(path.is_file(), "真实样本必须存在：{}", path.display());
    println!("[STEP0] 样本={} 字节={}", path.display(), std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));

    // editor_open 的 IPC 前置条件：SessionManager 必须已有 active session。
    let mut manager = SessionManager::new();
    println!(
        "[STEP1] editor_open 前置：active_session={:?}, session_count={}（None 时 IPC 必返回『没有活动会话』）",
        manager.active().map(|s| s.state),
        manager.session_count()
    );
    let session_id = manager.create_session();
    let session_count = manager.session_count();
    let active_id = manager.active_id();
    let session = manager.active_mut().expect("create_session 后必须有活动会话");
    println!(
        "[STEP1] 建会话后：active_session_id={:?}, session_id={}, state={:?}, sessions={}",
        active_id,
        session_id,
        session.state,
        session_count
    );

    let document_id = register_document(session, &path);
    println!(
        "[STEP2] 注册文档：document_id={}, active_document={:?}, documents={}",
        document_id.0,
        session.workspace.view_state().active_document_id,
        session.documents.len()
    );

    let mut events = EventBus::new();
    let mut editor = ObjectEditingService::new();
    let open_result = editor.open(session, document_id, &mut events);
    match &open_result {
        Ok(()) => println!("[STEP3] editor_open=Ok"),
        Err(error) => println!("[STEP3] editor_open=Err({}: {})", error.driver, error.message),
    }
    open_result.expect("真实 PDF 应能绑定编辑服务");

    let document = session.documents.get(document_id).expect("文档存在");
    let before = editor.snapshot(session, document, 1, UI_ZOOM as f32);
    println!(
        "[STEP3] 绑定后：snapshot.document_id={}, selection.primary={:?}, selection.count={}, can_edit_text={}",
        before.document_id, before.selection.primary, before.selection.selected.len(), before.capability.can_edit_text
    );
    assert_ne!(before.document_id, 0, "editor.open 必须写入 document_id");

    // 同一份文档解析出的第一个页面内可见文本对象，作为 UI 点击目标。
    let parser = PdfParser::new();
    let result = parser.parse(document).expect("真实 PDF 可解析");
    let page = result.tree.get_page(PageId::new(1)).expect("第 1 页存在");
    let target = page
        .objects
        .values()
        .find(|object| {
            object.kind == ObjectKind::Text
                && object.bbox.x >= 0.0
                && object.bbox.x + object.bbox.width <= page.size.width
                && object.bbox.y >= 0.0
                && object.bbox.y + object.bbox.height <= page.size.height
        })
        .expect("第一页应有页面内文本对象");
    let page_width_px = (page.size.width * BASE_SCALE * UI_ZOOM).round();
    let zoom_backend = page_width_px / page.size.width;
    let x = (target.bbox.x + target.bbox.width / 2.0) * zoom_backend;
    let y = (page.size.height - (target.bbox.y + target.bbox.height / 2.0)) * zoom_backend;
    println!(
        "[STEP4] 页尺寸={:?}, page_width_px={}, backend_zoom={:.5}",
        page.size, page_width_px, zoom_backend
    );
    println!(
        "[STEP4] 目标 object_id={}, bbox=[{:.2},{:.2},{:.2},{:.2}]",
        target.object_id.0, target.bbox.x, target.bbox.y, target.bbox.width, target.bbox.height
    );

    let hit = editor.hit_test(1, x, y, page_width_px, 0);
    match &hit {
        Some(hit) => println!(
            "[STEP5] hit_test(page=1,x={:.2},y={:.2},rotation=0) -> object_id={}, kind={}, bbox=[{:.2},{:.2},{:.2},{:.2}]",
            x, y, hit.object_id, hit.kind, hit.bbox[0], hit.bbox[1], hit.bbox[2], hit.bbox[3]
        ),
        None => println!("[STEP5] hit_test(page=1,x={:.2},y={:.2}) -> None", x, y),
    }
    let hit = hit.expect("UI 同款坐标必须命中");
    assert_eq!(hit.object_id, target.object_id.0);
    assert_eq!(hit.kind, "Text");

    editor
        .select(session, &mut events, 1, hit.object_id, false, false, false)
        .expect("选择应成功");
    let document = session.documents.get(document_id).expect("文档存在");
    let selected = editor.snapshot(session, document, 1, UI_ZOOM as f32);
    println!(
        "[STEP6] select 后：selection.page={:?}, primary={:?}, selected={:?}, count={}, version={}",
        selected.selection.page,
        selected.selection.primary,
        selected.selection.selected,
        selected.selection.selected.len(),
        selected.selection.version
    );
    println!(
        "[STEP6] capability：can_edit={}, can_edit_text={}, can_edit_image={}, can_edit_path={}",
        selected.capability.can_edit,
        selected.capability.can_edit_text,
        selected.capability.can_edit_image,
        selected.capability.can_edit_path
    );
    assert_eq!(selected.selection.primary, Some(hit.object_id));
    assert!(selected.capability.can_edit_text);

    editor
        .begin_edit(session, document, &mut events, hit.object_id, "A".to_string())
        .expect("进入编辑态应成功");
    let editing = editor.snapshot(session, document, 1, UI_ZOOM as f32);
    println!(
        "[STEP7] begin_edit 后：editing.active={}, editing.object_id={:?}, selection.primary={:?}, revision={}",
        editing.editing.active, editing.editing.object_id, editing.selection.primary, editing.revision
    );
    assert!(editing.editing.active);
    assert_eq!(editing.editing.object_id, Some(hit.object_id));
}
