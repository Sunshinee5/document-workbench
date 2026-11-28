//! 回归：UI 同款坐标映射必须命中真实 PDF 文字对象并进入编辑态。
//!
//! 背景：真实 PDF 首行是 `0.24 0 0 -0.24 0 841.92 cm`（y 翻转 + 缩放），
//! 旧版 Parser 忽略 `cm`，bbox 留在未变换的用户空间，导致页面上的文字点不到。
//! 本测试用同构的合成 PDF 固定该行为；若本机存在真实样本，再用真实样本复验。

use std::path::{Path, PathBuf};

use dw_core::document::{Document, DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::eventbus::EventBus;
use dw_core::object::ObjectKind;
use dw_core::page::{PageId, Size};
use dw_core::session::Session;
use dw_pdf::editing_service::ObjectEditingService;
use dw_pdf::parser::PdfParser;
use lopdf::dictionary;

/// UI `PdfViewer` 的固定渲染基准：`BASE_SCALE = 1.25`，PDF.js scale=1 的页宽即 PDF 点宽。
const BASE_SCALE: f64 = 1.25;
/// 用户实测窗口的缩放比例。
const UI_ZOOM: f64 = 1.19;

/// 合成一份与真实样本同构的 PDF：全局 y 翻转 + 嵌套缩放 + `1 0 0 -1 Tm`。
fn write_ctm_pdf(path: &Path) {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let content = pdf.add_object(lopdf::Stream::new(
        lopdf::dictionary! {},
        b"0.24 0 0 -0.24 0 841.91998 cm q 3.125 0 0 3.125 0 0 cm BT /F1 12 Tf 1 0 0 -1 32 30 Tm (A) Tj ET Q".to_vec(),
    ));
    let font_id = pdf.add_object(lopdf::dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = pdf.add_object(lopdf::dictionary! {
        "Font" => lopdf::dictionary! { "F1" => font_id },
    });
    let page_id = pdf.add_object(lopdf::dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), lopdf::Object::Real(594.96), lopdf::Object::Real(841.92)],
    });
    pdf.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(lopdf::dictionary! {
            "Type" => "Pages",
            "Kids" => vec![lopdf::Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = pdf.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(path).expect("合成样本应能保存");
}

/// 把源文件注册进 Session，并打开编辑服务。
fn open_service(path: &Path) -> (Session, EventBus, DocumentId, ObjectEditingService) {
    let mut session = Session::create();
    let capabilities = DocumentKind::Pdf.declared_capabilities();
    let id = session.documents.create(DocumentKind::Pdf, capabilities.clone());
    {
        let document = session.documents.get(id).expect("文档刚创建");
        document.set_source_path(path);
        document.initialize_page_layout(PageLayout::from_count(1));
        document.set_state(DocumentState::Editing);
    }
    let mut events = EventBus::new();
    let mut service = ObjectEditingService::new();
    service.open(&mut session, id, &mut events).expect("编辑服务绑定成功");
    (session, events, id, service)
}

/// UI 同款映射：pageWidthPx = round(页宽 × BASE_SCALE × zoom)，
/// 后端 zoom = pageWidthPx / 页宽；screen_y = (页高 − PDF y) × 后端 zoom。
fn ui_page_width_px(page_width: f64) -> f64 {
    (page_width * BASE_SCALE * UI_ZOOM).round()
}

/// 断言：给定页上第一个可见文本对象能被 UI 坐标命中、选择并进入编辑态。
fn assert_ui_hit_and_edit(session: &mut Session, events: &mut EventBus, service: &mut ObjectEditingService, id: DocumentId, page_no: u32, page_size: Size) {
    let target = {
        let document = session.documents.get(id).expect("文档存在");
        let parser = PdfParser::new();
        let result = parser.parse(document).expect("真实 PDF 应可解析");
        let page = result.tree.get_page(PageId::new(page_no)).expect("页存在");
        let Some(object) = page.objects.values().find(|object| {
            object.kind == ObjectKind::Text
                && object.bbox.x >= 0.0
                && object.bbox.x + object.bbox.width <= page.size.width
                && object.bbox.y >= 0.0
                && object.bbox.y + object.bbox.height <= page.size.height
        }) else {
            panic!("第 {} 页没有落在页面内的文本对象", page_no);
        };
        (object.object_id.0, object.bbox)
    };
    let (object_id, bbox) = target;
    assert!(
        bbox.y >= 0.0 && bbox.y + bbox.height <= page_size.height,
        "CTM 应用后 bbox 必须落在页面坐标系内：{:?}",
        bbox
    );

    let page_width_px = ui_page_width_px(page_size.width);
    let zoom_backend = page_width_px / page_size.width;
    let screen_x = (bbox.x + bbox.width / 2.0) * zoom_backend;
    let screen_y = (page_size.height - (bbox.y + bbox.height / 2.0)) * zoom_backend;
    let hit = service
        .hit_test(page_no, screen_x, screen_y, page_width_px, 0)
        .unwrap_or_else(|| panic!("UI 同款坐标必须命中文本：({:.2},{:.2})", screen_x, screen_y));
    assert_eq!(hit.object_id, object_id);
    assert_eq!(hit.kind, "Text");

    service
        .select(session, events, page_no, object_id, false, false, false)
        .expect("命中对象应能选中");
    let document = session.documents.get(id).expect("文档存在");
    let selected = service.snapshot(session, document, page_no, UI_ZOOM as f32);
    assert_eq!(selected.selection.primary, Some(object_id));
    assert!(selected.capability.can_edit_text, "选中文本后编辑按钮应点亮");

    service
        .begin_edit(session, document, events, object_id, "A".to_string())
        .expect("双击链应能进入编辑态");
    let editing = service.snapshot(session, document, page_no, UI_ZOOM as f32);
    assert!(editing.editing.active, "begin_edit 后必须处于编辑态");
    assert_eq!(editing.editing.object_id, Some(object_id));
}

/// 合成样本：证明 CTM 链被正确应用到 bbox，且 UI 映射能命中并编辑。
#[test]
fn ui_mapping_hits_ctm_transformed_text_and_enters_edit() {
    let path = std::env::temp_dir().join("dw-ui-hit-regression.pdf");
    write_ctm_pdf(&path);

    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let parser = PdfParser::new();
    let result = parser.parse(&document).expect("合成 PDF 应可解析");
    assert!(parser.warnings().is_empty(), "合法多行内容流不应产生误报警告");
    let page = result.tree.get_page(PageId::new(1)).expect("页存在");
    assert!(
        (page.size.width - 594.96).abs() < 0.01 && (page.size.height - 841.92).abs() < 0.01,
        "页尺寸应来自 MediaBox：{:?}",
        page.size
    );

    let (mut session, mut events, id, mut service) = open_service(&path);
    assert_ui_hit_and_edit(
        &mut session,
        &mut events,
        &mut service,
        id,
        1,
        page.size,
    );
    let _ = std::fs::remove_file(&path);
}

/// 真实样本：若存在 `DW_PDF_SAMPLE` 或本机实测文件，再复验一次同一条链。
#[test]
fn ui_mapping_hits_real_pdf_text_and_enters_edit() {
    let path = std::env::var("DW_PDF_SAMPLE")
        .map(PathBuf::from)
        .ok()
        .filter(|path| path.is_file())
        .or_else(|| {
            let fallback = PathBuf::from(r"C:\Users\Administrator\Desktop\打印准考证详情.pdf");
            fallback.is_file().then_some(fallback)
        });
    let Some(path) = path else {
        eprintln!("跳过：本机没有真实 PDF 样本");
        return;
    };

    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let parser = PdfParser::new();
    let result = parser.parse(&document).expect("真实 PDF 应可解析");
    let page = result.tree.get_page(PageId::new(1)).expect("页存在");
    let texts = page.objects.values().filter(|object| object.kind == ObjectKind::Text).count();
    assert!(texts > 0, "真实样本第一页应有文本对象");

    let (mut session, mut events, id, mut service) = open_service(&path);
    assert_ui_hit_and_edit(
        &mut session,
        &mut events,
        &mut service,
        id,
        1,
        page.size,
    );
}
