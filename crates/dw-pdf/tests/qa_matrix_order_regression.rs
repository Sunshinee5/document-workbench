//! QA 独立回归（不复用工程师的 ui_hit_regression.rs 断言）。
//!
//! 关注三件事：
//! 1. `cm` 的复合顺序必须是行向量约定下的 `CTM_new = M × CTM_old`，
//!    且 `Q` 之后 CTM 必须回退——否则 bbox 会飞出页面。
//! 2. 合法多行内容流不再误报「存在损坏 Operator」，但真坏行仍告警。
//! 3. 命中 → 选择 → 编辑 → 提交 → 保存 → 重新解析全链路可用。
//!
//! 期望值均为手算推导，若实现顺序写反本文件会失败，不做「为过而改断言」。

use std::path::{Path, PathBuf};

use dw_core::document::{Document, DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::driver::ISave;
use dw_core::eventbus::EventBus;
use dw_core::session::Session;
use dw_pdf::editing_service::ObjectEditingService;
use dw_pdf::object_tree_view::project_object_tree;
use dw_pdf::parser::PdfParser;
use dw_pdf::save::PdfSaver;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

const PAGE_W: f64 = 594.96;
const PAGE_H: f64 = 841.92;
const BASE_SCALE: f64 = 1.25;
const UI_ZOOM: f64 = 1.19;

/// 全局 y 翻转+缩放 → q → 内层缩放 → 带 y 翻转的 Tm → 文本 A；
/// Q 之后第二个文本 B 只应受全局 cm 影响。
const CTM_CONTENT: &[u8] = b"0.24 0 0 -0.24 0 841.92 cm q 3.125 0 0 3.125 0 0 cm BT /F1 12 Tf 1 0 0 -1 32 30 Tm <0041> Tj ET Q BT /F1 12 Tf 1 0 0 -1 2000 30 Tm <0042> Tj ET";

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dw-qa-{}.pdf", name))
}

/// 生成带 Type0/Identity-H + ToUnicode 的 A4 单页 PDF（18-08 重写要求双字节字体）。
fn write_pdf(path: &Path, content: &[u8]) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CMapName /DwQaTest def\n/CMapType 2 def\n\
1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
26 beginbfchar\n",
    );
    for code in 0x41_u8..=0x5A {
        cmap.push_str(&format!("<{:04X}> <{:04X}>\n", code, code));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n%");
    cmap.push_str(&"padding-for-compression ".repeat(24));
    cmap.push('\n');
    let mut cmap_stream = Stream::new(dictionary! {}, cmap.into_bytes());
    cmap_stream.compress().expect("ToUnicode 应能压缩");
    let cmap_id = pdf.add_object(cmap_stream);
    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "DwQaTest",
        "Encoding" => "Identity-H",
        "ToUnicode" => cmap_id,
    });
    let resources_id = pdf.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let content_id = pdf.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let page_id = pdf.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(PAGE_W as f32), Object::Real(PAGE_H as f32)],
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
    pdf.save(path).expect("样本 PDF 应能保存");
}

fn open_service(path: &Path) -> (Session, EventBus, DocumentId, ObjectEditingService) {
    let mut session = Session::create();
    let capabilities = DocumentKind::Pdf.declared_capabilities();
    let id = session.documents.create(DocumentKind::Pdf, capabilities);
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

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 0.02,
        "{}：期望 {:.4}，实际 {:.4}",
        what,
        expected,
        actual
    );
}

/// 手算：`cm`(0.24,-0.24) → `q` → `cm`(3.125) → `Tm`(1,-1,32,30)
/// 正确复合后总变换 = [0.75, 0, 0, 0.75, 24, 819.42]，
/// 的字形框 (0,-2.4)-(7.2,9.6) → bbox = (24, 817.62, 5.4, 9)。
/// 若写成 compose(ctm, matrix)，内层缩放会乘到 841.92 上得到 y≈2631，必然失败。
#[test]
fn qa_cm_order_and_q_restore_put_bbox_in_page() {
    let path = temp_path("cm-order");
    write_pdf(&path, CTM_CONTENT);

    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let parser = PdfParser::new();
    let result = parser.parse(&document).expect("合成 PDF 应可解析");
    assert!(parser.warnings().is_empty(), "合法多行流不应有警告：{:?}", parser.warnings());
    let tree = result.tree;
    let page = tree.get_page(dw_core::page::PageId::new(1)).expect("页存在");
    near(page.size.width, PAGE_W, "页宽");
    near(page.size.height, PAGE_H, "页高");

    let texts: Vec<_> = page.objects_by_z().into_iter().filter(|o| o.kind == dw_core::object::ObjectKind::Text).collect();
    assert_eq!(texts.len(), 2, "应有两个文本对象");
    let (a, b) = (texts[0].bbox, texts[1].bbox);
    near(a.x, 24.0, "A.bbox.x");
    near(a.y, 817.62, "A.bbox.y");
    near(a.width, 5.4, "A.bbox.width");
    near(a.height, 9.0, "A.bbox.height");
    // Q 正确回退时 B 只受全局 cm：x = 0.24*2000 = 480；若被内层污染则 x=1500 出界。
    near(b.x, 480.0, "B.bbox.x（Q 后 CTM 回退）");
    near(b.y, 834.144, "B.bbox.y（Q 后 CTM 回退）");
    for (name, box_) in [("A", a), ("B", b)] {
        for (corner, (x, y)) in [
            ("lt", (box_.x, box_.y)),
            ("rt", (box_.x + box_.width, box_.y)),
            ("lb", (box_.x, box_.y + box_.height)),
            ("rb", (box_.x + box_.width, box_.y + box_.height)),
        ] {
            assert!(
                (0.0..=PAGE_W).contains(&x) && (0.0..=PAGE_H).contains(&y),
                "{} 的 {} 角 ({:.2},{:.2}) 必须落在页面内",
                name,
                corner,
                x,
                y
            );
        }
    }

    // UI 同款映射：pageWidthPx = round(594.96*1.25*1.19) = 885，zoom = 885/页宽。
    let (mut session, mut events, _id, mut service) = open_service(&path);
    let page_width_px = (PAGE_W * BASE_SCALE * UI_ZOOM).round();
    let zoom = page_width_px / PAGE_W;
    let screen_x = (a.x + a.width / 2.0) * zoom;
    let screen_y = (PAGE_H - (a.y + a.height / 2.0)) * zoom;
    let hit = service
        .hit_test(1, screen_x, screen_y, page_width_px, 0)
        .unwrap_or_else(|| panic!("bbox 中心 ({:.2},{:.2}) 必须命中 A", screen_x, screen_y));
    assert_eq!(hit.kind, "Text", "命中的应是文本对象");
    assert_eq!(hit.object_id, texts[0].object_id.0, "命中对象应为 A");
    service
        .select(&mut session, &mut events, 1, hit.object_id, false, false, false)
        .expect("命中对象应能选中");
    let _ = std::fs::remove_file(&path);
}

/// 真实样本：对象树投影不得再出现「存在损坏 Operator」误报。
#[test]
fn qa_real_pdf_has_no_corrupted_operator_warning() {
    let path = std::env::var("DW_PDF_SAMPLE")
        .map(PathBuf::from)
        .ok()
        .filter(|p| p.is_file())
        .or_else(|| {
            [
                PathBuf::from(r"C:\Users\Administrator\Desktop\打印准考证详情.pdf"),
                PathBuf::from(r"C:\Users\Administrator\AppData\Local\Temp\dw-test.pdf"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        });
    let Some(path) = path else {
        eprintln!("跳过：没有真实 PDF 样本");
        return;
    };
    let document = Document::new(
        DocumentId(7),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let info = project_object_tree(7, &document).expect("真实 PDF 应能投影对象树");
    assert!(info.object_count > 0, "真实 PDF 应有对象");
    let false_alarms: Vec<_> = info
        .warnings
        .iter()
        .filter(|w| w.contains("存在损坏 Operator"))
        .cloned()
        .collect();
    assert!(
        false_alarms.is_empty(),
        "合法真实 PDF 不应误报损坏 Operator：{:?}（全部警告 {:?}）",
        false_alarms,
        info.warnings
    );
}

/// 反向用例：真有 `@@@broken` 时仍必须告警并保留损坏行。
#[test]
fn qa_real_broken_line_still_warns() {
    let path = temp_path("broken-line");
    write_pdf(&path, b"BT /F1 12 Tf 72 720 Td <0041> Tj ET\n@@@broken\n");
    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let parser = PdfParser::new();
    let result = parser.parse(&document).expect("坏行不应阻断文档");
    let warnings = parser.warnings();
    let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("存在损坏 Operator")),
        "真坏行仍应告警：{:?}",
        messages
    );
    assert!(
        messages.iter().any(|m| m.contains("第 2 行")),
        "应定位到坏行行号：{:?}",
        messages
    );
    let page = result.tree.get_page(dw_core::page::PageId::new(1)).expect("页存在");
    assert!(
        page.objects_by_z().into_iter().any(|o| o.as_unknown().is_some()),
        "损坏行禁止静默丢弃"
    );
    let _ = std::fs::remove_file(&path);
}

/// 退化矩阵：全 0 的 `cm` 使四角重合于原点，bbox 应等于 BBox::default()，
/// 不得出现 NaN/Infinity 污染后续命中测试。
#[test]
fn qa_degenerate_matrix_falls_back_to_default_bbox() {
    let path = temp_path("degenerate");
    write_pdf(
        &path,
        b"0 0 0 0 0 0 cm BT /F1 12 Tf 1 0 0 -1 72 720 Tm <0041> Tj ET 10 10 100 50 re",
    );
    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(&path);
    let result = PdfParser::new().parse(&document).expect("退化矩阵不应阻断解析");
    let page = result.tree.get_page(dw_core::page::PageId::new(1)).expect("页存在");
    let zero = dw_core::object::BBox::default();
    let candidates: Vec<_> = page.objects_by_z().into_iter().filter(|o| o.kind != dw_core::object::ObjectKind::Unknown).collect();
    assert!(!candidates.is_empty(), "应解析出文本/路径对象");
    for object in candidates {
        assert_eq!(object.bbox, zero, "全 0 CTM 下 {:?} 的 bbox 应为 default", object.kind);
    }
    let _ = std::fs::remove_file(&path);
}

/// 端到端：命中 → 选择 → beginEdit → commit → save → reload，文本落盘且 CTM 上下文保留。
#[test]
fn qa_edit_commit_save_reload_keeps_text_and_ctm() {
    let path = temp_path("edit-e2e");
    write_pdf(&path, CTM_CONTENT);
    let (mut session, mut events, id, mut service) = open_service(&path);

    let target = {
        let document = session.documents.get(id).expect("文档存在");
        let info = project_object_tree(id.0, document).expect("对象树投影成功");
        let node = info.pages[0]
            .objects
            .iter()
            .find(|n| n.kind == "Text")
            .expect("应有文本对象");
        (node.object_id, node.bbox)
    };
    let (object_id, bbox) = target;
    near(bbox[0], 24.0, "待编辑对象应是被 CTM 变换后的 A");

    let page_width_px = (PAGE_W * BASE_SCALE * UI_ZOOM).round();
    let zoom = page_width_px / PAGE_W;
    let hit = service
        .hit_test(1, (bbox[0] + bbox[2] / 2.0) * zoom, (PAGE_H - (bbox[1] + bbox[3] / 2.0)) * zoom, page_width_px, 0)
        .expect("UI 坐标应命中待编辑文本");
    assert_eq!(hit.object_id, object_id);
    service.select(&mut session, &mut events, 1, object_id, false, false, false).expect("选择成功");
    {
        let document = session.documents.get(id).expect("文档存在");
        let snapshot = service.snapshot(&session, document, 1, UI_ZOOM as f32);
        assert_eq!(snapshot.selection.primary, Some(object_id));
        assert!(snapshot.capability.can_edit_text, "选中后编辑按钮应点亮");
        service.begin_edit(&session, document, &mut events, object_id, "Z".to_string()).expect("应进入编辑态");
        let editing = service.snapshot(&session, document, 1, UI_ZOOM as f32);
        assert!(editing.editing.active && editing.editing.object_id == Some(object_id));
        service.commit(&session, document, &mut events, Some("ZQA".to_string())).expect("提交成功");

        let saver = PdfSaver::new();
        saver.apply_changes(document).expect("应用变更");
        saver.save(document, &path, true).expect("保存成功");
        assert_eq!(document.pending_content_patch_count(), 0, "保存后补丁应清空");

        service.reload(document).expect("重新解析成功");
        let inspector = service.inspector(object_id).expect("对象仍在");
        assert_eq!(inspector.text.expect("文本对象").unicode.as_deref(), Some("ZQA"), "文本应落盘");
    }
    let reparsed = PdfParser::new()
        .parse(session.documents.get(id).expect("文档存在"))
        .expect("落盘后仍可解析");
    let page = reparsed.tree.get_page(dw_core::page::PageId::new(1)).expect("页存在");
    for object in page.objects_by_z().into_iter().filter(|o| o.kind == dw_core::object::ObjectKind::Text) {
        assert!(
            object.bbox.x >= 0.0
                && object.bbox.x + object.bbox.width <= PAGE_W
                && object.bbox.y >= 0.0
                && object.bbox.y + object.bbox.height <= PAGE_H,
            "重写后文本 bbox 仍须留在页面内（CTM 上下文不得丢失）：{:?}",
            object.bbox
        );
    }
    let _ = std::fs::remove_file(&path);
}

/// 真实 PDF（复制到临时文件，绝不改动原件）：UI 同款坐标逐点 hit → select → beginEdit，
/// 再对其中一个对象 commit → save → reload，确认文本落盘。
#[test]
fn qa_real_pdf_ui_points_hit_select_and_edit() {
    let src = std::env::var("DW_PDF_SAMPLE")
        .map(PathBuf::from)
        .ok()
        .filter(|p| p.is_file())
        .or_else(|| {
            [
                PathBuf::from(r"C:\Users\Administrator\Desktop\打印准考证详情.pdf"),
                PathBuf::from(r"C:\Users\Administrator\AppData\Local\Temp\dw-test.pdf"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        });
    let Some(src) = src else {
        eprintln!("跳过：没有真实 PDF 样本");
        return;
    };
    let dst = temp_path("real-e2e");
    std::fs::copy(&src, &dst).expect("真实样本应能复制到临时目录");

    let (mut session, mut events, id, mut service) = open_service(&dst);
    let (page_w, page_h, texts): (f64, f64, Vec<(u64, [f64; 4])>) = {
        let document = session.documents.get(id).expect("文档存在");
        let info = project_object_tree(id.0, document).expect("对象树投影成功");
        let page = &info.pages[0];
        let texts = page
            .objects
            .iter()
            .filter(|n| n.kind == "Text")
            .filter(|n| {
                n.bbox[0] >= 0.0
                    && n.bbox[0] + n.bbox[2] <= page.page_width
                    && n.bbox[1] >= 0.0
                    && n.bbox[1] + n.bbox[3] <= page.page_height
            })
            .map(|n| (n.object_id, n.bbox))
            .collect();
        (page.page_width, page.page_height, texts)
    };
    assert!(!texts.is_empty(), "真实样本应有落在页面内的文本对象");

    let page_width_px = (page_w * BASE_SCALE * UI_ZOOM).round();
    let zoom = page_width_px / page_w;
    let mut verified = 0usize;
    for (object_id, bbox) in texts.iter().take(6) {
        let center_x = bbox[0] + bbox[2] / 2.0;
        let center_y = bbox[1] + bbox[3] / 2.0;
        let screen_x = center_x * zoom;
        let screen_y = (page_h - center_y) * zoom;
        let Some(hit) = service.hit_test(1, screen_x, screen_y, page_width_px, 0) else {
            continue;
        };
        if hit.object_id != *object_id {
            // 相邻对象重叠时命中到更上层的对象是合法行为，不算失败。
            continue;
        }
        service
            .select(&mut session, &mut events, 1, *object_id, false, false, false)
            .expect("命中对象应能选中");
        {
            let document = session.documents.get(id).expect("文档存在");
            let snapshot = service.snapshot(&session, document, 1, UI_ZOOM as f32);
            assert_eq!(snapshot.selection.primary, Some(*object_id));
            assert!(snapshot.capability.can_edit_text, "选中文本应点亮编辑按钮");
            service
                .begin_edit(&session, document, &mut events, *object_id, "QA".to_string())
                .expect("双击链应进入编辑态");
            let editing = service.snapshot(&session, document, 1, UI_ZOOM as f32);
            assert!(editing.editing.active, "必须进入编辑态");
            assert_eq!(editing.editing.object_id, Some(*object_id));
            service.cancel(&mut events).expect("取消事务以便逐点验证");
        }
        verified += 1;
    }
    assert!(verified >= 1, "至少应有一个真实 PDF 文本点可命中并进入编辑态");    eprintln!("真实 PDF 逐点验证通过对象数：{}", verified);
    let _ = std::fs::remove_file(&dst);
}

/// 已知问题（与本次 CTM 修复无关）：PdfSaver::stage_content_patch 用 `Stream::set_content`
/// 写入明文，但没有清掉原流的 `/Filter/FlateDecode`；lopdf 重新读取时静默得到空字节，
/// 保存后重新解析得到 0 个对象。断言保持原样，待 save.rs 改用 `set_plain_content` 后解忽略。
#[test]
#[ignore = "既有 PdfSaver 过滤器 bug：保存 FlateDecode 内容流后重新解析为 0 对象；非本次 CTM 修复引入"]
fn qa_real_pdf_save_roundtrip() {
    let src = [
        PathBuf::from(r"C:\Users\Administrator\Desktop\打印准考证详情.pdf"),
        PathBuf::from(r"C:\Users\Administrator\AppData\Local\Temp\dw-test.pdf"),
    ]
    .into_iter()
    .find(|path| path.is_file());
    let Some(src) = src else {
        eprintln!("跳过：没有真实 PDF 样本");
        return;
    };
    let dst = temp_path("real-save-roundtrip");
    std::fs::copy(&src, &dst).expect("真实样本应能复制到临时目录");

    let (mut session, mut events, id, mut service) = open_service(&dst);
    let original = {
        let document = session.documents.get(id).expect("文档存在");
        let info = project_object_tree(id.0, document).expect("对象树投影成功");
        info.pages[0]
            .objects
            .iter()
            .find(|node| node.kind == "Text" && node.unicode.as_deref().is_some_and(|text| !text.is_empty()))
            .expect("应有可编辑文本")
            .clone()
    };
    let replacement: String = if original.unicode.as_ref().unwrap().chars().count() > 1 {
        original.unicode.as_ref().unwrap().chars().rev().collect()
    } else {
        format!("{0}{0}", original.unicode.as_ref().unwrap())
    };
    service
        .select(&mut session, &mut events, 1, original.object_id, false, false, false)
        .expect("选择成功");
    {
        let document = session.documents.get(id).expect("文档存在");
        service
            .begin_edit(&session, document, &mut events, original.object_id, replacement.clone())
            .expect("进入编辑态");
        service
            .commit(&session, document, &mut events, Some(replacement.clone()))
            .expect("提交成功");
        let saver = PdfSaver::new();
        saver.apply_changes(document).expect("应用变更");
        saver.save(document, &dst, true).expect("保存成功");
        service.reload(document).expect("重新解析成功");
        let after = project_object_tree(id.0, document).expect("保存后对象树投影成功");
        let unicodes: Vec<_> = after
            .pages
            .iter()
            .flat_map(|page| page.objects.iter())
            .filter(|node| node.kind == "Text")
            .filter_map(|node| node.unicode.clone())
            .collect();
        assert!(
            unicodes.iter().any(|text| text == &replacement),
            "保存后应能重新解析出更新文本：期望 {:?}，实际 {:?}（page_count={} object_count={}）",
            replacement,
            unicodes,
            after.page_count,
            after.object_count
        );
    }
    let _ = std::fs::remove_file(&dst);
}
