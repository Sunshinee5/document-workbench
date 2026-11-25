//! 18-02 PDF Object Parser 集成测试：PT-01 ~ PT-08。
//!
//! 样例 PDF 用 lopdf 在临时目录生成，参考 `metadata.rs` 的 `write_sample_pdf`。

use std::collections::BTreeMap;
use std::path::Path;

use dw_core::document::{Document, DocumentId, DocumentKind};
use dw_core::object::ObjectKind;
use dw_core::page::PageId;
use dw_pdf::parser::PdfParser;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

/// PT-03 单个对象的一致性签名：page_id / object_id.raw / z_index / bbox.x / bbox.y。
type IdSignature = (u32, u64, i32, f64, f64);

/// 样例变体：普通双页 / 同一字体引用 100 次 / 部分 Operator 损坏。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sample {
    /// 双页：文本、路径、未知 Operator。
    Normal,
    /// 单页：同一字体被引用 100 次。
    RepeatedFont,
    /// 单页：含一行非法 Operator。
    Corrupted,
}

/// 生成样例 PDF 到指定路径。
fn write_sample_pdf(path: &Path, sample: Sample) {
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

    let contents: Vec<String> = match sample {
        Sample::Normal => vec![
            "BT /F1 24 Tf 72 720 Td (Hello) Tj ET\n10 20 30 40 re\nq\n1 0 0 1 cm\nQ".to_string(),
            "BT /F1 12 Tf 72 700 Td (World) Tj ET\nUnknownOp".to_string(),
        ],
        Sample::RepeatedFont => {
            vec!["BT\n".to_string() + &"/F1 12 Tf 72 700 Td (a) Tj\n".repeat(100) + "ET"]
        }
        Sample::Corrupted => {
            vec!["BT /F1 12 Tf 72 700 Td (ok) Tj ET\n@@@ broken\n10 20 30 40 re".to_string()]
        }
    };

    let mut page_ids = Vec::new();
    for content in contents {
        let content_id = pdf.add_object(Stream::new(dictionary! {}, content.into_bytes()));
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
    let info_id = pdf.add_object(dictionary! {
        "Title" => Object::string_literal("解析测试"),
        "Author" => Object::string_literal("寇豆码"),
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.trailer.set("Info", info_id);
    pdf.compress();
    pdf.save(path).expect("样例 PDF 应能保存");
}

/// 构造带 source_path 的领域文档。
fn core_document(path: &Path) -> Document {
    let document = Document::new(
        DocumentId(1),
        DocumentKind::Pdf,
        DocumentKind::Pdf.declared_capabilities(),
    );
    document.set_source_path(path);
    document
}

/// 临时文件路径：按测试名固定，避免重复解析时文件被并发覆盖。
fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dw-pdf-parser-{}.pdf", name))
}

/// 收集全部对象的稳定快照：页码 / 对象号 / 层级 / 边界。
fn object_snapshot(tree: &dw_core::object_tree::ObjectTree) -> Vec<(u32, u64, i32, f64, f64)> {
    let mut snapshot = Vec::new();
    for (page_id, page) in &tree.pages {
        for object in page.objects_by_z() {
            snapshot.push((
                page_id.raw(),
                object.object_id.raw(),
                object.z_index,
                object.bbox.x,
                object.bbox.y,
            ));
        }
    }
    snapshot
}

/// PT-01：Metadata 正确解析（复用 17，无第二套解析）。
#[test]
fn pt01_metadata_is_parsed() {
    let path = temp_path("pt01");
    write_sample_pdf(&path, Sample::Normal);
    let parser = PdfParser::new();
    parser.parse(&core_document(&path)).expect("解析应成功");

    let metadata = parser.metadata().expect("元数据应存在");
    assert_eq!(metadata.page_count, 2);
    assert_eq!(metadata.title, "解析测试");
    assert_eq!(metadata.author.as_deref(), Some("寇豆码"));
    assert!(metadata.file_size > 0);

    let _ = std::fs::remove_file(&path);
}

/// PT-02：ObjectTree 页面与对象数量正确。
#[test]
fn pt02_object_tree_counts() {
    let path = temp_path("pt02");
    write_sample_pdf(&path, Sample::Normal);
    let parser = PdfParser::new();
    let tree = parser.parse(&core_document(&path)).expect("解析应成功");

    assert_eq!(tree.page_count(), 2);
    assert_eq!(tree.parsed_page_count(), 2);
    assert_eq!(tree.object_count(), 7);
    assert_eq!(
        tree.get_page(PageId::new(1))
            .expect("第一页应存在")
            .objects
            .len(),
        5
    );
    assert_eq!(
        tree.get_page(PageId::new(2))
            .expect("第二页应存在")
            .objects
            .len(),
        2
    );

    let _ = std::fs::remove_file(&path);
}

/// PT-03：同一 PDF 连续解析 10 次，ObjectId / BBox / Object Order / PageId 一致。
#[test]
fn pt03_stable_ids_across_ten_parses() {
    let path = temp_path("pt03");
    write_sample_pdf(&path, Sample::Normal);
    let document = core_document(&path);
    let mut baseline: Option<Vec<IdSignature>> = None;

    for _ in 0..10 {
        let parser = PdfParser::new();
        let tree = parser.parse(&document).expect("重复解析应成功");
        let snapshot = object_snapshot(&tree);
        assert_eq!(snapshot.len(), 7);
        match &baseline {
            None => baseline = Some(snapshot),
            Some(expected) => assert_eq!(&snapshot, expected, "连续解析结果必须完全一致"),
        }
    }

    let _ = std::fs::remove_file(&path);
}

/// PT-04：未知 Operator 建立 UnknownObject，operator 与操作数均保留。
#[test]
fn pt04_unknown_operators_are_preserved() {
    let path = temp_path("pt04");
    write_sample_pdf(&path, Sample::Normal);
    let parser = PdfParser::new();
    let tree = parser.parse(&core_document(&path)).expect("解析应成功");

    let first = tree.get_page(PageId::new(1)).expect("第一页应存在");
    let unknown: Vec<&str> = first
        .objects_by_z()
        .iter()
        .filter(|object| object.kind == ObjectKind::Unknown)
        .filter_map(|object| object.as_unknown().map(|item| item.operator.as_str()))
        .collect();
    assert_eq!(unknown, vec!["q", "cm", "Q"]);
    assert_eq!(
        first
            .objects_by_z()
            .iter()
            .find(|object| object.kind == ObjectKind::Text)
            .and_then(|object| object.as_text())
            .map(|text| text.unicode.as_deref()),
        Some(Some("Hello"))
    );

    let _ = std::fs::remove_file(&path);
}

/// PT-05：同一 Font 引用 100 次只解析一次。
#[test]
fn pt05_font_is_resolved_once() {
    let path = temp_path("pt05");
    write_sample_pdf(&path, Sample::RepeatedFont);
    let parser = PdfParser::new();
    let tree = parser.parse(&core_document(&path)).expect("解析应成功");

    assert_eq!(parser.last_font_resolve_count(), 1);
    assert_eq!(
        tree.get_page(PageId::new(1))
            .expect("第一页应存在")
            .objects
            .len(),
        100
    );

    let _ = std::fs::remove_file(&path);
}

/// PT-06：首开只解析第一页，其它页保留空壳。
#[test]
fn pt06_lazy_parse_only_first_page() {
    let path = temp_path("pt06");
    write_sample_pdf(&path, Sample::Normal);
    let parser = PdfParser::new();
    let tree = parser
        .parse_first_page(&core_document(&path))
        .expect("懒解析应成功");

    assert_eq!(tree.page_count(), 2);
    assert_eq!(tree.parsed_page_count(), 1);
    assert!(tree.is_parsed(PageId::new(1)));
    assert!(!tree.is_parsed(PageId::new(2)));
    assert_eq!(
        tree.get_page(PageId::new(2))
            .expect("第二页空壳应存在")
            .objects
            .len(),
        0
    );

    let _ = std::fs::remove_file(&path);
}

/// PT-07：重复访问同一页直接命中缓存；失效后按页重新解析。
#[test]
fn pt07_page_cache_hit_and_invalidation() {
    let path = temp_path("pt07");
    write_sample_pdf(&path, Sample::Normal);
    let parser = PdfParser::new();
    parser
        .parse_first_page(&core_document(&path))
        .expect("懒解析应成功");

    let first = parser.parse_page(PageId::new(1)).expect("首页应可读");
    let second = parser.parse_page(PageId::new(1)).expect("缓存页应可读");
    assert_eq!(first, second);
    assert_eq!(parser.parse_count(PageId::new(1)), 1);

    let page_two = parser.parse_page(PageId::new(2)).expect("第二页应按需解析");
    assert_eq!(page_two.objects.len(), 2);
    assert_eq!(parser.parse_count(PageId::new(2)), 1);
    assert_eq!(parser.tree().parsed_page_count(), 2);

    parser.invalidate_page(PageId::new(1));
    assert!(!parser.tree().is_parsed(PageId::new(1)));
    parser.parse_page(PageId::new(1)).expect("失效后应重新解析");
    assert_eq!(parser.parse_count(PageId::new(1)), 2);
    assert_eq!(parser.tree().parsed_page_count(), 2);

    let _ = std::fs::remove_file(&path);
}

/// PT-08：部分 Operator 损坏仍能打开，Warning 正确且不丢弃损坏行。
#[test]
fn pt08_corrupted_operations_are_recovered() {
    let path = temp_path("pt08");
    write_sample_pdf(&path, Sample::Corrupted);
    let parser = PdfParser::new();
    let tree = parser
        .parse(&core_document(&path))
        .expect("损坏页不应阻断文档");

    assert_eq!(tree.page_count(), 1);
    assert_eq!(tree.parsed_page_count(), 1);
    let warnings = parser.warnings();
    assert!(warnings
        .iter()
        .any(|item| item.message.contains("逐行恢复")));
    assert!(warnings.iter().any(|item| item.message.contains("第 2 行")));

    let page = tree.get_page(PageId::new(1)).expect("第一页应存在");
    let corrupted: Vec<&str> = page
        .objects_by_z()
        .iter()
        .filter_map(|object| object.as_unknown().map(|item| item.operator.as_str()))
        .collect();
    assert!(corrupted.contains(&"<corrupted>"), "损坏行禁止静默丢弃");
    assert!(page
        .objects_by_z()
        .iter()
        .any(|object| object.kind == ObjectKind::Path));

    let _ = std::fs::remove_file(&path);
}

/// Parser 及子组件满足 Send + Sync，可在后台线程调用。
#[test]
fn parser_components_are_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PdfParser>();
    assert_send_sync::<dw_pdf::parse_context::ParseContext>();
    assert_send_sync::<dw_pdf::intermediate::IntermediateTree>();

    // 页索引类型不依赖具体 PDF，保证缓存结构可在线程间传递。
    let _: BTreeMap<PageId, (u32, u16)> = BTreeMap::new();
}
