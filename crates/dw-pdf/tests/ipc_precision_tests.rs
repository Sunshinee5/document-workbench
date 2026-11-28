//! 回归测试：跨 Tauri IPC 的 u64 对象标识必须以十进制字符串传递。
//!
//! 根因回顾：`ObjectId` 由 `(xref, generation, stream_index, operator_index)`
//! 确定性派生，可达 19 位 u64。JS `number` 的安全整数上限是 2^53 - 1，
//! 若按 JSON 数字传输，回传 Rust 时已不是原对象，双击编辑会报
//! 「对象 #… 不存在或已删除」。本测试锁死序列化边界的契约。

use dw_core::serde_util;
use dw_pdf::editing_service::{
    CapabilityInfo, CommitInfo, EditingInfo, EditorSnapshot, HitInfo, ImageInspector,
    InspectorInfo, SelectionInfo,
};
use dw_pdf::object_tree_view::{ObjectNodeInfo, ObjectTreeInfo};
use serde_json::{json, Value};

/// 两个实测会丢精度的 19 位对象标识（均大于 2^53）。
const BIG_ID: u64 = 1_617_674_655_699_130_457;
const OTHER_ID: u64 = 31_774_738_392_861_546;

/// 断言某字段是字符串且内容等于期望 u64 的十进制表示。
fn assert_string_field(value: &Value, path: &[&str], expected: u64) {
    let mut cursor = value;
    for key in path {
        cursor = if cursor.is_array() {
            let index = key
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("JSON 数组下标非法：{}", key));
            cursor
                .get(index)
                .unwrap_or_else(|| panic!("JSON 数组缺少下标 {}", key))
        } else {
            cursor
                .get(key)
                .unwrap_or_else(|| panic!("JSON 缺少字段 {}", key))
        };
    }
    assert!(
        cursor.is_string(),
        "字段 {:?} 必须是字符串，实际为 {:?}",
        path,
        cursor
    );
    assert_eq!(
        cursor.as_str().expect("已确认是字符串"),
        expected.to_string(),
        "字段 {:?} 的十进制内容不一致",
        path
    );
}

/// 用属性自身的反序列化函数把 JSON 字符串还原成 u64，证明往返无损。
fn parse_back(text: &str) -> u64 {
    let quoted = format!("\"{}\"", text);
    let mut deserializer = serde_json::Deserializer::from_str(&quoted);
    serde_util::u64_as_string::deserialize(&mut deserializer).expect("字符串应能解析回 u64")
}

fn sample_snapshot() -> EditorSnapshot {
    EditorSnapshot {
        document_id: 7,
        page: 1,
        zoom: 1.0,
        selection: SelectionInfo {
            page: Some(1),
            primary: Some(BIG_ID),
            selected: vec![BIG_ID, OTHER_ID],
            version: OTHER_ID,
        },
        editing: EditingInfo {
            active: true,
            object_id: Some(BIG_ID),
        },
        capability: CapabilityInfo::default(),
        can_undo: false,
        can_redo: false,
        dirty: false,
        pending_patches: 0,
        revision: BIG_ID,
    }
}

#[test]
fn hit_info_object_id_is_json_string_and_round_trips() {
    let hit = HitInfo {
        page: 1,
        object_id: BIG_ID,
        kind: "Text".to_string(),
        kind_label: "文本".to_string(),
        bbox: [0.0, 0.0, 10.0, 10.0],
    };
    let json = serde_json::to_string(&hit).expect("HitInfo 应能序列化");
    let value: Value = serde_json::from_str(&json).expect("JSON 应可解析");
    assert_string_field(&value, &["object_id"], BIG_ID);
    assert!(
        !json.contains(&format!("\"object_id\":{}", BIG_ID)),
        "object_id 不得作为 JSON 数字出现：{}",
        json
    );
    // JS 把该值当 number 时会变成 1617674655699130400；字符串往返保持原值。
    let text = value["object_id"].as_str().expect("已是字符串");
    assert_eq!(parse_back(text), BIG_ID);
    assert_ne!(text.parse::<f64>().expect("模拟 JS number") as u64, BIG_ID);
}

#[test]
fn editor_snapshot_all_u64_fields_are_strings() {
    let snapshot = sample_snapshot();
    let value = serde_json::to_value(&snapshot).expect("EditorSnapshot 应能序列化");
    assert_string_field(&value, &["selection", "primary"], BIG_ID);
    assert_string_field(&value, &["selection", "version"], OTHER_ID);
    assert_string_field(&value, &["editing", "object_id"], BIG_ID);
    assert_string_field(&value, &["revision"], BIG_ID);
    assert_eq!(value["selection"]["selected"].as_array().expect("字符串数组").len(), 2);
    assert_eq!(
        value["selection"]["selected"][0].as_str(),
        Some("1617674655699130457")
    );
    assert_eq!(
        value["selection"]["selected"][1].as_str(),
        Some("31774738392861546")
    );

    let quoted = value["selection"]["primary"].to_string();
    let mut deserializer = serde_json::Deserializer::from_str(&quoted);
    let restored = serde_util::option_u64_as_string::deserialize(&mut deserializer)
        .expect("字符串应能解析回 Option<u64>");
    assert_eq!(restored, Some(BIG_ID));
}

#[test]
fn commit_inspector_and_tree_ids_are_strings() {
    let commit = CommitInfo {
        object_id: BIG_ID,
        page: 1,
        before_version: 1,
        after_version: OTHER_ID,
        operators: 3,
        snapshot: sample_snapshot(),
    };
    let value = serde_json::to_value(&commit).expect("CommitInfo 应能序列化");
    assert_string_field(&value, &["object_id"], BIG_ID);
    assert_string_field(&value, &["before_version"], 1);
    assert_string_field(&value, &["after_version"], OTHER_ID);
    assert_string_field(&value, &["snapshot", "editing", "object_id"], BIG_ID);

    let inspector = InspectorInfo {
        object_id: BIG_ID,
        page: 1,
        kind: "Image".to_string(),
        kind_label: "图片".to_string(),
        bbox: [0.0, 0.0, 1.0, 1.0],
        z_index: 0,
        lifecycle: "存活".to_string(),
        version: OTHER_ID,
        text: None,
        image: Some(ImageInspector {
            pixel_width: 10,
            pixel_height: 20,
            colorspace: "DeviceRGB".to_string(),
            xref: BIG_ID,
            resolution_dpi: None,
            compression: None,
        }),
        path: None,
    };
    let value = serde_json::to_value(&inspector).expect("InspectorInfo 应能序列化");
    assert_string_field(&value, &["object_id"], BIG_ID);
    assert_string_field(&value, &["version"], OTHER_ID);
    assert_string_field(&value, &["image", "xref"], BIG_ID);

    let node = ObjectNodeInfo {
        object_id: BIG_ID,
        page: 1,
        kind: "Text".to_string(),
        kind_label: "文本".to_string(),
        bbox: [0.0, 0.0, 1.0, 1.0],
        z_index: 0,
        stream_ref: Some(OTHER_ID),
        lifecycle: "存活".to_string(),
        version: OTHER_ID,
        font_id: None,
        font_size: None,
        unicode: None,
        glyph_count: 0,
    };
    let tree = ObjectTreeInfo {
        document_id: 7,
        page_count: 1,
        object_count: 1,
        pages: vec![dw_pdf::object_tree_view::ObjectPageInfo {
            page: 1,
            page_width: 595.0,
            page_height: 842.0,
            object_count: 1,
            objects: vec![node],
        }],
        warnings: Vec::new(),
    };
    let value = serde_json::to_value(&tree).expect("ObjectTreeInfo 应能序列化");
    assert_string_field(&value, &["pages", "0", "objects", "0", "object_id"], BIG_ID);
    assert_string_field(&value, &["pages", "0", "objects", "0", "stream_ref"], OTHER_ID);
    assert_string_field(&value, &["pages", "0", "objects", "0", "version"], OTHER_ID);
    // document_id 由 session 分配，是小整数，保持 JSON 数字以匹配 IPC 入参。
    assert_eq!(json!(value["document_id"]), json!(7));
    assert!(value["document_id"].is_number());
}
