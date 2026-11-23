//! Diff 数据模型：恢复 `Command → Diff → Checkpoint` 链路（11.1 第三节）。
//! Diff 分两档：结构 Diff（可 JSON 表达）与二进制 Diff（内容流等大对象）。
//! 冻结约束：Diff 不是层、不是 Manager，只是 History 回放所依据的数据。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::document::{Document, PageLayout};
use crate::error::DwError;

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
