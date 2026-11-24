//! Recovery 纯存储组件：只负责把「未保存状态摘要」写成 JSON 并清理，
//! 不做恢复执行、不复制 PDF 正文、不调度 Task、不进入执行链（`00` §3 / 文档17 裁决 7）。
//!
//! 落盘布局（与正式 PDF 分离，`Recovery/` 已在 `.gitignore`）：
//! ```text
//! Recovery/session.json    Session 摘要 + Workspace + History 引用 + save_point
//! Recovery/history/        History / Diff 引用
//! Recovery/checkpoint/     Checkpoint 引用
//! Recovery/temp/           临时文件
//! ```
//!
//! Document 正文仍由 Checkpoint + Diff 恢复；本组件不写第二份文档真值。

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::DwError;
use crate::session::{SavePoint, Session, SessionState};

/// 当前 Unix 毫秒：不引入 chrono（`05` R9）。
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 恢复记录中的一个页签：只保存 id 与标题，不复制 Document。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryTab {
    /// 文档标识。
    pub document_id: u64,
    /// 页签标题。
    pub title: String,
}

/// `Recovery/session.json` 的内容：可 JSON 表达的会话摘要。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecoveryInfo {
    /// 会话标识。
    pub session_id: u64,
    /// 会话状态中文标签。
    pub state: String,
    /// 活动文档标识。
    pub active_document_id: Option<u64>,
    /// 活动文档源路径（用字符串存储，落盘后可从 JSON 还原为 PathBuf）。
    pub active_document_path: Option<PathBuf>,
    /// History 游标：指向下一条待记录位置。
    pub history_cursor: usize,
    /// History 记录条数。
    pub history_entries: usize,
    /// Checkpoint 引用数量。
    pub checkpoint_count: usize,
    /// 最近保存点；未保存过为 `None`。
    pub save_point: Option<SavePoint>,
    /// 页签列表（Workspace 投影）。
    pub tabs: Vec<RecoveryTab>,
    /// 当前页（Workspace.view_state 投影）。
    pub current_page: u32,
    /// 缩放（Workspace.view_state 投影）。
    pub zoom: f32,
    /// 记录时间戳（Unix 毫秒）。
    pub timestamp: u64,
}

impl SessionRecoveryInfo {
    /// 会话状态中文标签：只用于展示，不参与业务判断。
    fn state_label(state: SessionState) -> &'static str {
        match state {
            SessionState::Created => "已创建",
            SessionState::Initializing => "初始化中",
            SessionState::Running => "运行中",
            SessionState::Closing => "关闭中",
            SessionState::Disposed => "已释放",
        }
    }

    /// 由 Session 组装摘要：只读，不改动会话状态。
    pub fn from_session(session_id: u64, session: &Session) -> Self {
        let active = session.workspace.view_state().active_document_id;
        let active_document_path = active
            .and_then(|id| session.documents.get(id))
            .and_then(|document| document.source_path());
        let tabs = session
            .workspace
            .tabs()
            .iter()
            .map(|tab| RecoveryTab {
                document_id: tab.document_id.0,
                title: tab.title.clone(),
            })
            .collect();
        Self {
            session_id,
            state: Self::state_label(session.state).to_string(),
            active_document_id: active.map(|id| id.0),
            active_document_path,
            history_cursor: session.history.cursor(),
            history_entries: session.history.entries().len(),
            checkpoint_count: session.history.checkpoints().len(),
            save_point: session.save_point.clone(),
            tabs,
            current_page: session.workspace.view_state().current_page,
            zoom: session.workspace.view_state().zoom,
            timestamp: now_millis(),
        }
    }
}

/// RecoveryStore：与 `CheckpointStore` 同级的纯存储组件，不是 Manager、不是第 4 个真值源。
#[derive(Debug, Clone)]
pub struct RecoveryStore {
    /// 应用根目录（绿色版根目录）。
    root: PathBuf,
}

impl RecoveryStore {
    /// 构造恢复存储：目录不存在时不创建，首次写入时再建。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Recovery 根目录。
    pub fn directory(&self) -> PathBuf {
        self.root.join("Recovery")
    }

    /// `session.json` 的完整路径。
    pub fn session_path(&self) -> PathBuf {
        self.directory().join("session.json")
    }

    /// 确保四种子目录存在：历史引用 / Checkpoint 引用 / 临时文件 / Checkpoint 实体。
    pub fn ensure_directories(&self) -> Result<(), DwError> {
        let root = self.directory();
        for name in ["history", "checkpoint", "checkpoints", "temp"] {
            let path = root.join(name);
            std::fs::create_dir_all(&path).map_err(|error| {
                DwError::Execution(format!(
                    "创建 Recovery 子目录失败：{}（{}）",
                    path.display(),
                    error
                ))
            })?;
        }
        Ok(())
    }

    /// 保存会话摘要：失败只记 WARN，由调用方决定是否阻断业务（保存本身必须成功）。
    pub fn save_session(&self, session_id: u64, session: &Session) -> Result<PathBuf, DwError> {
        self.ensure_directories()?;
        let path = self.session_path();
        let info = SessionRecoveryInfo::from_session(session_id, session);
        let bytes = serde_json::to_vec_pretty(&info)
            .map_err(|error| DwError::Execution(format!("Recovery 摘要序列化失败：{}", error)))?;
        std::fs::write(&path, bytes)
            .map_err(|error| DwError::Execution(format!("写入 Recovery 摘要失败：{}", error)))?;
        Ok(path)
    }

    /// 读取会话摘要；没有恢复记录时返回 `Ok(None)`，不视为错误。
    pub fn load_session(&self) -> Result<Option<SessionRecoveryInfo>, DwError> {
        let path = self.session_path();
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| DwError::Execution(format!("读取 Recovery 摘要失败：{}", error)))?;
        let info = serde_json::from_slice(&bytes)
            .map_err(|error| DwError::Execution(format!("Recovery 摘要解析失败：{}", error)))?;
        Ok(Some(info))
    }

    /// 是否存在恢复记录：启动检测用。
    pub fn exists(&self) -> bool {
        self.session_path().is_file()
    }

    /// 正常退出清理：删除整个 Recovery 目录后重建空目录约定。
    pub fn clear(&self) -> Result<(), DwError> {
        let root = self.directory();
        if root.exists() {
            std::fs::remove_dir_all(&root).map_err(|error| {
                DwError::Execution(format!(
                    "清理 Recovery 目录失败：{}（{}）",
                    root.display(),
                    error
                ))
            })?;
        }
        self.ensure_directories()
    }
}

impl Default for RecoveryStore {
    /// 默认根目录为当前工作目录。
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionManager;

    #[test]
    fn recovery_round_trip_and_clear() {
        let root = std::env::temp_dir().join(format!("dw-recovery-test-{}", std::process::id()));
        let store = RecoveryStore::new(&root);
        let _ = store.clear();
        assert!(!store.exists());

        let mut manager = SessionManager::new();
        let id = manager.create_session();
        let session = manager.active().expect("会话应存在");
        let path = store.save_session(id, session).expect("保存摘要应成功");
        assert_eq!(path, store.session_path());
        assert!(store.exists());

        let loaded = store.load_session().expect("读取应成功").expect("应有记录");
        assert_eq!(loaded.session_id, id);
        assert!(loaded.tabs.is_empty());
        assert_eq!(loaded.history_cursor, 0);

        store.clear().expect("清理应成功");
        assert!(!store.exists());
        assert!(store.load_session().expect("无记录不算错误").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recovery_directories_are_created() {
        let root = std::env::temp_dir().join(format!("dw-recovery-dirs-{}", std::process::id()));
        let store = RecoveryStore::new(&root);
        store.ensure_directories().expect("应创建子目录");
        for name in ["history", "checkpoint", "checkpoints", "temp"] {
            assert!(store.directory().join(name).is_dir());
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
