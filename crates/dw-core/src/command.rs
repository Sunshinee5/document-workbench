//! Command 是撤销的唯一真值源（语义层），负责记录 Undo；Task 绝不记录 Undo、不存历史、不做快照。
//! Command 继承统一生命周期 IExecutable。
//! 11 章修正版 B-01 / B-02：Command 只表达意图并产出 Task（`build_tasks`），不持有 TaskManager、
//! 不持有 EventBus，也不存在 CommandExecutor / TaskBuilder 类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::action::{Action, ActionSource};
use crate::context::ExecutionContext;
use crate::driver::SaveOutcome;
use crate::error::DwError;
use crate::lifecycle::IExecutable;
use crate::task::{
    DeletePagesTask, MovePagesTask, OpenDocumentTask, RotatePagesTask, RotateTask, SaveTask, Task,
};

/// 命令类型枚举：替代不可序列化的 trait object 身份，供 History 记录与重放选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    /// 新建会话。
    NewSession,
    /// 打开文档。
    Open,
    /// 旋转页面。
    RotatePage,
    /// 删除页面。
    DeletePage,
    /// 移动页面（Feature-02）。
    MovePage,
    /// 对象级编辑提交（`17` 章新增；与页面编辑共用同一条 History）。
    EditObject,
    /// 合并文档。
    Merge,
    /// 保存文档。
    Save,
    /// 另存为（R1.1 新增；与 Save 共享 `SaveTask`，靠目标路径区分）。
    SaveAs,
    /// 导出 Word。
    ExportWord,
}

/// 撤销模式：每条 Command 自己声明，不设全局 Strategy。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoMode {
    /// 轻量操作：用反向 Diff 撤销。
    DiffUndo,
    /// 复杂操作：回滚到更早的 Checkpoint 后重放其后 Diff。
    CheckpointUndo,
}

/// 命令 trait：语义层撤销真值源，Execute / Undo / Redo 均由 Command 负责。
pub trait Command: IExecutable {
    /// 命令类型。
    fn kind(&self) -> CommandKind;

    /// Command 生成 Task 的动作（不是独立类型、不是 Manager）。
    ///
    /// 只负责产出 Task，不接触 Driver / Runtime；Capability Gate 仍由 TaskManager 入队前统一执行。
    fn build_tasks(&self, context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>>;

    /// 撤销：必须带 context，否则无法恢复 Document。
    fn undo(&mut self, context: &ExecutionContext<'_>) -> Result<(), DwError>;

    /// 重做：必须带 context，否则无法重放 Document 变更。
    fn redo(&mut self, context: &ExecutionContext<'_>) -> Result<(), DwError>;

    /// 撤销模式：默认反向 Diff。
    fn undo_mode(&self) -> UndoMode {
        UndoMode::DiffUndo
    }

    /// 命令描述（用于历史展示）。
    fn description(&self) -> &str;

    /// 可 JSON 表达的命令参数：写入 `CommandRecord.params`，供 History 重放。
    ///
    /// 冻结约束（`02` D1 / §2）：不得输出引用、句柄或 trait object。
    /// 无参命令返回 `Value::Null`，但每个具体 Command 仍应显式实现。
    fn params(&self) -> Value {
        Value::Null
    }

    /// 是否写入 History。
    ///
    /// 判据：**只有产生 `Document` Diff 的命令才进 History**（`00` §5）。
    /// 打开 / 关闭 / 切标签 / 缩放 / 滚动 / 跳页 / 选择不改变文档内容，返回 `false`。
    ///
    /// 注意：返回 `false` **不是跳过执行**，该命令仍走完整执行链
    /// （`execute → build_tasks → Capability Gate → ExecutionGraph → Task → Driver`）。
    fn records_history(&self) -> bool {
        true
    }
}

/// Command 来源分类：AI 唯一身份即 CommandSource::Ai（Command Generator）。
/// 冻结约束：只有四个变体，批处理是一次生成 N 条 Command 的入口，不是第五种来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSource {
    /// 用户。
    User,
    /// AI 生成。
    Ai,
    /// 插件。
    Plugin,
    /// 宏。
    Macro,
}

/// 新建会话命令：把一次用户意图归一化为 Command，Stage 1 只跑空流程。
#[derive(Debug, Clone)]
pub struct NewSessionCommand {
    /// 中文命令描述。
    description: String,
    /// 命令来源。
    source: CommandSource,
}

impl NewSessionCommand {
    /// 由 Action 生成 Command（AI / 插件 / 宏与用户走同一入口）。
    pub fn from_action(action: &dyn Action) -> Self {
        let source = match action.source() {
            ActionSource::User => CommandSource::User,
            ActionSource::Ai => CommandSource::Ai,
            ActionSource::Plugin => CommandSource::Plugin,
            ActionSource::Macro => CommandSource::Macro,
        };
        Self {
            description: format!("新建会话（来源：{}）", action.name()),
            source,
        }
    }

    /// 命令来源。
    pub fn source(&self) -> CommandSource {
        self.source
    }
}

impl IExecutable for NewSessionCommand {
    /// 初始化命令。
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 执行命令：Stage 1 不修改任何文档，只通过校验。
    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 取消命令。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放命令。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for NewSessionCommand {
    /// 命令类型。
    fn kind(&self) -> CommandKind {
        CommandKind::NewSession
    }

    /// Stage 1 空流程：用旋转任务占位，验证 Command → TaskManager → Driver 通道已打通。
    fn build_tasks(&self, _context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(RotateTask::default())]
    }

    /// 撤销：Stage 1 空流程无需回滚内容。
    fn undo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 重做：Stage 1 空流程无需重放内容。
    fn redo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 撤销模式：新建会话属轻量结构变更，走反向 Diff。
    fn undo_mode(&self) -> UndoMode {
        UndoMode::DiffUndo
    }

    /// 命令描述。
    fn description(&self) -> &str {
        &self.description
    }

    /// 新建会话参数：只有来源，无可回放业务参数。
    fn params(&self) -> Value {
        serde_json::json!({ "source": format!("{:?}", self.source) })
    }

    /// 新建会话不改变任何文档内容，无 Diff 可回放，因此不进 History（`00` §5 判据）。
    fn records_history(&self) -> bool {
        false
    }
}

/// 打开文档命令：Feature-01 的唯一业务命令。
///
/// 职责边界（`00` §8.5）：Document 已由 `CoreState` 在 submit 之前 `create()` 完成，
/// 本命令只产出 `OpenDocumentTask`，不创建 Document、不碰 DocumentStore。
#[derive(Debug, Clone)]
pub struct OpenDocumentCommand {
    /// 待打开文件路径（命令自持，构造 Task 时传给 Task）。
    path: PathBuf,
    /// 中文命令描述。
    description: String,
}

impl OpenDocumentCommand {
    /// 由文件路径构造命令。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let description = format!(
            "打开文档：{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("未命名")
        );
        Self { path, description }
    }

    /// 待打开路径。
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl IExecutable for OpenDocumentCommand {
    /// 初始化命令。
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 命令级前置：打开不修改文档内容，无命令级副作用。
    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 取消命令。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放命令。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for OpenDocumentCommand {
    /// 命令类型。
    fn kind(&self) -> CommandKind {
        CommandKind::Open
    }

    /// 产出唯一的打开任务。
    fn build_tasks(&self, _context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(OpenDocumentTask::new(self.path.clone()))]
    }

    /// 打开无内容变更，无反向 Diff 可应用：`Ok(())` 并明确说明。
    ///
    /// 冻结约束（`02` §1.6.2）：禁止把 `undo(open)` 实现成「关闭文档」之类的替代语义。
    fn undo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 同上：打开无内容变更，无 Diff 可重放。
    fn redo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 命令描述。
    fn description(&self) -> &str {
        &self.description
    }

    /// 打开命令参数：路径按字符串输出，不含引用或句柄。
    fn params(&self) -> Value {
        serde_json::json!({ "path": self.path.to_string_lossy() })
    }

    /// 打开不改变文档内容，无 Diff 可回放，因此不进 History（`00` §5 判据、`02` §1.6）。
    ///
    /// 注意：返回 `false` 不是跳过执行，本命令仍走
    /// `execute → build_tasks → Capability Gate → ExecutionGraph → Task → Driver`。
    fn records_history(&self) -> bool {
        false
    }
}

/// 旋转页面命令：Feature-02 的第一个编辑命令。
///
/// 只表达意图与业务参数，不接触 Document，也不产 Diff（Diff 由 Task 生成）。
#[derive(Debug, Clone)]
pub struct RotatePagesCommand {
    /// 页位置（1 起）。
    pages: Vec<u32>,
    /// 顺时针角度。
    degrees: i32,
}

impl RotatePagesCommand {
    /// 构造旋转命令。
    pub fn new(pages: Vec<u32>, degrees: i32) -> Self {
        Self { pages, degrees }
    }

    /// 目标页位置。
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }

    /// 旋转角度。
    pub fn degrees(&self) -> i32 {
        self.degrees
    }
}

impl IExecutable for RotatePagesCommand {
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for RotatePagesCommand {
    fn kind(&self) -> CommandKind {
        CommandKind::RotatePage
    }

    fn build_tasks(&self, _context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(RotatePagesTask::new(
            self.pages.clone(),
            self.degrees,
        ))]
    }

    /// 撤销语义归 Feature-04；本章只产出反向所需的 Diff。
    fn undo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn redo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "旋转页面"
    }

    fn params(&self) -> Value {
        serde_json::json!({ "pages": self.pages, "degrees": self.degrees })
    }

    fn records_history(&self) -> bool {
        true
    }
}

/// 删除页面命令。
#[derive(Debug, Clone)]
pub struct DeletePagesCommand {
    /// 页位置（1 起）。
    pages: Vec<u32>,
}

impl DeletePagesCommand {
    /// 构造删除命令。
    pub fn new(pages: Vec<u32>) -> Self {
        Self { pages }
    }

    /// 目标页位置。
    pub fn pages(&self) -> &[u32] {
        &self.pages
    }
}

impl IExecutable for DeletePagesCommand {
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for DeletePagesCommand {
    fn kind(&self) -> CommandKind {
        CommandKind::DeletePage
    }

    fn build_tasks(&self, _context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(DeletePagesTask::new(self.pages.clone()))]
    }

    fn undo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn redo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "删除页面"
    }

    fn params(&self) -> Value {
        serde_json::json!({ "pages": self.pages })
    }

    fn records_history(&self) -> bool {
        true
    }
}

/// 移动页面命令。
#[derive(Debug, Clone)]
pub struct MovePagesCommand {
    /// 原位置（1 起）。
    from: u32,
    /// 目标位置（1 起）。
    to: u32,
}

impl MovePagesCommand {
    /// 构造移动命令。
    pub fn new(from: u32, to: u32) -> Self {
        Self { from, to }
    }

    /// 原位置。
    pub fn from(&self) -> u32 {
        self.from
    }

    /// 目标位置。
    pub fn to(&self) -> u32 {
        self.to
    }
}

impl IExecutable for MovePagesCommand {
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for MovePagesCommand {
    fn kind(&self) -> CommandKind {
        CommandKind::MovePage
    }

    fn build_tasks(&self, _context: &ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(MovePagesTask::new(self.from, self.to))]
    }

    fn undo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn redo(&mut self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "移动页面"
    }

    fn params(&self) -> Value {
        serde_json::json!({ "from": self.from, "to": self.to })
    }

    fn records_history(&self) -> bool {
        true
    }
}

/// 保存命令：只表达「把当前文档写到目标路径」的意图。
///
/// 保存改变的是文件而非 Document 内容，不产生 Diff，因此 `records_history() = false`；
/// 但它仍走完整执行链 `build_tasks() → Capability Gate → ExecutionGraph → Task → Driver`。
#[derive(Debug, Clone)]
pub struct SaveCommand {
    /// 目标路径（由 IPC 从 Document.source_path 取得后传入）。
    target: PathBuf,
    /// 期望的保存方式；实际策略由 Driver 判定。
    incremental: bool,
    /// 与 SaveTask 共享的保存结果句柄。
    outcome: Arc<Mutex<Option<SaveOutcome>>>,
}

impl SaveCommand {
    /// 构造保存命令。
    pub fn new(target: impl Into<PathBuf>, incremental: bool) -> Self {
        Self::with_outcome(target, incremental, Arc::new(Mutex::new(None)))
    }

    /// 以外部结果句柄构造：组装层可在提交后直接读取保存结果。
    pub fn with_outcome(
        target: impl Into<PathBuf>,
        incremental: bool,
        outcome: Arc<Mutex<Option<SaveOutcome>>>,
    ) -> Self {
        Self {
            target: target.into(),
            incremental,
            outcome,
        }
    }

    /// 目标路径。
    pub fn target(&self) -> &PathBuf {
        &self.target
    }

    /// 保存结果；未执行时返回 `None`。
    pub fn outcome(&self) -> Option<SaveOutcome> {
        // 锁中毒时取出内部值继续读：一次异常不应让 UI 永远拿不到保存结果。
        self.outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl IExecutable for SaveCommand {
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn execute(&self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for SaveCommand {
    fn kind(&self) -> CommandKind {
        CommandKind::Save
    }

    fn build_tasks(&self, _context: &crate::context::ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(SaveTask::with_outcome(
            self.target.clone(),
            self.incremental,
            true,
            self.outcome.clone(),
        ))]
    }

    /// 保存无内容变更，无反向 Diff 可应用。
    fn undo(&mut self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 保存无内容变更，无 Diff 可重放。
    fn redo(&mut self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "保存文档"
    }

    fn params(&self) -> Value {
        serde_json::json!({ "path": self.target.to_string_lossy() })
    }

    /// 保存不产生 Document Diff，不入 History（文档17 裁决 4）。
    fn records_history(&self) -> bool {
        false
    }
}

/// 另存为命令：与 SaveCommand 共享 SaveTask，靠任务自持的目标路径区分。
#[derive(Debug, Clone)]
pub struct SaveAsCommand {
    /// 新目标路径。
    target: PathBuf,
    /// 期望的保存方式；另存为新文件通常由 Driver 判定走全量。
    incremental: bool,
    /// 与 SaveTask 共享的保存结果句柄。
    outcome: Arc<Mutex<Option<SaveOutcome>>>,
}

impl SaveAsCommand {
    /// 构造另存为命令。
    pub fn new(target: impl Into<PathBuf>, incremental: bool) -> Self {
        Self::with_outcome(target, incremental, Arc::new(Mutex::new(None)))
    }

    /// 以外部结果句柄构造：组装层可在提交后直接读取保存结果。
    pub fn with_outcome(
        target: impl Into<PathBuf>,
        incremental: bool,
        outcome: Arc<Mutex<Option<SaveOutcome>>>,
    ) -> Self {
        Self {
            target: target.into(),
            incremental,
            outcome,
        }
    }

    /// 目标路径。
    pub fn target(&self) -> &PathBuf {
        &self.target
    }

    /// 保存结果；未执行时返回 `None`。
    pub fn outcome(&self) -> Option<SaveOutcome> {
        // 锁中毒时取出内部值继续读：一次异常不应让 UI 永远拿不到保存结果。
        self.outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl IExecutable for SaveAsCommand {
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn execute(&self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Command for SaveAsCommand {
    fn kind(&self) -> CommandKind {
        CommandKind::SaveAs
    }

    fn build_tasks(&self, _context: &crate::context::ExecutionContext<'_>) -> Vec<Box<dyn Task>> {
        vec![Box::new(SaveTask::with_outcome(
            self.target.clone(),
            self.incremental,
            false,
            self.outcome.clone(),
        ))]
    }

    /// 另存为无内容变更，无反向 Diff 可应用。
    fn undo(&mut self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 另存为无内容变更，无 Diff 可重放。
    fn redo(&mut self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    fn description(&self) -> &str {
        "另存为"
    }

    fn params(&self) -> Value {
        serde_json::json!({ "path": self.target.to_string_lossy() })
    }

    /// 另存为不产生 Document Diff，不入 History（文档17 裁决 4）。
    fn records_history(&self) -> bool {
        false
    }
}
