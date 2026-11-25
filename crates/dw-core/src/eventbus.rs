//! EventBus 横切事件总线，不进入执行链；Task 完成等通知经此发布，UI 只监听不直调 Task。

use crate::editing::EditingCancelReason;
use crate::object::ObjectId;
use crate::selection::SelectionChange;

/// 事件结构化载荷（`11` §十 / `12` §十五）。
///
/// `Event` 原本只有种类与中文描述；`SelectionChanged` 与三类编辑事件必须携带结构化数据，
/// 故用可选载荷承载，避免监听方再去反解文案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    /// `SelectionChanged` 的完整前后状态。
    Selection(Box<SelectionChange>),
    /// `EditingStarted`：编辑事务开始。
    EditingStarted {
        /// 被编辑对象。
        object_id: ObjectId,
        /// 开始编辑时的版本。
        version: u64,
    },
    /// `EditingCommitted`：编辑已提交。
    EditingCommitted {
        /// 被编辑对象。
        object_id: ObjectId,
        /// 提交前版本。
        before_version: u64,
        /// 提交后版本。
        after_version: u64,
    },
    /// `EditingCanceled`：编辑已取消。
    EditingCanceled {
        /// 被编辑对象。
        object_id: ObjectId,
        /// 取消原因。
        reason: EditingCancelReason,
    },
    /// `FontRewriteStarted`：字体重写开始（`13` §十一）。
    FontRewriteStarted {
        /// 被编辑对象。
        object_id: u64,
    },
    /// `FontRewriteFinished`：字体重写成功，产出 `FontPatch`。
    FontRewriteFinished {
        /// 被编辑对象。
        object_id: u64,
        /// 解析出的字形映射条目数。
        glyph_count: usize,
    },
    /// `FontRewriteFailed`：字体重写失败。
    FontRewriteFailed {
        /// 被编辑对象。
        object_id: u64,
        /// 中文失败原因。
        reason: String,
    },
}

/// 事件对象：承载事件种类、中文描述与可选结构化载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// 事件种类，例如 TaskCompleted。
    pub kind: String,
    /// 中文事件描述。
    pub message: String,
    /// 结构化载荷；纯通知类事件为 `None`。
    pub payload: Option<EventPayload>,
}

impl Event {
    /// 构造事件。
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            payload: None,
        }
    }

    /// 附带结构化载荷。
    pub fn with_payload(mut self, payload: EventPayload) -> Self {
        self.payload = Some(payload);
        self
    }

    /// `SelectionChanged`（`11` §十）：选择状态变化后发布。
    ///
    /// 载荷为完整 `SelectionChange`，Inspector / Toolbar / Canvas 统一监听此事件。
    pub fn selection_changed(change: SelectionChange) -> Self {
        let message = format!(
            "选择已变化：{} → {} 个对象{}",
            change.old.selected().len(),
            change.new.selected().len(),
            change
                .new
                .primary()
                .map(|id| format!("，主对象 {}", id.0))
                .unwrap_or_default()
        );
        Self::new("SelectionChanged", message)
            .with_payload(EventPayload::Selection(Box::new(change)))
    }

    /// `EditingStarted`（`12` §十五）：编辑事务开始后发布。
    pub fn editing_started(object_id: ObjectId, version: u64) -> Self {
        Self::new(
            "EditingStarted",
            format!("开始编辑对象：{}（版本 {}）", object_id.0, version),
        )
        .with_payload(EventPayload::EditingStarted { object_id, version })
    }

    /// `EditingCommitted`（`12` §十五）：Commit 成功后发布。
    pub fn editing_committed(object_id: ObjectId, before_version: u64, after_version: u64) -> Self {
        Self::new(
            "EditingCommitted",
            format!(
                "编辑已提交：对象 {}（版本 {} → {}）",
                object_id.0, before_version, after_version
            ),
        )
        .with_payload(EventPayload::EditingCommitted {
            object_id,
            before_version,
            after_version,
        })
    }

    /// `EditingCanceled`（`12` §十五 / §十九 C-7）：取消或选择变化导致会话结束时发布。
    pub fn editing_canceled(object_id: ObjectId, reason: EditingCancelReason) -> Self {
        let reason_text = match reason {
            EditingCancelReason::UserCancel => "用户取消",
            EditingCancelReason::SelectionChanged => "选择已变化",
        };
        Self::new(
            "EditingCanceled",
            format!("编辑已取消：对象 {}（{}）", object_id.0, reason_text),
        )
        .with_payload(EventPayload::EditingCanceled { object_id, reason })
    }

    /// `FontRewriteStarted`（`13` §十一）：字体重写开始。
    pub fn font_rewrite_started(object_id: u64) -> Self {
        Self::new(
            "FontRewriteStarted",
            format!("开始字体重写：对象 {}", object_id),
        )
        .with_payload(EventPayload::FontRewriteStarted { object_id })
    }

    /// `FontRewriteFinished`（`13` §十一）：字体重写成功。
    pub fn font_rewrite_finished(object_id: u64, glyph_count: usize) -> Self {
        Self::new(
            "FontRewriteFinished",
            format!(
                "字体重写完成：对象 {}，{} 个字形映射",
                object_id, glyph_count
            ),
        )
        .with_payload(EventPayload::FontRewriteFinished {
            object_id,
            glyph_count,
        })
    }

    /// `FontRewriteFailed`（`13` §十一）：字体重写失败。
    pub fn font_rewrite_failed(object_id: u64, reason: &str) -> Self {
        Self::new(
            "FontRewriteFailed",
            format!("字体重写失败：对象 {}（{}）", object_id, reason),
        )
        .with_payload(EventPayload::FontRewriteFailed {
            object_id,
            reason: reason.to_string(),
        })
    }

    /// `TaskFinished`：Task 成功结束（`01` §6 冻结的 6 类事件之一）。
    pub fn task_finished(task_name: &str) -> Self {
        Self::new("TaskFinished", format!("任务已完成：{}", task_name))
    }

    /// `DocumentModified`：编辑 Command Commit 后发布；失败不发布。
    pub fn document_modified(document_id: u64, operation: &str) -> Self {
        Self::new(
            "DocumentModified",
            format!("文档已修改：{} · {}", document_id, operation),
        )
    }

    /// `DocumentOpened`：打开文档进入 Ready 后发布。
    pub fn document_opened(title: &str) -> Self {
        Self::new("DocumentOpened", format!("已打开：{}", title))
    }

    /// `DocumentClosed`：关闭文档、移除页签后发布。
    pub fn document_closed(document_id: u64) -> Self {
        Self::new("DocumentClosed", format!("已关闭文档：{}", document_id))
    }

    /// `WorkspaceChanged`：视图状态变化后发布。
    pub fn workspace_changed(document_id: u64) -> Self {
        Self::new(
            "WorkspaceChanged",
            format!("工作区已变化：文档 {}", document_id),
        )
    }

    /// `DriverError`：Driver 返回归一化错误时发布，替代该轮 `TaskFinished`。
    pub fn driver_error(message: &str) -> Self {
        Self::new("DriverError", format!("驱动错误：{}", message))
    }

    /// `HistoryRecorded`：Command 成功 Commit 并写入 History 后。
    pub fn history_recorded(command_id: u64) -> Self {
        Self::new("HistoryRecorded", format!("已记录命令：{}", command_id))
    }

    /// `UndoExecuted`：Diff 反向回放成功后。
    pub fn undo_executed(command_id: u64) -> Self {
        Self::new("UndoExecuted", format!("已撤销命令：{}", command_id))
    }

    /// `RedoExecuted`：Diff 正向回放成功后。
    pub fn redo_executed(command_id: u64) -> Self {
        Self::new("RedoExecuted", format!("已重做命令：{}", command_id))
    }

    /// `CheckpointCreated`：Checkpoint 落盘成功后。
    pub fn checkpoint_created(revision: u64) -> Self {
        Self::new(
            "CheckpointCreated",
            format!("已创建检查点：修订号 {}", revision),
        )
    }

    /// `RollbackCompleted`：事务失败且逆序补偿完成后。
    pub fn rollback_completed() -> Self {
        Self::new("RollbackCompleted", "事务已回滚")
    }

    /// `HistoryTruncated`：redo 分支被新 Command 截断后。
    pub fn history_truncated(command_id: u64) -> Self {
        Self::new(
            "HistoryTruncated",
            format!("已截断重做分支：命令 {}", command_id),
        )
    }

    /// `SaveStarted`：SaveTask 开始写盘前，由组装层发布。
    pub fn save_started(path: &str) -> Self {
        Self::new("SaveStarted", format!("开始保存：{}", path))
    }

    /// `SaveCompleted`：写盘成功且状态回落 `Ready` 后发布。
    pub fn save_completed(path: &str, bytes_written: u64, incremental: bool) -> Self {
        Self::new(
            "SaveCompleted",
            format!(
                "保存完成：{} · {} 字节 · {}",
                path,
                bytes_written,
                if incremental {
                    "增量保存"
                } else {
                    "全量保存"
                }
            ),
        )
    }

    /// `SaveFailed`：写盘失败，含归一化后的中文原因。
    pub fn save_failed(path: &str, reason: &str) -> Self {
        Self::new("SaveFailed", format!("保存失败：{}（{}）", path, reason))
    }

    /// `RecoveryCreated`：未保存状态落盘后发布。
    pub fn recovery_created(path: &str) -> Self {
        Self::new("RecoveryCreated", format!("已创建恢复记录：{}", path))
    }

    /// `RecoveryLoaded`：启动检测到恢复记录并加载后发布。
    pub fn recovery_loaded(path: &str) -> Self {
        Self::new("RecoveryLoaded", format!("已加载恢复记录：{}", path))
    }
}

/// 事件总线：记录已发布事件，供 ViewModel 查询结果。
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    /// 事件队列（按发布顺序）。
    events: Vec<Event>,
}

impl EventBus {
    /// 构造空事件总线。
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// 发布事件。
    pub fn publish(&mut self, event: Event) {
        self.events.push(event);
    }

    /// 已发布事件列表。
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// 最近一条事件。
    pub fn last_event(&self) -> Option<&Event> {
        self.events.last()
    }
}
