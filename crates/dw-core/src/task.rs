//! Task 是最小执行单元，只调用 Driver，不保存历史 / 不 Undo / 不做快照。
//! 冻结约束：TaskManager 是唯一任务管理者；ExecutionGraph(DAG) 仅为其内部数据结构，不是新架构层。
//! Task 使用 Rust trait 实现（禁止 C++ 风格 class），并继承 IPausable 扩展生命周期。

use crate::capability::{capability_gate, Capability};
use crate::diff::Diff;
use crate::document::{DocumentSnapshot, DocumentState};
use crate::driver::{DriverManager, IDriver, PDF_DRIVER, EMPTY_DRIVER};
use crate::error::{DwError, TaskRejected};
use crate::eventbus::{Event, EventBus};
use crate::lifecycle::{IExecutable, IPausable};
use std::path::{Path, PathBuf};

/// Task trait：统一执行器，入队前须经 Capability Gate 校验，执行时只调用 Driver。
pub trait Task: IExecutable {
    /// 该任务所需能力，供入队前的 Capability Gate 使用（执行阶段不再判断）。
    fn required_capability(&self) -> Capability;
    /// 任务名称。
    fn task_name(&self) -> &str;

    /// 该任务使用的 Driver 名称，由 `TaskManager` 据此 `resolve` 后注入。
    ///
    /// 默认使用 Stage 1 的空驱动；需要真实第三方的 Task 覆盖它（如 PDF 用 `PDF_DRIVER`）。
    /// Task 只声明名字，不自己持有 `DriverManager`、不自己 resolve（`00` §8.1.6）。
    fn driver_name(&self) -> &str {
        EMPTY_DRIVER
    }

    /// 本 Task 产出的 Diff（一个 Task 最多 1 个，`02` §5.3）。
    ///
    /// 默认 `None`：打开 / 选择等不改内容的 Task 不产 Diff。
    /// TaskManager 在 submit 成功后统一收口，Command 再把它们交给 History。
    fn diff(&self) -> Option<Diff> {
        None
    }

    /// 经 Driver 执行：冻结链路 Task → Driver → Runtime 的唯一入口。
    /// 默认实现先让 Driver 完成适配，再执行 Task 自身。
    fn execute_on(
        &self,
        context: &crate::context::ExecutionContext<'_>,
        driver: &dyn IDriver,
    ) -> Result<(), DwError> {
        driver.execute(context)?;
        self.execute(context)
    }
}

/// 旋转任务骨架示例：证明 Task 以 trait 实现，Stage 1 函数体为空流程。
#[derive(Debug, Clone, Default)]
pub struct RotateTask {}

impl IExecutable for RotateTask {
    /// 初始化。
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 执行空流程：Stage 1 不修改任何文档。
    fn execute(&self, _context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 取消。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl IPausable for RotateTask {
    /// 暂停。
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 恢复。
    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for RotateTask {
    /// 所需能力：Stage 1 空流程使用渲染能力占位。
    fn required_capability(&self) -> Capability {
        Capability::Render
    }

    /// 任务名称。
    fn task_name(&self) -> &str {
        "旋转页面（空流程）"
    }
}

/// 打开文档任务：自持文件路径（业务参数不进 Context），只做初始化，不创建 Document。
///
/// 前置条件（`00` §8.5）：`CoreState` 已调用 `DocumentStore::create()` 并把 `source_path`
/// 写入文档，因此本 Task 拿到 Context 时 Document 已存在。
#[derive(Debug, Clone)]
pub struct OpenDocumentTask {
    /// 待打开文件的路径：Task 自持，经 Driver 读取时由 Driver 从 Document 取用。
    pub path: PathBuf,
}

impl OpenDocumentTask {
    /// 构造打开任务。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 文件合法性校验：存在 + 是文件 + 可读。不做格式解析（那是 Driver 的事）。
    fn validate(&self) -> Result<(), DwError> {
        let path: &Path = &self.path;
        if !path.exists() {
            return Err(DwError::Execution(format!(
                "文件不存在：{}",
                path.display()
            )));
        }
        if !path.is_file() {
            return Err(DwError::Execution(format!(
                "不是文件：{}",
                path.display()
            )));
        }
        // 以只读方式打开一次：既验证可读权限，也能拿到文件大小。
        std::fs::File::open(path)
            .map_err(|error| DwError::Execution(format!("文件无法读取：{}（{}）", path.display(), error)))?;
        Ok(())
    }
}

impl IExecutable for OpenDocumentTask {
    /// 初始化：校验文件合法性（`00` §8.5 第 2 条「文件合法性校验在 Task 内做」）。
    fn initialize(&mut self) -> Result<(), DwError> {
        self.validate()
    }

    /// 执行：只推进状态机，元数据由 Driver 写入。
    fn execute(&self, context: &crate::context::ExecutionContext<'_>) -> Result<(), DwError> {
        // Driver 已在 execute_on 的默认流程里跑完；此处只负责状态收口。
        context.document.set_state(DocumentState::Ready);
        Ok(())
    }

    /// 取消。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl IPausable for OpenDocumentTask {
    /// 暂停：打开是短任务，不支持中途暂停。
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 恢复。
    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for OpenDocumentTask {
    /// 打开文档需要渲染能力：Gate 依据 `create()` 时声明的能力判定（`00` §4.12）。
    fn required_capability(&self) -> Capability {
        Capability::Render
    }

    /// 任务名称。
    fn task_name(&self) -> &str {
        "打开文档"
    }

    /// 使用 PDF 驱动读取元数据。
    fn driver_name(&self) -> &str {
        crate::driver::PDF_DRIVER
    }

    /// 打开流程：校验 → Created 置 Loading → Driver 读元数据 → 置 Ready。
    ///
    /// 显式覆盖默认实现，是为了让状态机的三段（Created / Loading / Ready）真正跑出来，
    /// 而不是跳过 Loading 直接 Ready（`01` §5.3）。
    fn execute_on(
        &self,
        context: &crate::context::ExecutionContext<'_>,
        driver: &dyn IDriver,
    ) -> Result<(), DwError> {
        self.validate()?;
        context.document.set_state(DocumentState::Loading);
        // Driver 内部读元数据并写回 document；失败时状态停在 Loading，由调用方移除文档。
        driver.execute(context)?;
        context.document.set_state(DocumentState::Ready);
        Ok(())
    }
}

/// 旋转页面任务：只把 Command 的参数转成 Driver 调用，不保存状态、不写 History。
#[derive(Debug, Clone)]
pub struct RotatePagesTask {
    /// 页位置（1 起）。
    pages: Vec<u32>,
    /// 顺时针角度。
    degrees: i32,
}

impl RotatePagesTask {
    /// 构造旋转任务。
    pub fn new(pages: Vec<u32>, degrees: i32) -> Self {
        Self { pages, degrees }
    }
}

impl IExecutable for RotatePagesTask {
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

impl IPausable for RotatePagesTask {
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for RotatePagesTask {
    fn required_capability(&self) -> Capability {
        Capability::Edit
    }

    fn task_name(&self) -> &str {
        "旋转页面"
    }

    fn driver_name(&self) -> &str {
        PDF_DRIVER
    }

    fn diff(&self) -> Option<Diff> {
        Some(Diff::rotation(&crate::diff::RotationDiff {
            pages: self.pages.clone(),
            degrees: self.degrees,
        }))
    }

    fn execute_on(
        &self,
        context: &crate::context::ExecutionContext<'_>,
        driver: &dyn IDriver,
    ) -> Result<(), DwError> {
        let editor = driver.page_editor().ok_or_else(|| {
            DwError::Execution(format!("驱动 {} 不支持页编辑", driver.name()))
        })?;
        editor
            .rotate_pages(context.document, &self.pages, self.degrees)
            .map_err(DwError::Driver)?;
        self.execute(context)
    }
}

/// 删除页面任务。
#[derive(Debug, Clone)]
pub struct DeletePagesTask {
    /// 页位置（1 起）。
    pages: Vec<u32>,
}

impl DeletePagesTask {
    /// 构造删除任务。
    pub fn new(pages: Vec<u32>) -> Self {
        Self { pages }
    }
}

impl IExecutable for DeletePagesTask {
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

impl IPausable for DeletePagesTask {
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for DeletePagesTask {
    fn required_capability(&self) -> Capability {
        Capability::Edit
    }

    fn task_name(&self) -> &str {
        "删除页面"
    }

    fn driver_name(&self) -> &str {
        PDF_DRIVER
    }

    fn diff(&self) -> Option<Diff> {
        Some(Diff::delete_page(&crate::diff::DeletePageDiff {
            pages: self.pages.clone(),
        }))
    }

    fn execute_on(
        &self,
        context: &crate::context::ExecutionContext<'_>,
        driver: &dyn IDriver,
    ) -> Result<(), DwError> {
        let editor = driver.page_editor().ok_or_else(|| {
            DwError::Execution(format!("驱动 {} 不支持页编辑", driver.name()))
        })?;
        editor
            .delete_pages(context.document, &self.pages)
            .map_err(DwError::Driver)?;
        self.execute(context)
    }
}

/// 移动页面任务。
#[derive(Debug, Clone)]
pub struct MovePagesTask {
    /// 原位置（1 起）。
    from: u32,
    /// 目标位置（1 起）。
    to: u32,
}

impl MovePagesTask {
    /// 构造移动任务。
    pub fn new(from: u32, to: u32) -> Self {
        Self { from, to }
    }
}

impl IExecutable for MovePagesTask {
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

impl IPausable for MovePagesTask {
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for MovePagesTask {
    fn required_capability(&self) -> Capability {
        Capability::Edit
    }

    fn task_name(&self) -> &str {
        "移动页面"
    }

    fn driver_name(&self) -> &str {
        PDF_DRIVER
    }

    fn diff(&self) -> Option<Diff> {
        Some(Diff::move_page(&crate::diff::MovePageDiff {
            from: self.from,
            to: self.to,
        }))
    }

    fn execute_on(
        &self,
        context: &crate::context::ExecutionContext<'_>,
        driver: &dyn IDriver,
    ) -> Result<(), DwError> {
        let editor = driver.page_editor().ok_or_else(|| {
            DwError::Execution(format!("驱动 {} 不支持页编辑", driver.name()))
        })?;
        editor
            .move_pages(context.document, self.from, self.to)
            .map_err(DwError::Driver)?;
        self.execute(context)
    }
}

/// 唯一 TaskManager：负责创建、暂停、恢复、取消、依赖分析与优先级，不处理业务，不进入执行链。
///
/// 冻结约束（11 章修正版 B-01 / 11.1 第十节）：TaskManager 不持有 DriverManager 与 EventBus，
/// 二者由调用方（dw-desktop 的 CoreState）传入，保证三个 Manager 与 EventBus 各自唯一。
pub struct TaskManager {
    /// 内部执行图（DAG）。
    graph: ExecutionGraph,
    /// 本轮 submit 收到的 Diff，按执行顺序追加；由调用方取走写 History。
    diffs: Vec<Diff>,
}

impl TaskManager {
    /// 构造任务管理者：不内置驱动，驱动由调用方登记后传入。
    pub fn new() -> Self {
        Self {
            graph: ExecutionGraph::default(),
            diffs: Vec::new(),
        }
    }

    /// 取走本轮收口的 Diff（顺序即回放顺序），取后清空。
    pub fn take_diffs(&mut self) -> Vec<Diff> {
        std::mem::take(&mut self.diffs)
    }

    /// 提交单个 Task：先 Capability Gate，再经 Driver 执行。
    ///
    /// `drivers` 是唯一 Driver 入口，`events` 是唯一事件总线，均由调用方传入。
    pub fn submit(
        &mut self,
        task: Box<dyn Task>,
        context: &crate::context::ExecutionContext<'_>,
        drivers: &DriverManager,
        events: &mut EventBus,
    ) -> Result<(), DwError> {
        // Capability Gate：Task 入图前唯一一次校验。
        // `00` §4.4：`CapabilityNotSupported` 是内部信号，对外唯一出口是 `TaskRejected`。
        if let Err(error) = capability_gate(context, task.required_capability()) {
            let reason = match error {
                DwError::CapabilityNotSupported(info) => {
                    format!("缺少能力：{}（{}）", info.capability, info.message)
                }
                other => other.to_string(),
            };
            return Err(DwError::TaskRejected(TaskRejected { reason }));
        }
        // 按 Task 声明的名字解析 Driver（`00` §8.4）：查找即释放锁，执行期间不持 Manager 锁。
        let driver_name = task.driver_name().to_string();
        let driver = drivers.resolve(&driver_name).ok_or_else(|| {
            DwError::Execution(format!("未注册的驱动：{}（见 DriverManager::names）", driver_name))
        })?;
        // 生命周期：先 initialize（任务级前置校验），再经 Driver 执行。
        let mut task = task;
        task.initialize()?;
        // 解引用 `Arc` 得到 `&dyn IDriver`：Task 只认 trait 对象，不认所有权形式。
        task.execute_on(context, driver.as_ref())?;
        // Diff 由 Task 构造、TaskManager 收口；顺序即执行顺序，不得重排（`02` §5.3）。
        if let Some(diff) = task.diff() {
            self.diffs.push(diff);
        }
        // 事件通知：UI 只监听，不直接调用 Task。
        events.publish(Event::task_finished(task.task_name()));
        Ok(())
    }

    /// 提交一批 Task（`Command::build_tasks` 的产物）：**具有事务语义**（`02` §3.6）。
    ///
    /// 全部成功 → Diff 保留，调用方进入 Commit；任一失败 → 逆序回滚已完成的 Task、
    /// 丢弃本轮 Diff、发布 `DriverError`，对外只返回错误。History 永不记录半个 Command。
    pub fn submit_all(
        &mut self,
        tasks: Vec<Box<dyn Task>>,
        context: &crate::context::ExecutionContext<'_>,
        drivers: &DriverManager,
        events: &mut EventBus,
    ) -> Result<(), DwError> {
        let diff_baseline = self.diffs.len();
        let mut completed: Vec<DocumentSnapshot> = Vec::new();

        for task in tasks {
            let before = context.document.snapshot();
            match self.submit(task, context, drivers, events) {
                Ok(()) => completed.push(before),
                Err(error) => {
                    // 逆序补偿：后执行的先回滚，避免把文档留在中间态。
                    for snapshot in completed.iter().rev() {
                        context.document.restore(snapshot);
                    }
                    self.diffs.truncate(diff_baseline);
                    events.publish(Event::driver_error(&error.to_string()));
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// 内部执行图（DAG）。
    pub fn graph(&self) -> &ExecutionGraph {
        &self.graph
    }
}

impl Default for TaskManager {
    /// 默认构造。
    fn default() -> Self {
        Self::new()
    }
}

/// 执行图节点标识：TaskManager 内部 DAG 的节点编号。
pub type TaskNodeId = usize;

/// ExecutionGraph(DAG)：TaskManager 的内部数据结构，不是新层、不是新架构概念。
#[derive(Debug, Clone, Default)]
pub struct ExecutionGraph {
    /// 有向边：(前置节点, 后继节点)，表示依赖顺序。
    edges: Vec<(TaskNodeId, TaskNodeId)>,
}

impl ExecutionGraph {
    /// 构造空执行图：Stage 1 空流程尚无依赖边。
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
        }
    }

    /// 依赖边数量：供 Stage 1 状态展示，空流程恒为 0。
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}
