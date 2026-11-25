//! 规范 16 C-8 对象树只读投影测试：OTV-01 ~ OTV-04。
//!
//! 样例 PDF 用 lopdf 在临时目录生成（与 `parser_tests.rs` 同一手法）。
//! 本组测试只验证「投影正确 + 只读」两件事，不验证编辑能力。

use std::path::Path;

use dw_core::document::{Document, DocumentId, DocumentKind, DocumentSaveState};
use dw_pdf::object_tree_view::project_object_tree;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

/// 生成两页样例：第 1 页文本 + 矩形，第 2 页文本。
fn write_sample_pdf(path: &Path) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = pdf.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let contents = [
        "BT /F1 24 Tf 72 720 Td (Hello) Tj ET\n10 20 30 40 re",
        "BT /F1 12 Tf 72 700 Td (World) Tj ET",
    ];

    let mut page_ids = Vec::new();
    for content in contents {
        let content_id = pdf.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
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
    pdf.compress();
    pdf.save(path).expect("样例 PDF 应能保存");
}

/// 构造带 source_path 的领域文档。
fn core_document(path: &Path) -> Document {
    let document = Document::new(
        DocumentId(7),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(path);
    document
}

/// 临时文件路径：按测试名固定。
fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dw-pdf-object-tree-{}.pdf", name))
}

/// OTV-01：页数 / 页码基准 / 对象计数正确，页码从 1 起。
#[test]
fn otv01_pages_and_counts() {
    let path = temp_path("otv01");
    write_sample_pdf(&path);
    let info = project_object_tree(7, &core_document(&path)).expect("投影应成功");

    assert_eq!(info.document_id, 7);
    assert_eq!(info.page_count, 2);
    assert_eq!(info.pages.len(), 2);
    assert_eq!(info.pages[0].page, 1);
    assert_eq!(info.pages[1].page, 2);
    assert!(info.object_count > 0);
    assert_eq!(
        info.object_count,
        info.pages.iter().map(|page| page.object_count).sum::<usize>()
    );
    for page in &info.pages {
        assert_eq!(page.objects.len(), page.object_count);
        // 页尺寸用于 Overlay 坐标映射，必须与 MediaBox 一致。
        assert!((page.page_width - 595.0).abs() < 0.01);
        assert!((page.page_height - 842.0).abs() < 0.01);
    }

    let _ = std::fs::remove_file(&path);
}

/// OTV-02：每个对象节点字段完整，且 page 与所属页一致。
#[test]
fn otv02_node_fields() {
    let path = temp_path("otv02");
    write_sample_pdf(&path);
    let info = project_object_tree(7, &core_document(&path)).expect("投影应成功");

    for page in &info.pages {
        for node in &page.objects {
            assert_eq!(node.page, page.page);
            assert!(!node.kind.is_empty());
            assert!(!node.kind_label.is_empty());
            assert!(!node.lifecycle.is_empty());
            // bbox 恒为四元组，UI 直接消费。
            assert_eq!(node.bbox.len(), 4);
            assert!(node.bbox[2] >= 0.0 && node.bbox[3] >= 0.0);
        }
    }

    // 文本对象必须带字体名与字号；非文本对象不得带字体。
    let text = info
        .pages
        .iter()
        .flat_map(|page| page.objects.iter())
        .find(|node| node.kind == "Text")
        .expect("样例应含文本对象");
    assert_eq!(text.font_id.as_deref(), Some("F1"));
    assert!(text.font_size.unwrap_or(0.0) > 0.0);
    assert!(text.glyph_count > 0);

    let non_text = info
        .pages
        .iter()
        .flat_map(|page| page.objects.iter())
        .find(|node| node.kind != "Text");
    if let Some(node) = non_text {
        assert!(node.font_id.is_none());
        assert!(node.unicode.is_none());
        assert_eq!(node.glyph_count, 0);
    }

    let _ = std::fs::remove_file(&path);
}

/// OTV-03：投影是只读的——不产生 Command、不改 Document 状态、不写盘。
#[test]
fn otv03_projection_is_read_only() {
    let path = temp_path("otv03");
    write_sample_pdf(&path);
    let document = core_document(&path);
    let before_state = document.save_state();
    let before_layout = document.page_layout().len();
    let before_len = std::fs::metadata(&path).expect("文件应存在").len();

    let _ = project_object_tree(7, &document).expect("投影应成功");

    assert_eq!(document.save_state(), before_state);
    assert_eq!(document.save_state(), DocumentSaveState::Clean);
    assert_eq!(document.page_layout().len(), before_layout);
    assert_eq!(
        std::fs::metadata(&path).expect("文件应存在").len(),
        before_len
    );

    let _ = std::fs::remove_file(&path);
}

/// OTV-04：无源文件路径时返回中文错误，不 panic。
#[test]
fn otv04_missing_source_path() {
    let document = Document::new(
        DocumentId(7),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    let error = project_object_tree(7, &document).expect_err("无源文件路径应失败");
    assert!(error.message.contains("路径") || error.message.contains("文件"));
}
