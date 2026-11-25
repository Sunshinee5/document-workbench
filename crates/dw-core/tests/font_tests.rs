//! 18-07 Font Rewrite Engine 集成测试（`13` §十二：FT-01 ～ FT-08）。
//!
//! 只用公开 API 驱动，验证 AD-17 ～ AD-20 的可观察行为：
//! 注册表真值、Unicode 规范化、字形解析、子集规划、FontPatch、错误与事件。

use dw_core::diff::{Diff, ObjectDiffPayload};
use dw_core::editing::EditingCommit;
use dw_core::eventbus::{EventBus, EventPayload};
use dw_core::font::{
    FontDescriptor, FontEncoding, FontPatch, FontRegistry, FontRewriteEngine, FontRewriteError,
    SubsetStrategy,
};
use dw_core::object::{BBox, FontRef, Object, ObjectId, TextObject};
use dw_core::page::PageId;

fn font_ref() -> FontRef {
    FontRef::new("F1").with_xref(12)
}

/// 构造一个文本对象：unicode 为 `"你好"`，glyphs 仍是 Parser 产出的原始编码。
fn text_object(unicode: &str) -> Object {
    let mut text = TextObject::new("F1", vec![20320, 22909], Some(unicode.to_string()));
    text.font = font_ref();
    Object::text(
        ObjectId(1),
        PageId::new(1),
        BBox::new(0.0, 0.0, 10.0, 10.0),
        0,
        None,
        text,
    )
}

/// 由编辑后的对象构造 `EditingCommit`（18-06 的输出契约）。
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

/// 标准字体：Identity 编码、未嵌入、含「你 / 好 / A / B / !」字形。
fn registry_with(encoding: FontEncoding, embedded: bool, subset: bool) -> FontRegistry {
    let mut registry = FontRegistry::new();
    let descriptor = FontDescriptor::new(font_ref(), encoding)
        .with_family("宋体", "Regular")
        .with_flags(embedded, subset)
        .with_glyph("你", 20320)
        .with_glyph("好", 22909)
        .with_glyph("A", 33)
        .with_glyph("B", 34)
        .with_glyph("!", 33);
    registry.insert(descriptor);
    registry
}

fn standard_registry() -> FontRegistry {
    registry_with(FontEncoding::Identity, false, false)
}

/// FT-01：Unicode 规范化——全角 ASCII / 全角空格折半角，trim 生效。
#[test]
fn ft01_unicode_normalize() {
    let commit = commit_of(text_object("  ＡＢ！ "));
    let registry = standard_registry();
    let mut events = EventBus::new();
    let engine = FontRewriteEngine::new();

    let patch = engine
        .rewrite(&commit, &registry, &mut events)
        .expect("规范化后应全部命中字形");
    assert_eq!(patch.glyph_mapping.len(), 3);
    assert_eq!(patch.glyph_mapping.glyph_of("A"), Some(33));
    assert_eq!(patch.glyph_mapping.glyph_of("B"), Some(34));
    assert_eq!(patch.glyph_mapping.glyph_of("!"), Some(33));
    assert_eq!(patch.subset_plan.used_glyphs.len(), 2, "A 与 ! 共用字形 33");

    // 全角空格折叠为半角空格。
    let space_commit = commit_of(text_object("　"));
    assert!(
        engine
            .rewrite(&space_commit, &registry, &mut events)
            .is_err(),
        "空格不在 cmap 中，必须报 MissingGlyph 而非静默替换"
    );
}

/// FT-02：Glyph Resolve 按 cmap 得到确定映射。
#[test]
fn ft02_glyph_resolve() {
    let commit = commit_of(text_object("你好"));
    let patch = FontRewriteEngine::new()
        .rewrite(&commit, &standard_registry(), &mut EventBus::new())
        .expect("解析成功");

    assert_eq!(patch.glyph_mapping.font, font_ref());
    assert_eq!(patch.glyph_mapping.glyph_of("你"), Some(20320));
    assert_eq!(patch.glyph_mapping.glyph_of("好"), Some(22909));
    assert_eq!(
        patch.glyph_mapping.glyph_of("你"),
        Some(20320),
        "重复查询必须一致"
    );
}

/// FT-03：Subset Planning 三种策略与 used glyph 合并。
#[test]
fn ft03_subset_planning() {
    let commit = commit_of(text_object("你好"));
    let engine = FontRewriteEngine::new();

    let patch = engine
        .rewrite(&commit, &standard_registry(), &mut EventBus::new())
        .expect("未嵌入字体");
    assert_eq!(patch.subset_plan.strategy, SubsetStrategy::NewSubset);
    assert_eq!(patch.subset_plan.font, font_ref());
    assert_eq!(
        patch.subset_plan.used_glyphs,
        [20320, 22909].into_iter().collect()
    );
    assert_eq!(
        patch.added_fonts,
        vec![font_ref()],
        "未嵌入字体需 18-08 新建"
    );
    assert!(patch.removed_fonts.is_empty());

    let embedded = engine
        .rewrite(
            &commit,
            &registry_with(FontEncoding::Identity, true, false),
            &mut EventBus::new(),
        )
        .expect("已嵌入字体");
    assert_eq!(embedded.subset_plan.strategy, SubsetStrategy::ReuseEmbedded);
    assert!(embedded.added_fonts.is_empty());

    let subset = engine
        .rewrite(
            &commit,
            &registry_with(FontEncoding::Identity, true, true),
            &mut EventBus::new(),
        )
        .expect("已是子集字体");
    assert_eq!(subset.subset_plan.strategy, SubsetStrategy::ExtendSubset);
    assert!(subset.added_fonts.is_empty());
}

/// FT-04：MissingGlyph 严格报错，不做静默替换。
#[test]
fn ft04_missing_glyph_never_substitutes() {
    let commit = commit_of(text_object("你x"));
    let mut events = EventBus::new();
    let error = FontRewriteEngine::new()
        .rewrite(&commit, &standard_registry(), &mut events)
        .expect_err("x 不在 cmap 中必须报错");

    assert_eq!(
        error,
        FontRewriteError::MissingGlyph {
            font: font_ref(),
            character: "x".to_string(),
        }
    );
    assert!(error.to_string().contains("不做静默替换"), "{}", error);
}

/// FT-05：FontRegistry 是 Document 内的唯一字体真值；Object 只持有 FontRef。
#[test]
fn ft05_registry_is_document_truth() {
    let mut document = dw_core::document::Document::new(
        dw_core::document::DocumentId(1),
        dw_core::document::DocumentKind::Pdf,
        dw_core::capability::CapabilitySet::new(),
    );
    assert!(document.font_registry.is_empty());
    document
        .font_registry
        .insert(FontDescriptor::new(font_ref(), FontEncoding::Identity));
    assert!(document.font_registry.contains(&font_ref()));
    assert_eq!(document.font_registry.len(), 1);
    assert_eq!(document.font_registry.font_refs(), vec![font_ref()]);

    let object = text_object("你好");
    let text = object.as_text().expect("文本对象");
    assert_eq!(text.font, font_ref(), "Object 只保存引用，不保存字体资源");
    assert_eq!(text.font_id, "F1");
    assert!(serde_json::to_string(&text.font)
        .expect("引用可序列化")
        .contains("F1"));
}

/// FT-06：FontPatch 四字段完整，且是可 JSON 往返的值对象。
#[test]
fn ft06_patch_is_complete_value_object() {
    let commit = commit_of(text_object("你好"));
    let patch = FontRewriteEngine::new()
        .rewrite(&commit, &standard_registry(), &mut EventBus::new())
        .expect("生成补丁");

    assert!(!patch.is_empty());
    assert_eq!(patch.added_fonts, vec![font_ref()]);
    assert!(patch.removed_fonts.is_empty());
    assert_eq!(patch.subset_plan.used_glyphs.len(), 2);
    assert_eq!(patch.glyph_mapping.len(), 2);

    let json = serde_json::to_string(&patch).expect("补丁可序列化");
    let decoded: FontPatch = serde_json::from_str(&json).expect("补丁可反序列化");
    assert_eq!(decoded, patch);
}

/// FT-07：Validation 四类失败与编码不支持，全部中文 Display。
#[test]
fn ft07_validation_failures() {
    let engine = FontRewriteEngine::new();
    let mut events = EventBus::new();

    // Font 不存在。
    let error = engine
        .rewrite(
            &commit_of(text_object("你好")),
            &FontRegistry::new(),
            &mut events,
        )
        .expect_err("未注册字体必须报错");
    assert_eq!(error, FontRewriteError::FontNotFound(font_ref()));
    assert!(error.to_string().contains("字体不存在"));

    // 编码不支持。
    let error = engine
        .rewrite(
            &commit_of(text_object("你好")),
            &registry_with(FontEncoding::Other("GBK-legacy".to_string()), false, false),
            &mut events,
        )
        .expect_err("未识别编码必须报错");
    assert!(matches!(error, FontRewriteError::EncodingUnsupported(_)));
    assert!(error.to_string().contains("不支持的字体编码"));

    // Unicode 为空。
    let error = engine
        .rewrite(
            &commit_of(text_object("   ")),
            &standard_registry(),
            &mut events,
        )
        .expect_err("空 Unicode 必须报错");
    assert!(matches!(error, FontRewriteError::ValidationFailed(_)));

    // 控制字符。
    let error = engine
        .rewrite(
            &commit_of(text_object("你\u{0001}")),
            &standard_registry(),
            &mut events,
        )
        .expect_err("非法控制字符必须报错");
    assert!(matches!(error, FontRewriteError::ValidationFailed(_)));
    assert!(error.to_string().contains("字体重写校验失败"));

    // FontRef 名为空。
    let mut object = text_object("你好");
    let mut text = object.as_text().expect("文本对象").clone();
    text.font = FontRef::new("");
    object.set_payload(dw_core::object::ObjectPayload::Text(text));
    let error = engine
        .rewrite(&commit_of(object), &standard_registry(), &mut events)
        .expect_err("空 FontRef 必须报错");
    assert!(matches!(error, FontRewriteError::ValidationFailed(_)));
}

/// FT-08：Started / Finished / Failed 三类事件携带结构化载荷。
#[test]
fn ft08_events_carry_payload() {
    let engine = FontRewriteEngine::new();
    let mut events = EventBus::new();
    let commit = commit_of(text_object("你好"));

    engine
        .rewrite(&commit, &standard_registry(), &mut events)
        .expect("成功路径");
    assert!(events.events().iter().any(|event| {
        event.kind == "FontRewriteStarted"
            && matches!(
                event.payload,
                Some(EventPayload::FontRewriteStarted { object_id: 1 })
            )
    }));
    assert!(events.events().iter().any(|event| {
        event.kind == "FontRewriteFinished"
            && matches!(
                event.payload,
                Some(EventPayload::FontRewriteFinished { glyph_count: 2, .. })
            )
    }));

    let mut failed_events = EventBus::new();
    let error = engine
        .rewrite(
            &commit_of(text_object("你x")),
            &standard_registry(),
            &mut failed_events,
        )
        .expect_err("失败路径");
    assert!(failed_events.events().iter().any(|event| {
        event.kind == "FontRewriteFailed"
            && matches!(&event.payload, Some(EventPayload::FontRewriteFailed { reason, .. }) if reason.contains("缺少字符"))
    }));
    assert!(event_has_failed_payload(&failed_events));
    assert!(error.to_string().contains("F1"));
}

/// 辅助：确认失败事件在失败后仍可读取（事件不因错误返回值丢失）。
fn event_has_failed_payload(events: &EventBus) -> bool {
    events
        .events()
        .iter()
        .any(|event| matches!(event.payload, Some(EventPayload::FontRewriteFailed { .. })))
}

/// AD-19：引擎只读 `EditingCommit` 与 `&FontRegistry`，不接触 Selection / HitTest。
#[test]
fn engine_has_no_selection_input() {
    // 编译期即可证明：`rewrite` 的公开签名只有 commit / registry / events 三个参数。
    let engine = FontRewriteEngine::new();
    let patch = engine
        .rewrite(
            &commit_of(text_object("你好")),
            &standard_registry(),
            &mut EventBus::new(),
        )
        .expect("规划成功");
    assert_eq!(patch.glyph_mapping.len(), 2);
}

/// AD-18：Object 的 glyphs 字段在重写后保持 Parser 原值，不被新计算的字形覆盖。
#[test]
fn object_glyphs_are_not_rewritten() {
    let object = text_object("你好");
    let before = object.clone();
    let commit = commit_of(object);
    let engine = FontRewriteEngine::new();
    let patch = engine
        .rewrite(&commit, &standard_registry(), &mut EventBus::new())
        .expect("规划成功");

    assert_eq!(
        commit.object.as_text().expect("文本对象").glyphs,
        before.as_text().expect("文本对象").glyphs,
        "Object 中的 Parser 原始 glyphs 不变"
    );
    assert_eq!(patch.glyph_mapping.glyph_of("你"), Some(20320));
    assert_ne!(
        patch.glyph_mapping.glyph_of("你"),
        Some(999),
        "新解析的字形只存在于 Patch"
    );
}
