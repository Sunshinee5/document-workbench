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

    /// 构造任务完成事件。
    pub fn task_completed(task_name: &str) -> Self {
        Self::new("TaskCompleted", format!("任务已完成：{}", task_name))
    }

    /// 构造页结构变化事件：UI 据此刷新缩略图与页码（Feature-02 Step 14）。
    pub fn pages_changed(document_id: u64, operation: &str) -> Self {
        Self::new(
            "PagesChanged",
            format!("页面已变化：文档 {} · {}", document_id, operation),
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
        Self {
            events: Vec::new(),
        }
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
