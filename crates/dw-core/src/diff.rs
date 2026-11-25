//! Diff 数据模型：恢复 `Command → Diff → Checkpoint` 链路（11.1 第三节）。
//! Diff 分两档：结构 Diff（可 JSON 表达）与二进制 Diff（内容流等大对象）。
//! 冻结约束：Diff 不是层、不是 Manager，只是 History 回放所依据的数据。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::document::{Document, PageLayout};
use crate::error::DwError;
use crate::object::{Object, ObjectId};
use crate::page::Page;

/// Diff 引用：指向 `Recovery/session-XXX/diffs/` 下的具体文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRef(pub String);

/// Checkpoint 引用：指向 `Recovery/session-XXX/checkpoints/` 下的快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRef(pub String);

/// 结构 Diff：页序、旋转、删页、注释、元数据等可 JSON 表达的变更。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureDiff {
    /// 变更类型，例如 RotatePage / DeletePage / Merge。
    pub operation: String,
    /// 变更载荷，例如 `{ "page": 3, "oldRotation": 0, "newRotation": 90 }`。
    pub payload: serde_json::Value,
}

/// 二进制 Diff：内容流、图片替换等大对象变更，以独立文件落盘。
#[derive(Debug, Clone)]
pub struct BinaryDiff {
    /// 二进制块所在路径。
    pub path: PathBuf,
    /// 二进制块字节数。
    pub size: u64,
}

/// 旋转载荷：Feature-02 三种结构 Diff 之一。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationDiff {
    /// 被旋转的页位置（1 起）。
    pub pages: Vec<u32>,
    /// 顺时针角度（0 / 90 / 180 / 270）。
    pub degrees: i32,
}

/// 删页载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletePageDiff {
    /// 被删除的页位置（1 起，删除前的位置）。
    pub pages: Vec<u32>,
    /// 被删页的原始页码，顺序与升序后的 `pages` 一一对应。
    /// Undo 要把它们插回原位；只有位置信息是还原不了的。
    pub removed: Vec<u32>,
    /// 被删页当时的旋转角，`removed` 中每项一条。
    pub rotations: Vec<(u32, i32)>,
}

/// 移页载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovePageDiff {
    /// 原位置（1 起）。
    pub from: u32,
    /// 目标位置（1 起）。
    pub to: u32,
}

/// 对象级 Diff 载荷（`07` 裁决 10/11/12）。
///
/// 装在 `StructureDiff.payload` 里，**不新增 Diff 顶层变体**，不破坏「结构 / 二进制」两档结构。
/// 变化判据是 Object 的 version（before/after），不是 Document 的 Dirty。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectDiffPayload {
    /// 被修改的对象。
    pub object_id: ObjectId,
    /// 对象级操作名（如 TextReplace / ImageReplace / ObjectDelete）。
    pub operation: String,
    /// 修改前版本号。
    pub before_version: u64,
    /// 修改后版本号。
    pub after_version: u64,
    /// 修改前快照（JSON）。
    pub before: serde_json::Value,
    /// 修改后快照（JSON）。
    pub after: serde_json::Value,
}

impl Diff {
    /// 旋转 Diff：`operation` 固定为 `RotatePages`。
    pub fn rotation(payload: &RotationDiff) -> Self {
        Self::structure("RotatePages", payload)
    }

    /// 删页 Diff：`operation` 固定为 `DeletePages`。
    pub fn delete_page(payload: &DeletePageDiff) -> Self {
        Self::structure("DeletePages", payload)
    }

    /// 移页 Diff：`operation` 固定为 `MovePages`。
    pub fn move_page(payload: &MovePageDiff) -> Self {
        Self::structure("MovePages", payload)
    }

    /// 对象级 Diff：`operation` 固定为 `ObjectChange`，载荷含 ObjectId 与版本区间。
    ///
    /// 回放实现归 18-02 起的章节；本章只冻结数据与规则（裁决 10/11/12）。
    pub fn object_change(payload: &ObjectDiffPayload) -> Self {
        Self::structure("ObjectChange", payload)
    }

    /// 统一的结构 Diff 构造口；序列化失败时退化为 Null，绝不 panic 在回放链上。
    fn structure(operation: &str, payload: &impl Serialize) -> Self {
        Self::Structure(StructureDiff {
            operation: operation.to_string(),
            payload: serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
        })
    }

    /// 回放：把本 Diff 应用到 Document。Redo 走这里。
    ///
    /// 冻结约束（`00` §5）：Undo / Redo 只应用 Diff，**不重跑 Driver、不重新生成 Task**。
    pub fn apply(&self, document: &Document) -> Result<(), DwError> {
        match self {
            Diff::Structure(structure) => apply_structure(document, structure),
            Diff::Binary(_) => Err(DwError::Execution(
                "二进制 Diff 的回放尚未实现（本阶段无此类变更）".to_string(),
            )),
        }
    }

    /// 反向回放：Undo 走这里。
    pub fn revert(&self, document: &Document) -> Result<(), DwError> {
        match self {
            Diff::Structure(structure) => revert_structure(document, structure),
            Diff::Binary(_) => Err(DwError::Execution(
                "二进制 Diff 的反向回放尚未实现（本阶段无此类变更）".to_string(),
            )),
        }
    }

    /// 对象级 Diff 的正向回放（`12` §十九 C-4）：Redo 走这里。
    ///
    /// 对象挂在 `Page` 上，`Document` 不含对象树，因此对象级 Diff **只对 Page 回放**。
    pub fn apply_object(&self, page: &mut Page) -> Result<(), DwError> {
        self.replay_object(page, true)
    }

    /// 对象级 Diff 的反向回放（`12` §十九 C-4 / C-5）：Undo 走这里。
    ///
    /// 恢复 `before` 整份快照（含 `version`）——回放必须与 Diff 完全对称，
    /// 单调递增只约束编辑动作，不约束回放。
    pub fn revert_object(&self, page: &mut Page) -> Result<(), DwError> {
        self.replay_object(page, false)
    }

    /// 对象级回放实现：`forward = true` 取 `after`，否则取 `before`。
    fn replay_object(&self, page: &mut Page, forward: bool) -> Result<(), DwError> {
        let Diff::Structure(structure) = self else {
            return Err(DwError::Execution(
                "二进制 Diff 不支持对象级回放".to_string(),
            ));
        };
        if structure.operation != "ObjectChange" {
            return Err(DwError::Execution(format!(
                "非对象级 Diff 不能对 Page 回放：{}",
                structure.operation
            )));
        }
        let payload: ObjectDiffPayload = decode(&structure.payload)?;
        let snapshot = if forward {
            &payload.after
        } else {
            &payload.before
        };
        let object: Object = serde_json::from_value(snapshot.clone()).map_err(|error| {
            DwError::Execution(format!(
                "对象快照损坏（对象 {}）：{}",
                payload.object_id.raw(),
                error
            ))
        })?;
        page.insert_object(object);
        Ok(())
    }
}

/// 角度归一到 0 / 90 / 180 / 270。
fn normalize(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

/// 解码结构 Diff 载荷；解码失败返回中文错误，不 panic。
fn decode<T: serde::de::DeserializeOwned>(payload: &serde_json::Value) -> Result<T, DwError> {
    serde_json::from_value(payload.clone())
        .map_err(|error| DwError::Execution(format!("Diff 载荷损坏：{}", error)))
}

/// 应用结构 Diff：按 operation 分派到三种页编辑回放。
fn apply_structure(document: &Document, diff: &StructureDiff) -> Result<(), DwError> {
    let mut layout = document.page_layout();
    match diff.operation.as_str() {
        "RotatePages" => {
            let payload: RotationDiff = decode(&diff.payload)?;
            rotate(&mut layout, &payload, payload.degrees)?;
        }
        "DeletePages" => {
            let payload: DeletePageDiff = decode(&diff.payload)?;
            delete(&mut layout, &payload.pages)?;
        }
        "MovePages" => {
            let payload: MovePageDiff = decode(&diff.payload)?;
            move_page(&mut layout, payload.from, payload.to)?;
        }
        "ObjectChange" => {
            // Document 不含对象树（Object 挂在 Page 上），对象级 Diff 必须对 Page 回放。
            let payload: ObjectDiffPayload = decode(&diff.payload)?;
            return Err(DwError::Execution(format!(
                "对象级 Diff 不能对 Document 回放，请用 Diff::apply_object(&mut Page)（对象 {}，版本 {} → {}）",
                payload.object_id.raw(),
                payload.before_version,
                payload.after_version
            )));
        }
        other => return Err(DwError::Execution(format!("未知 Diff 操作：{}", other))),
    }
    document.set_page_layout(layout);
    Ok(())
}

/// 反向应用结构 Diff。
fn revert_structure(document: &Document, diff: &StructureDiff) -> Result<(), DwError> {
    let mut layout = document.page_layout();
    match diff.operation.as_str() {
        "RotatePages" => {
            let payload: RotationDiff = decode(&diff.payload)?;
            rotate(&mut layout, &payload, -payload.degrees)?;
        }
        "DeletePages" => {
            let payload: DeletePageDiff = decode(&diff.payload)?;
            restore_pages(&mut layout, &payload)?;
        }
        "MovePages" => {
            let payload: MovePageDiff = decode(&diff.payload)?;
            move_page(&mut layout, payload.to, payload.from)?;
        }
        "ObjectChange" => {
            let payload: ObjectDiffPayload = decode(&diff.payload)?;
            return Err(DwError::Execution(format!(
                "对象级 Diff 不能对 Document 反向回放，请用 Diff::revert_object(&mut Page)（对象 {}，版本 {} → {}）",
                payload.object_id.raw(),
                payload.before_version,
                payload.after_version
            )));
        }
        other => return Err(DwError::Execution(format!("未知 Diff 操作：{}", other))),
    }
    document.set_page_layout(layout);
    Ok(())
}

/// 旋转页：位置是当前显示顺序的 1 起下标。
fn rotate(layout: &mut PageLayout, payload: &RotationDiff, delta: i32) -> Result<(), DwError> {
    for position in &payload.pages {
        let original = *layout
            .order
            .get(*position as usize - 1)
            .ok_or_else(|| DwError::Execution(format!("回放页码 {} 越界", position)))?;
        let next = normalize(layout.rotation_of(original) + delta);
        if next == 0 {
            layout.rotations.remove(&original);
        } else {
            layout.rotations.insert(original, next);
        }
    }
    Ok(())
}

/// 删除页：与 Driver 的删除语义保持一致。
fn delete(layout: &mut PageLayout, pages: &[u32]) -> Result<(), DwError> {
    let mut positions: Vec<usize> = pages.iter().map(|page| *page as usize - 1).collect();
    positions.sort_unstable();
    positions.dedup();
    if positions.len() >= layout.len() {
        return Err(DwError::Execution(
            "回放删页失败：至少要保留一页".to_string(),
        ));
    }
    let removed: Vec<u32> = positions
        .iter()
        .filter_map(|index| layout.order.get(*index).copied())
        .collect();
    layout.order = layout
        .order
        .iter()
        .enumerate()
        .filter(|(index, _)| !positions.contains(index))
        .map(|(_, page)| *page)
        .collect();
    for page in removed {
        layout.rotations.remove(&page);
    }
    Ok(())
}

/// 还原被删页：按记录的原始页码插回原位置，并恢复旋转角。
fn restore_pages(layout: &mut PageLayout, payload: &DeletePageDiff) -> Result<(), DwError> {
    if payload.pages.len() != payload.removed.len() {
        return Err(DwError::Execution("删页 Diff 载荷不一致".to_string()));
    }
    let mut pairs: Vec<(u32, u32)> = payload
        .pages
        .iter()
        .copied()
        .zip(payload.removed.iter().copied())
        .collect();
    pairs.sort_unstable();
    for (position, original) in pairs {
        let index = (position - 1) as usize;
        if index > layout.order.len() {
            return Err(DwError::Execution(format!("还原页码 {} 越界", position)));
        }
        layout.order.insert(index, original);
    }
    let mut rotations: BTreeMap<u32, i32> = payload.rotations.iter().copied().collect();
    layout.rotations.append(&mut rotations);
    Ok(())
}

/// 移动页：与 Driver 的 move 语义一致。
fn move_page(layout: &mut PageLayout, from: u32, to: u32) -> Result<(), DwError> {
    if from == 0 || to == 0 || from as usize > layout.len() || to as usize > layout.len() {
        return Err(DwError::Execution(format!(
            "回放移页越界：from={} to={}",
            from, to
        )));
    }
    if from == to {
        return Ok(());
    }
    let page = layout.order.remove(from as usize - 1);
    layout.order.insert(to as usize - 1, page);
    Ok(())
}

/// Diff 总类型：History 按 Command 类型选择结构档或二进制档。
#[derive(Debug, Clone)]
pub enum Diff {
    /// 结构 Diff。
    Structure(StructureDiff),
    /// 二进制 Diff。
    Binary(BinaryDiff),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对象级 Diff 载荷 JSON 往返：ObjectId 与版本区间必须完整保留。
    #[test]
    fn object_diff_payload_roundtrips() {
        let payload = ObjectDiffPayload {
            object_id: ObjectId(7),
            operation: "TextReplace".to_string(),
            before_version: 2,
            after_version: 3,
            before: serde_json::json!({ "unicode": "旧" }),
            after: serde_json::json!({ "unicode": "新" }),
        };
        let json = serde_json::to_string(&payload).expect("载荷应可序列化");
        let decoded: ObjectDiffPayload = serde_json::from_str(&json).expect("载荷应可反序列化");
        assert_eq!(decoded, payload);
    }

    /// 对象级 Diff 走结构档，不新增 Diff 顶层变体；回放明确报「待 18-02」。
    #[test]
    fn object_change_diff_is_structure_and_reports_pending() {
        let payload = ObjectDiffPayload {
            object_id: ObjectId(9),
            operation: "TextReplace".to_string(),
            before_version: 1,
            after_version: 2,
            before: serde_json::Value::Null,
            after: serde_json::json!({ "unicode": "新" }),
        };
        let diff = Diff::object_change(&payload);
        let Diff::Structure(structure) = &diff else {
            panic!("对象级 Diff 必须装入结构档");
        };
        assert_eq!(structure.operation, "ObjectChange");

        let document = crate::document::Document::new(
            crate::document::DocumentId(1),
            crate::document::DocumentKind::Pdf,
            crate::capability::CapabilitySet::new(),
        );
        // Document 不含对象树，对象级 Diff 必须走 Page 回放（`12` §十九 C-4）。
        let error = diff
            .apply(&document)
            .expect_err("对象级 Diff 不能对 Document 回放");
        assert!(error.to_string().contains("apply_object"));
        let error = diff
            .revert(&document)
            .expect_err("对象级 Diff 不能对 Document 反向回放");
        assert!(error.to_string().contains("revert_object"));
    }
}
