//! 18-03 Object Mapper 集成测试：MT-01 ~ MT-08（spec/09 §二十二）。
//!
//! 样例 PDF 在临时目录用 lopdf 生成；映射只经过 ObjectMapper，
//! 测试不引入第二套 Mapper，也不触碰 Save / Driver。

use std::path::Path;

use dw_core::document::{Document, DocumentId, DocumentKind};
use dw_core::object::{
    BBox, FontRef, ImageRef, Object as DomainObject, ObjectId, ObjectKind, ObjectLifecycle,
    PathObject, TextObject, Transform, UnknownObject,
};
use dw_core::object_tree::ObjectTree;
use dw_core::page::PageId;
use dw_pdf::intermediate::{IntermediateImage, IntermediateNode, IntermediateRef, ParseWarning};
use dw_pdf::object_map::{MapperSchemaVersion, ObjectMapper};
use dw_pdf::parser::PdfParser;
use lopdf::{dictionary, Document as Pdf, Object as LopdfObject, Stream};
use serde_json::{json, Value};

/// 本测试只覆盖映射层，错误语义统一由 DriverError 承担。
type MapResult<T> = Result<T, dw_core::error::DriverError>;

/// 构造中间节点的小工具：来源固定为 xref=42，避免随机 ID。
fn sample_node(operator: &str, operands: Vec<Value>, z_index: i32, bbox: BBox) -> IntermediateNode {
    IntermediateNode::new(
        IntermediateRef::new(42, 0, 0),
        z_index.max(0) as u32,
        operator,
        operands,
        z_index,
        bbox,
    )
}

/// 生成只含一页的样例 PDF，返回内容流 xref 供 Round Trip 原地替换。
fn write_sample(path: &Path, content: &str) -> (u32, u16) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let content_id = pdf.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page_id = pdf.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    pdf.objects.insert(
        pages_id,
        LopdfObject::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![LopdfObject::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    let info_id = pdf.add_object(dictionary! {
        "Title" => LopdfObject::string_literal("映射测试"),
        "Author" => LopdfObject::string_literal("寇豆码"),
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.trailer.set("Info", info_id);
    pdf.save(path).expect("样例 PDF 应能保存");
    content_id
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

/// 临时路径按测试名固定，避免并行测试互相覆盖。
fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dw-pdf-mapper-{}.pdf", name))
}

/// 按绘制顺序收集对象签名：ID / BBox / Version / Kind。
type Signature = (u64, f64, f64, f64, f64, u64, ObjectKind);

/// 对象树签名：页码 + 绘制顺序 + 四字段，便于 Round Trip 直接比较。
fn signatures(tree: &ObjectTree) -> Vec<(u32, Signature)> {
    let mut result = Vec::new();
    for (page_id, page) in &tree.pages {
        for object in page.objects_by_z() {
            result.push((
                page_id.raw(),
                (
                    object.object_id.raw(),
                    object.bbox.x,
                    object.bbox.y,
                    object.bbox.width,
                    object.bbox.height,
                    object.version,
                    object.kind,
                ),
            ));
        }
    }
    result
}

/// MT-01：文本中间节点完整映射，反向生成 `TJ`。
#[test]
fn mt01_text_mapping() {
    let mapper = ObjectMapper::new();
    let bbox = BBox::new(10.0, 20.0, 30.0, 12.0);
    let mut node = sample_node("Tj", vec![json!((b"Hi".to_vec()))], 0, bbox);
    node.font_id = Some("F1".to_string());
    node.glyphs = vec![72, 105];
    node.unicode = Some("Hi".to_string());
    node.transform = Transform::new(1.0, 0.0, 0.0, 1.0, 10.0, 20.0);

    let object = mapper
        .to_editor_node(&node, PageId::new(1))
        .expect("文本应可映射");
    assert_eq!(object.kind, ObjectKind::Text);
    assert_eq!(object.bbox, bbox);
    assert_eq!(object.version, 1);
    assert_eq!(object.lifecycle, ObjectLifecycle::Loaded);
    let text = object.as_text().expect("文本载荷应存在");
    assert_eq!(text.font_id, "F1");
    assert_eq!(text.font, FontRef::new("F1"));
    assert_eq!(text.glyphs, vec![72, 105]);
    assert_eq!(text.unicode.as_deref(), Some("Hi"));

    let back = mapper.to_intermediate(&object).expect("文本应可反向映射");
    assert_eq!(back.operator, "TJ");
    assert_eq!(back.glyphs, text.glyphs);
    assert_eq!(back.font_id.as_deref(), Some("F1"));
}

/// MT-02：图片中间节点完整映射，反向生成 `Do` 并保留资源名。
#[test]
fn mt02_image_mapping() {
    let mapper = ObjectMapper::new();
    let bbox = BBox::new(0.0, 0.0, 100.0, 50.0);
    let mut node = sample_node("Do", vec![json!("/Im0")], 1, bbox);
    node.image = Some(IntermediateImage {
        xref: 42,
        width: 100,
        height: 50,
        colorspace: "DeviceRGB".to_string(),
    });

    let object = mapper
        .to_editor_node(&node, PageId::new(1))
        .expect("图片应可映射");
    assert_eq!(object.kind, ObjectKind::Image);
    assert_eq!(object.version, 1);
    let image = object.as_image().expect("图片载荷应存在");
    assert_eq!(image.xref, 42);
    assert_eq!(image.resource, ImageRef::new("Im0").with_xref(42));
    assert_eq!(image.color_space.name, "DeviceRGB");

    let back = mapper.to_intermediate(&object).expect("图片应可反向映射");
    assert_eq!(back.operator, "Do");
    assert_eq!(back.operands, vec![json!("/Im0")]);
    assert_eq!(back.image.expect("图片描述应保留").xref, 42);
}

/// MT-03：未知 Operator 与损坏行原样保留，反向写回不丢操作数。
#[test]
fn mt03_unknown_object_is_preserved() {
    let mapper = ObjectMapper::new();
    let operands = vec![json!(1), json!(0), json!(0), json!(1)];
    let node = sample_node("cm", operands.clone(), 2, BBox::default());
    let object = mapper
        .to_editor_node(&node, PageId::new(1))
        .expect("未知对象应可映射");
    assert_eq!(object.kind, ObjectKind::Unknown);
    assert_eq!(object.stream_ref, Some(42));
    let unknown = object.as_unknown().expect("未知载荷应存在");
    assert_eq!(unknown.operator, "cm");
    assert_eq!(unknown.operands, operands);
    assert_eq!(unknown.stream_ref, Some(42));

    let back = mapper
        .to_intermediate(&object)
        .expect("未知对象应可反向映射");
    assert_eq!(back.operator, "cm");
    assert_eq!(back.operands, operands);
    assert_eq!(back.stream_ref, Some(42));
    assert_eq!(back.object_id, object.object_id);

    let corrupted = DomainObject::unknown(
        ObjectId(7),
        PageId::new(1),
        BBox::default(),
        3,
        None,
        UnknownObject::new("<corrupted>", vec![json!("@@@ broken")], None),
    );
    let back = mapper
        .to_intermediate(&corrupted)
        .expect("损坏行应可原样写回");
    assert_eq!(back.operator, "<corrupted>");
    assert_eq!(back.operands, vec![json!("@@@ broken")]);
}

/// MT-04：PDF → Object → PDF → Object，四字段完全一致（AD-4 核心验收）。
#[test]
fn mt04_round_trip_keeps_four_fields() {
    let path = temp_path("mt04");
    let rewritten = path.with_extension("roundtrip.pdf");
    let xref = write_sample(&path, "(Hi) TJ\nq\n1 0 0 1 cm\n10 20 30 40 re");
    let parser = PdfParser::new();
    let result = parser
        .parse(&core_document(&path))
        .expect("样例 PDF 应可解析");
    assert_eq!(result.tree.page_count(), 1);
    assert_eq!(result.tree.parsed_page_count(), 1);
    let first = signatures(&result.tree);
    assert_eq!(first.len(), 4, "TJ / q / cm / re 应各映射一个对象");

    let mapper = ObjectMapper::new();
    let objects: Vec<DomainObject> = result.objects().cloned().collect();
    let content = mapper
        .to_content_stream(&objects)
        .expect("内容流应可反向生成");
    let operators: Vec<&str> = content
        .operations
        .iter()
        .map(|operation| operation.operator.as_str())
        .collect();
    assert_eq!(operators, vec!["TJ", "q", "cm", "re"]);
    let bytes = content.encode().expect("内容流应可编码为 PDF 字节");

    // 原地替换同 xref 的内容流：ID 的来源（xref / 流序号 / Operator 下标）保持不变。
    let mut pdf = Pdf::load(&path).expect("样例 PDF 应可重新打开");
    match pdf.objects.get_mut(&xref) {
        Some(LopdfObject::Stream(stream)) => {
            stream.content = bytes;
            stream.dict.set("Length", stream.content.len() as i64);
        }
        _ => panic!("内容流 xref 应指向 Stream"),
    }
    pdf.save(&rewritten).expect("往返 PDF 应能保存");

    let reparsed = PdfParser::new()
        .parse(&core_document(&rewritten))
        .expect("往返 PDF 应可再次解析");
    assert_eq!(signatures(&reparsed.tree), first, "Round Trip 四字段须一致");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&rewritten);
}

/// MT-05：SchemaVersion 位于 Mapper，旧数据可升级，Recovery 不失败。
#[test]
fn mt05_schema_version_and_compatibility() {
    let current = MapperSchemaVersion::current();
    assert_eq!(current, MapperSchemaVersion::V1);
    assert_eq!(current.as_u32(), 1);
    assert_eq!(current.label(), "对象映射版本 1");
    assert!(current.can_upgrade_from(None));
    assert!(current.can_upgrade_from(Some(MapperSchemaVersion::V1)));
    assert_eq!(
        MapperSchemaVersion::upgrade(None).expect("旧 Schema 应可升级"),
        current
    );

    // 无版本旧对象（默认 PathObject）升级后仍可完成双向映射。
    let mapper = ObjectMapper::new();
    let object = DomainObject::path(
        ObjectId(3),
        PageId::new(1),
        BBox::new(1.0, 2.0, 3.0, 4.0),
        0,
        None,
        PathObject::default(),
    );
    let intermediate = mapper
        .to_intermediate(&object)
        .expect("旧数据应可恢复为中间节点");
    assert_eq!(intermediate.operator, "re");
    assert_eq!(intermediate.object_id, object.object_id);
}

/// MT-06：Version 在双向映射中原样同步，不被 Save 自行推导。
#[test]
fn mt06_version_is_synced() {
    let mapper = ObjectMapper::new();
    let mut node = sample_node("Tj", vec![json!("x")], 0, BBox::default());
    node.version = 7;
    let object = mapper
        .to_editor_node(&node, PageId::new(1))
        .expect("文本节点应可映射");
    assert_eq!(object.version, 7);

    let mut object = object;
    object.mark_modified();
    assert_eq!(object.version, 8);
    let back = mapper
        .to_intermediate(&object)
        .expect("修改后仍应可反向映射");
    assert_eq!(back.version, 8);
    assert_eq!(back.version, object.version);
}

/// MT-07：Lifecycle 由 Mapper 同步；Deleted 对象不写回内容流。
#[test]
fn mt07_lifecycle_is_synced() {
    let mapper = ObjectMapper::new();
    let mut node = sample_node("q", Vec::new(), 0, BBox::default());
    node.lifecycle = ObjectLifecycle::Modified;
    let object = mapper
        .to_editor_node(&node, PageId::new(1))
        .expect("未知节点应可映射");
    assert_eq!(object.lifecycle, ObjectLifecycle::Modified);
    let back = mapper
        .to_intermediate(&object)
        .expect("未知节点应可反向映射");
    assert_eq!(back.lifecycle, ObjectLifecycle::Modified);

    let mut deleted = object.clone();
    deleted.mark_deleted();
    let content = mapper
        .to_content_stream(&[object, deleted])
        .expect("内容流应可生成");
    assert_eq!(content.operations.len(), 1, "已删除对象禁止写回");
}

/// MT-08：同一页只映射一次（Parser 缓存命中），报告统计与警告同步产出。
#[test]
fn mt08_page_maps_once_and_report_is_ready() {
    let path = temp_path("mt08");
    write_sample(&path, "(ok) Tj\n@@@ broken");
    let parser = PdfParser::new();
    let result = parser
        .parse_first_page(&core_document(&path))
        .expect("懒解析应成功");
    assert_eq!(result.tree.parsed_page_count(), 1);
    assert_eq!(result.tree.page_count(), 1);
    assert_eq!(
        result.report.warnings.len(),
        2,
        "逐行恢复与坏行各产生一条警告"
    );
    assert_eq!(result.report.unknown_object_count, 1);
    assert_eq!(result.report.corrupted_object_count, 2);
    assert_eq!(result.report.recovered_object_count, 2);
    assert_eq!(result.warnings().count(), 2);
    assert_eq!(result.objects().count(), 2);
    let warnings: Vec<ParseWarning> = parser.warnings();
    assert_eq!(warnings.len(), 2);

    for _ in 0..5 {
        let page = parser
            .parse_page(PageId::new(1))
            .expect("缓存页应可重复读取");
        assert_eq!(page.objects.len(), 2);
    }
    assert_eq!(parser.parse_count(PageId::new(1)), 1, "同一页只解析一次");

    let _ = std::fs::remove_file(&path);
}

/// 文本对象拥有默认字体引用：无资源文件时仍能反向生成稳定 `TJ`。
#[test]
fn text_default_font_reference_is_stable() {
    let mapper = ObjectMapper::new();
    let object = DomainObject::text(
        ObjectId(1),
        PageId::new(1),
        BBox::default(),
        0,
        None,
        TextObject::new(String::new(), vec![65], None),
    );
    assert_eq!(
        object.as_text().expect("文本载荷应存在").font,
        FontRef::default()
    );
    let map: MapResult<IntermediateNode> = mapper.to_intermediate(&object);
    let node = map.expect("默认字体也应可映射");
    assert_eq!(node.operator, "TJ");
    assert_eq!(node.glyphs, vec![65]);
}
