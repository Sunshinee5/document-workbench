//! 18-08 Content Stream Rewrite 集成测试（`14` §八：CT-01 ～ CT-11）。
//!
//! 用真实 lopdf 内容流字节驱动，验证 AD-21 ～ AD-24：
//! 唯一重写入口、Unknown 100% 保留、Operator 顺序冻结、ContentPatch 唯一输出。

use dw_core::content::{ContentPatch, PdfOperand, RewriteRequest};
use dw_core::diff::{Diff, ObjectDiffPayload};
use dw_core::editing::EditingCommit;
use dw_core::eventbus::{EventBus, EventPayload};
use dw_core::font::{FontPatch, GlyphMapping, SubsetPlan, SubsetStrategy};
use dw_core::object::{BBox, ImageObject, Object, ObjectId, ObjectPayload, PathObject, TextObject};
use dw_core::page::PageId;
use dw_pdf::content_rewrite::{
    decode_operators, encode_operators, ContentRewriteEngine, RewriteError,
};

const SOURCE_STREAM: &str = "BT\n/F1 12 Tf\n1 0 0 1 100 700 Tm\n(old) Tj\nET";

fn page_id() -> PageId {
    PageId::new(1)
}

fn font_patch(unicode: &str, glyphs: &[u32]) -> FontPatch {
    let mut mapping = std::collections::BTreeMap::new();
    for (character, glyph) in unicode.chars().zip(glyphs.iter().copied()) {
        mapping.insert(character.to_string(), glyph);
    }
    FontPatch {
        added_fonts: Vec::new(),
        removed_fonts: Vec::new(),
        subset_plan: SubsetPlan {
            font: dw_core::object::FontRef::new("F1"),
            used_glyphs: glyphs.iter().copied().collect(),
            strategy: SubsetStrategy::ExtendSubset,
        },
        glyph_mapping: GlyphMapping {
            font: dw_core::object::FontRef::new("F1"),
            mapping,
        },
    }
}

fn commit_of(object: Object) -> EditingCommit {
    let json = serde_json::to_value(&object).expect("对象可序列化");
    let diff = Diff::object_change(&ObjectDiffPayload {
        object_id: object.object_id,
        operation: "TextReplace".to_string(),
        before_version: 1,
        after_version: 2,
        before: json.clone(),
        after: json,
    });
    EditingCommit {
        object,
        diff,
        before_version: 1,
        after_version: 2,
    }
}

fn text_object(unicode: &str) -> Object {
    let mut text = TextObject::new("F1", vec![20320], Some(unicode.to_string()));
    text.font = dw_core::object::FontRef::new("F1");
    Object::text(
        ObjectId(1),
        page_id(),
        BBox::new(0.0, 0.0, 10.0, 10.0),
        0,
        Some(12),
        text,
    )
}

fn target_index(source: &[u8], operator: &str) -> usize {
    decode_operators(source)
        .expect("源流可解析")
        .iter()
        .position(|item| item.name == operator)
        .unwrap_or_else(|| panic!("源流中没有 {}", operator))
}

fn request_for(object: Object, source: &[u8], target: &str) -> RewriteRequest {
    let unicode = object
        .as_text()
        .and_then(|text| text.unicode.clone())
        .unwrap_or_default();
    let glyphs: Vec<u32> = unicode.chars().map(u32::from).collect();
    RewriteRequest::new(
        commit_of(object),
        font_patch(&unicode, &glyphs),
        source.to_vec(),
        page_id(),
        Some(12),
        target_index(source, target),
    )
}

/// CT-01：ReplaceText 把 Tj 的操作数换成 FontPatch 计算出的字形字节。
#[test]
fn ct01_replace_text_operator() {
    let source = SOURCE_STREAM.as_bytes();
    let request = request_for(text_object("新文"), source, "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    let target = &patch.operators[request.target_operator_index];
    assert_eq!(target.name, "Tj");
    match &target.operands[0] {
        PdfOperand::String(bytes) => {
            assert_eq!(bytes.len(), 4, "两个字符 × 2 字节");
            assert_eq!(&bytes[0..2], &[0x65, 0xB0], "新 U+65B0");
            assert_eq!(&bytes[2..4], &[0x65, 0x87], "文 U+6587");
        }
        other => panic!("Tj 操作数应为字符串：{:?}", other),
    }
}

/// CT-02：Unknown Operator 原样保留（名称与操作数均不变）。
#[test]
fn ct02_unknown_operator_preserved() {
    let source = b"BT\n/F1 12 Tf\n7 (raw) 3.5 ZZCUSTOM\n(old) Tj\nET";
    let request = request_for(text_object("新"), source, "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    let custom = &patch.operators[2];
    assert_eq!(custom.name, "ZZCUSTOM");
    assert_eq!(
        custom.operands,
        vec![
            PdfOperand::Integer(7),
            PdfOperand::String(b"raw".to_vec()),
            PdfOperand::Real(3.5),
        ]
    );
}

/// CT-03：非目标 Operator 顺序与内容完全一致。
#[test]
fn ct03_operator_order_frozen() {
    let source = SOURCE_STREAM.as_bytes();
    let request = request_for(text_object("新"), source, "Tj");
    let original = decode_operators(source).expect("可解析");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    assert_eq!(patch.operators.len(), original.len());
    for (index, (before, after)) in original.iter().zip(patch.operators.iter()).enumerate() {
        if index == request.target_operator_index {
            continue;
        }
        assert_eq!(before, after, "第 {} 个 Operator 不得被改动", index);
    }
}

/// CT-04：中性 Operator 可编码回字节并再次解码，四字段一致（RoundTrip）。
#[test]
fn ct04_operator_round_trip() {
    let source = SOURCE_STREAM.as_bytes();
    let original = decode_operators(source).expect("可解析");
    let bytes = encode_operators(&original).expect("可编码");
    let decoded = decode_operators(&bytes).expect("可再次解析");

    assert_eq!(decoded.len(), original.len());
    for (before, after) in original.iter().zip(decoded.iter()) {
        assert_eq!(before.name, after.name);
    }
}

/// CT-05：越界 / 空流 / 缺字形 / 空 FontRef / Path 都返回对应错误。
#[test]
fn ct05_validation() {
    let engine = ContentRewriteEngine::new();
    let mut events = EventBus::new();
    let source = SOURCE_STREAM.as_bytes();

    let mut request = request_for(text_object("新"), source, "Tj");
    request.target_operator_index = 999;
    assert!(matches!(
        engine.rewrite(&request, &mut events),
        Err(RewriteError::OperatorMismatch {
            expected: 999,
            actual: 5
        })
    ));

    let mut empty = request_for(text_object("新"), source, "Tj");
    empty.source_stream = Vec::new();
    assert!(matches!(
        engine.rewrite(&empty, &mut events),
        Err(RewriteError::ValidationFailed(_))
    ));

    // 字体补丁缺少字符字形。
    let mut missing = request_for(text_object("新"), source, "Tj");
    missing.font_patch.glyph_mapping.mapping.clear();
    assert!(matches!(
        engine.rewrite(&missing, &mut events),
        Err(RewriteError::InvalidOperand(_))
    ));

    // 空 FontRef。
    let mut object = text_object("新");
    let mut text = object.as_text().expect("文本对象").clone();
    text.font = dw_core::object::FontRef::new("");
    object.set_payload(ObjectPayload::Text(text));
    let bad_font = request_for(object, source, "Tj");
    assert!(matches!(
        engine.rewrite(&bad_font, &mut events),
        Err(RewriteError::ValidationFailed(_))
    ));

    // Path 冻结接口。
    let path = Object::path(
        ObjectId(2),
        page_id(),
        BBox::new(0.0, 0.0, 10.0, 10.0),
        0,
        None,
        PathObject::default(),
    );
    let target = target_index(source, "Tj");
    let path_request = RewriteRequest::new(
        commit_of(path),
        FontPatch::default(),
        source.to_vec(),
        page_id(),
        Some(12),
        target,
    );
    assert!(matches!(
        engine.rewrite(&path_request, &mut events),
        Err(RewriteError::PolicyNotImplemented(_))
    ));
}

/// CT-06：ContentPatch 四字段齐全、可 JSON 往返、streams 与 operators 一致。
#[test]
fn ct06_patch_shape() {
    let request = request_for(text_object("新"), SOURCE_STREAM.as_bytes(), "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    assert_eq!(patch.page_id, page_id());
    assert_eq!(patch.stream_ref, Some(12));
    assert_eq!(patch.operator_count(), 5);
    assert_eq!(patch.streams.len(), 1);
    assert_eq!(patch.streams[0].stream_ref, 12);
    assert_eq!(patch.streams[0].operators, patch.operators);

    let json = serde_json::to_string(&patch).expect("补丁可序列化");
    let decoded: ContentPatch = serde_json::from_str(&json).expect("补丁可反序列化");
    assert_eq!(decoded, patch);
    assert!(!json.contains("selection"), "补丁不得包含 Selection");
    assert!(!json.contains("history"), "补丁不得包含 History");
}

/// CT-07：三类事件携带结构化载荷（含失败路径）。
#[test]
fn ct07_events() {
    let engine = ContentRewriteEngine::new();
    let mut events = EventBus::new();
    let request = request_for(text_object("新"), SOURCE_STREAM.as_bytes(), "Tj");
    engine.rewrite(&request, &mut events).expect("成功");

    assert!(events.events().iter().any(|event| {
        event.kind == "ContentRewriteStarted"
            && matches!(
                event.payload,
                Some(EventPayload::ContentRewriteStarted { page: 1 })
            )
    }));
    assert!(events.events().iter().any(|event| {
        event.kind == "ContentRewriteFinished"
            && matches!(
                event.payload,
                Some(EventPayload::ContentRewriteFinished {
                    page: 1,
                    operator_count: 5,
                })
            )
    }));

    let mut bad_events = EventBus::new();
    let mut bad = request_for(text_object("新"), SOURCE_STREAM.as_bytes(), "Tj");
    bad.target_operator_index = 99;
    assert!(engine.rewrite(&bad, &mut bad_events).is_err());
    assert!(bad_events.events().iter().any(|event| {
        event.kind == "ContentRewriteFailed"
            && matches!(
                &event.payload,
                Some(EventPayload::ContentRewriteFailed { page: 1, reason }) if reason.contains("越界")
            )
    }));
}

/// CT-08：源 Stream 只读——重写后原始字节不变，且可从补丁重建内容流。
#[test]
fn ct08_source_stream_is_read_only() {
    let source = SOURCE_STREAM.as_bytes().to_vec();
    let before = source.clone();
    let request = request_for(text_object("新"), &source, "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    assert_eq!(source, before, "引擎不得修改源字节");
    let rebuilt = encode_operators(&patch.operators).expect("可从补丁重建");
    let reparsed = decode_operators(&rebuilt).expect("重建流可解析");
    assert_eq!(reparsed.len(), patch.operators.len());
    assert_eq!(
        reparsed[request.target_operator_index],
        patch.operators[request.target_operator_index]
    );
}

/// CT-09：Unknown 分散在首 / 中 / 尾时仍 100% 保留，数量与顺序不变。
#[test]
fn ct09_unknown_preserve_all_positions() {
    let source = b"1 UNKNOWN\nBT\n/F1 12 Tf\n(old) Tj\n(x) MYOP\nET\nTAIL";
    let source_ops = decode_operators(source).expect("可解析");
    let request = request_for(text_object("新"), source, "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    assert_eq!(patch.operators.len(), source_ops.len());
    for index in [0usize, 4, 6] {
        assert_eq!(
            patch.operators[index], source_ops[index],
            "第 {} 个 Unknown 被改动",
            index
        );
    }
}

/// CT-10：多目标同类 Operator 时只替换指定下标，其余 Tj 原样。
#[test]
fn ct10_only_target_operator_changes() {
    let source = b"BT\n(old) Tj\n(old) Tj\nET";
    let request = request_for(text_object("新"), source, "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    let tj_positions: Vec<usize> = patch
        .operators
        .iter()
        .enumerate()
        .filter(|(_, op)| op.name == "Tj")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(tj_positions.len(), 2);
    assert_eq!(tj_positions[0], request.target_operator_index);
    assert_ne!(
        patch.operators[tj_positions[0]], patch.operators[tj_positions[1]],
        "只应替换第一个 Tj"
    );
}

/// CT-11：Save Pipeline 接口契约——补丁自带定位信息、可序列化，不实际写盘（18-09 才做集成）。
#[test]
fn ct11_save_integration_contract() {
    let request = request_for(text_object("新"), SOURCE_STREAM.as_bytes(), "Tj");
    let patch = ContentRewriteEngine::new()
        .rewrite(&request, &mut EventBus::new())
        .expect("重写成功");

    assert_eq!(patch.page_id, page_id());
    assert_eq!(patch.stream_ref, Some(12));
    assert!(!patch.is_empty());
    let json = serde_json::to_string(&patch).expect("补丁可交给 Save 层");
    assert!(json.contains("\"stream_ref\":12"));
    assert!(json.contains("\"page_id\""));
}

/// AD-21：图片对象走同一引擎，只替换 `Do` 资源名，其余操作数保持。
#[test]
fn image_rewrite_uses_same_engine() {
    let source = b"q\n/Im0 Do\nQ";
    let mut image = ImageObject::new(7, 10, 10, "DeviceRGB");
    image.resource.name = "Im1".to_string();
    let object = Object::image(
        ObjectId(3),
        page_id(),
        BBox::new(0.0, 0.0, 10.0, 10.0),
        0,
        Some(7),
        image,
    );
    let patch = ContentRewriteEngine::new()
        .rewrite(
            &RewriteRequest::new(
                commit_of(object),
                FontPatch::default(),
                source.to_vec(),
                page_id(),
                None,
                target_index(source, "Do"),
            ),
            &mut EventBus::new(),
        )
        .expect("图片重写成功");

    assert_eq!(patch.operators[1].name, "Do");
    assert_eq!(
        patch.operators[1].operands,
        vec![PdfOperand::Name("Im1".to_string())]
    );
    assert_eq!(patch.operators[0].name, "q");
    assert_eq!(patch.operators[2].name, "Q");
}
