//! EventBus 横切事件总线，不进入执行链；Task 完成等通知经此发布，UI 只监听不直调 Task。

/// 事件对象：Stage 1 只承载事件种类与中文描述，避免引入新层。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// 事件种类，例如 TaskCompleted。
    pub kind: String,
    /// 中文事件描述。
    pub message: String,
}

impl Event {
    /// 构造事件。
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
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
