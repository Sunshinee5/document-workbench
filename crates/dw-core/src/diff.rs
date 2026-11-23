//! Diff 数据模型：恢复 `Command → Diff → Checkpoint` 链路（11.1 第三节）。
//! Diff 分两档：结构 Diff（可 JSON 表达）与二进制 Diff（内容流等大对象）。
//! 冻结约束：Diff 不是层、不是 Manager，只是 History 回放所依据的数据。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}

/// Diff 总类型：History 按 Command 类型选择结构档或二进制档。
#[derive(Debug, Clone)]
pub enum Diff {
    /// 结构 Diff。
    Structure(StructureDiff),
    /// 二进制 Diff。
    Binary(BinaryDiff),
}
