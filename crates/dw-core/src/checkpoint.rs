//! Checkpoint 存储与恢复钩子（Feature-03 Step 8）。
//!
//! Checkpoint 不是 History，只是 Recovery 加速：每 100 条 Command 保存一次 Document State，
//! 恢复时从最近 Checkpoint 起重放其后 Diff。恢复实现归 Feature-17，这里只落数据接口。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diff::CheckpointRef;
use crate::document::{Document, DocumentSnapshot};
use crate::error::DwError;

/// Checkpoint 实体：Document 完整状态 + 生成它的命令号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// 生成时的命令号。
    pub command_id: u64,
    /// 文档状态快照。
    pub snapshot: DocumentSnapshot,
}

/// CheckpointStore：纯存储组件，不是 Manager、不是新层。
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    /// 根目录，落盘到 `<root>/Recovery/checkpoints/`。
    root: PathBuf,
}

impl CheckpointStore {
    /// 构造存储：目录还不存在时不创建，首次写入时再建。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Checkpoint 文件目录。
    pub fn directory(&self) -> PathBuf {
        self.root.join("Recovery").join("checkpoints")
    }

    /// 保存一份 Checkpoint，返回可写进 History 的引用。
    pub fn save(&self, command_id: u64, document: &Document) -> Result<CheckpointRef, DwError> {
        let directory = self.directory();
        std::fs::create_dir_all(&directory)
            .map_err(|error| DwError::Execution(format!("创建 Checkpoint 目录失败：{}", error)))?;
        let name = format!("checkpoint-{:06}", command_id);
        let path = directory.join(&name);
        let checkpoint = Checkpoint {
            command_id,
            snapshot: document.snapshot(),
        };
        let bytes = serde_json::to_vec(&checkpoint)
            .map_err(|error| DwError::Execution(format!("Checkpoint 序列化失败：{}", error)))?;
        std::fs::write(&path, bytes)
            .map_err(|error| DwError::Execution(format!("写入 Checkpoint 失败：{}", error)))?;
        Ok(CheckpointRef(path.to_string_lossy().to_string()))
    }

    /// 读取一份 Checkpoint；恢复执行本身归 Feature-17。
    pub fn load(&self, reference: &CheckpointRef) -> Result<Checkpoint, DwError> {
        let bytes = std::fs::read(Path::new(&reference.0))
            .map_err(|error| DwError::Execution(format!("读取 Checkpoint 失败：{}", error)))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| DwError::Execution(format!("Checkpoint 反序列化失败：{}", error)))
    }
}

/// Crash Recovery Hook：只冻结接口，不做恢复实现（文档 16 §一）。
///
/// 17 实现恢复时只需提供本 trait 的实现者，不必改 History / Checkpoint 数据结构。
pub trait RecoveryHook: Send + Sync {
    /// Checkpoint 落盘成功后的通知。
    fn on_checkpoint(&self, reference: &CheckpointRef) -> Result<(), DwError> {
        let _ = reference;
        Ok(())
    }

    /// 崩溃后的恢复入口；本阶段只有签名，没有实现。
    fn on_recover(&self, reference: &CheckpointRef) -> Result<(), DwError> {
        let _ = reference;
        Ok(())
    }
}

/// 空钩子：默认 NoOp，供组装层在 17 之前使用。
#[derive(Debug, Clone, Default)]
pub struct NoopRecoveryHook {}

impl RecoveryHook for NoopRecoveryHook {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::document::{DocumentId, DocumentKind, PageLayout};

    #[test]
    fn checkpoint_saves_and_loads_snapshot() {
        let root = std::env::temp_dir().join(format!("dw-checkpoint-test-{}", std::process::id()));
        let document = Document::new(DocumentId(1), DocumentKind::Pdf, CapabilitySet::new());
        document.initialize_page_layout(PageLayout::from_count(5));
        let store = CheckpointStore::new(&root);
        let reference = store.save(100, &document).expect("保存应成功");
        let loaded = store.load(&reference).expect("读取应成功");
        assert_eq!(loaded.command_id, 100);
        assert_eq!(loaded.snapshot.pages.len(), 5);
        let _ = std::fs::remove_dir_all(root);
    }
}
