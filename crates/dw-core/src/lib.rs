//! dw-core：Document Workbench 核心库。
//! 固化冻结架构的全部概念；当前为骨架，函数体为空实现，后续按执行链逐层填充。
//! 执行链：UI → ViewModel → Action → Command → Task → Driver → Runtime（Manager 与 Store 均为横切，不入链）。
//! 数据流（11.1 第九节）：Command → Task → Driver → Document.mutate() → Diff → History → Checkpoint。

pub mod action;
pub mod capability;
pub mod checkpoint;
pub mod clipboard;
pub mod command;
pub mod context;
pub mod coordinate;
pub mod diff;
pub mod document;
pub mod driver;
pub mod editing;
pub mod error;
pub mod eventbus;
pub mod font;
pub mod hittest;
pub mod lifecycle;
pub mod object;
pub mod object_tree;
pub mod page;
pub mod recovery;
pub mod selection;
pub mod session;
pub mod task;
pub mod workspace;

#[cfg(test)]
mod skeleton_tests {
    use crate::capability::CapabilitySet;
    use crate::command::CommandKind;
    use crate::context::{CancelToken, Config, ExecutionContext, Progress};
    use crate::diff::DiffRef;
    use crate::document::{Document, DocumentId, DocumentKind};
    use crate::session::{
        CommandRecord, DocumentStore, History, Session, SessionManager, SessionState,
    };

    /// 构造一条测试用命令记录：参数与 Diff 引用按 command_id 生成，便于断言顺序。
    fn sample_record(command_id: u64) -> CommandRecord {
        CommandRecord {
            command_id,
            command_type: CommandKind::RotatePage,
            params: serde_json::json!({ "page": command_id, "rotation": 90 }),
            diff_reference: vec![DiffRef(format!("diff{:03}", command_id))],
            timestamp: command_id,
            version: 1,
        }
    }

    /// 骨架级编译测试：验证冻结类型可被构造、8 字段上下文可组装。
    #[test]
    fn frozen_types_are_constructible() {
        // `selection_state` 是私有字段（`11` 约束 5），统一由 `Session::create()` 构造。
        let session = Session::create();
        let document = Document::new(DocumentId(1), DocumentKind::Pdf, CapabilitySet::default());
        let context = ExecutionContext {
            session: &session,
            document: &document,
            workspace: &session.workspace,
            selection: &session.selection,
            capability_set: CapabilitySet::default(),
            config: Config::default(),
            progress: Progress::default(),
            cancel_token: CancelToken::default(),
        };
        assert_eq!(context.session.state, SessionState::Created);
    }

    /// 骨架级常量测试：Checkpoint 每 100 条 Command 生成一次。
    #[test]
    fn checkpoint_interval_is_fixed() {
        assert_eq!(crate::session::CheckpointStore::INTERVAL, 100);
    }

    /// 验收测试 1：redo 分支截断。A → B → C，undo 到 B 后记录 D，C 必须被截断为 A → B → D。
    #[test]
    fn history_redo_truncates_branch() {
        let mut history = History::new();
        history.record(sample_record(1));
        history.record(sample_record(2));
        history.record(sample_record(3));
        assert_eq!(history.cursor(), 3);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // 撤销返回的是「被撤销的那一条」，即最后执行的 C；游标回退到 2（B 为最后已执行）。
        let undone = history.undo().expect("应能撤销到 B");
        assert_eq!(undone.command_id, 3);
        assert_eq!(history.cursor(), 2);
        assert!(history.can_redo());

        history.record(sample_record(4));
        let ids: Vec<u64> = history
            .entries()
            .iter()
            .map(|record| record.command_id)
            .collect();
        assert_eq!(ids, vec![1, 2, 4]);
        assert_eq!(history.cursor(), 3);
        assert!(!history.can_redo());
    }

    /// 验收测试 2：Document 写路径。经 mutate() 改 DocumentInner，再用 read() 读回必须可见。
    #[test]
    fn document_mutate_and_read() {
        let document = Document::new(DocumentId(9), DocumentKind::Pdf, CapabilitySet::default());
        assert_eq!(document.read(|inner| inner.revision), 0);

        let written = document.mutate(|inner| {
            inner.revision += 1;
            inner.revision
        });
        assert_eq!(written, 1);
        assert_eq!(document.read(|inner| inner.revision), 1);
    }

    /// 验收测试 3：DocumentStore 分配的 id 必须严格单调递增。
    #[test]
    fn document_store_issues_monotonic_ids() {
        let mut store = DocumentStore::new();
        let first = store.create(DocumentKind::Pdf, CapabilitySet::default());
        let second = store.create(DocumentKind::Pdf, CapabilitySet::default());
        assert!(second.0 > first.0);
        assert_eq!(store.len(), 2);
        assert!(!store.is_empty());
        assert!(store.get(first).is_some());
        assert!(store.get(second).is_some());
    }

    /// 验收测试 4：SessionManager 必须真正持有创建的 Session，而不是造完就丢。
    #[test]
    fn session_manager_keeps_created_sessions() {
        let mut manager = SessionManager::new();
        assert_eq!(manager.session_count(), 0);
        assert!(manager.active_id().is_none());

        let first = manager.create_session();
        let second = manager.create_session();
        assert_ne!(first, second);
        assert_eq!(manager.session_count(), 2);
        assert_eq!(manager.active_id(), Some(second));
        assert!(manager.active().is_some());
        assert!(manager.active_mut().is_some());
        // 多 Session 多 History 隔离：两个会话各自独立的 DocumentStore 与 History。
        assert!(manager.active().map(|session| session.documents.is_empty()) == Some(true));
    }
}
