//! Feature-02 整链集成测试（Implementation Order Step 15）。
//!
//! 覆盖：`Command → build_tasks() → Capability Gate → ExecutionGraph → Task → Driver`
//! 以及 Diff 收口 → History。页面大小用例（1 / 100 / 1000）在 `edit.rs` 单测中覆盖。

use std::sync::Arc;

use dw_core::capability::{Capability, CapabilitySet, HasCapability};
use dw_core::command::{
    Command, DeletePagesCommand, MovePagesCommand, RotatePagesCommand,
};
use dw_core::context::{CancelToken, Config, ExecutionContext, Progress};
use dw_core::diff::Diff;
use dw_core::document::{DocumentId, DocumentKind, PageLayout};
use dw_core::driver::{DriverManager, PDF_DRIVER};
use dw_core::error::DwError;
use dw_core::eventbus::EventBus;
use dw_core::session::{CommandRecord, Session};
use dw_core::task::TaskManager;
use dw_pdf::PdfDriver;

/// 组装一份已就绪的会话 + 文档：页布局直接初始化，编辑链不需要真实文件。
fn setup(page_count: usize, with_edit: bool) -> (Session, DocumentId) {
    let mut session = Session::create();
    let kind = DocumentKind::Pdf;
    let mut capabilities = if with_edit {
        kind.declared_capabilities()
    } else {
        CapabilitySet::new()
    };
    capabilities.insert(Capability::Render);
    let id = session.documents.create(kind, capabilities);
    let document = session.documents.get(id).expect("文档必须存在");
    document.initialize_page_layout(PageLayout::from_count(page_count));
    document.set_state(dw_core::document::DocumentState::Ready);
    (session, id)
}

/// 运行一条 Command 并返回收口的 Diff；完整链路与 CoreState 的编辑 IPC 一致。
fn run<C: Command>(
    session: &Session,
    id: DocumentId,
    command: C,
) -> Result<(Vec<Diff>, CommandRecord), DwError> {
    let document = session.documents.get(id).expect("文档必须存在");
    let context = ExecutionContext {
        session,
        document,
        workspace: &session.workspace,
        selection: &session.selection,
        capability_set: document.capabilities(),
        config: Config::default(),
        progress: Progress::default(),
        cancel_token: CancelToken::default(),
    };
    command.execute(&context)?;
    let tasks = command.build_tasks(&context);

    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    assert!(drivers.names().contains(&PDF_DRIVER.to_string()));

    let mut tasks_manager = TaskManager::new();
    let mut events = EventBus::new();
    tasks_manager.submit_all(tasks, &context, &drivers, &mut events)?;
    let diffs = tasks_manager.take_diffs();

    let record = CommandRecord {
        command_id: 1,
        command_type: command.kind(),
        params: command.params(),
        diff_reference: Vec::new(),
        timestamp: 0,
    };
    Ok((diffs, record))
}

#[test]
fn rotate_command_runs_full_chain_and_produces_one_diff() {
    let (session, id) = setup(3, true);
    let command = RotatePagesCommand::new(vec![1, 3], 90);
    assert!(command.records_history());
    let (diffs, record) = run(&session, id, command).expect("旋转应成功");

    assert_eq!(diffs.len(), 1);
    match &diffs[0] {
        Diff::Structure(structure) => {
            assert_eq!(structure.operation, "RotatePages");
            assert_eq!(structure.payload["degrees"], 90);
        }
        Diff::Binary(_) => panic!("旋转是结构变更，不能落成二进制 Diff"),
    }
    assert_eq!(record.params["pages"][0], 1);
    assert_eq!(record.params["degrees"], 90);

    let layout = session.documents.get(id).unwrap().page_layout();
    assert_eq!(layout.rotation_of(1), 90);
    assert_eq!(layout.rotation_of(3), 90);
    assert_eq!(layout.rotation_of(2), 0);
}

#[test]
fn delete_command_reorders_and_refuses_deleting_all() {
    let (session, id) = setup(5, true);
    let (diffs, record) = run(&session, id, DeletePagesCommand::new(vec![1, 4]))
        .expect("删除应成功");
    assert_eq!(diffs.len(), 1);
    match &diffs[0] {
        Diff::Structure(structure) => assert_eq!(structure.operation, "DeletePages"),
        Diff::Binary(_) => panic!("删页是结构变更"),
    }
    assert_eq!(record.params["pages"][1], 4);
    assert_eq!(
        session.documents.get(id).unwrap().page_layout().order,
        vec![2, 3, 5]
    );
    assert_eq!(session.documents.get(id).unwrap().page_count(), 3);

    let (session, id) = setup(2, true);
    let error = run(&session, id, DeletePagesCommand::new(vec![1, 2])).unwrap_err();
    assert!(error.to_string().contains("至少要保留一页"));
}

#[test]
fn move_command_produces_diff_and_reorders() {
    let (session, id) = setup(5, true);
    let (diffs, record) = run(&session, id, MovePagesCommand::new(5, 1)).expect("移动应成功");
    assert_eq!(diffs.len(), 1);
    match &diffs[0] {
        Diff::Structure(structure) => assert_eq!(structure.operation, "MovePages"),
        Diff::Binary(_) => panic!("移页是结构变更"),
    }
    assert_eq!(record.params["from"], 5);
    assert_eq!(record.params["to"], 1);
    assert_eq!(
        session.documents.get(id).unwrap().page_layout().order,
        vec![5, 1, 2, 3, 4]
    );
}

#[test]
fn gate_rejects_edit_when_capability_not_declared() {
    let (session, id) = setup(3, false);
    let error = run(&session, id, RotatePagesCommand::new(vec![1], 90)).unwrap_err();
    match error {
        DwError::TaskRejected(reason) => {
            assert!(reason.reason.contains("Edit"), "对外唯一出口是 TaskRejected：{}", reason.reason);
        }
        other => panic!("期望 TaskRejected，实际 {}", other),
    }
    // Gate 拒绝发生在 Task 执行前：页面状态未被改动。
    let layout = session.documents.get(id).unwrap().page_layout();
    assert!(layout.rotations.is_empty());
}

// ---- Transaction（02 §3.6）----

use dw_core::lifecycle::{IExecutable, IPausable};
use dw_core::task::Task;

/// 测试专用失败任务：不接触 Document，只在第二个位置失败。
struct FailingTask;

impl IExecutable for FailingTask {
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

impl IPausable for FailingTask {
    fn pause(&mut self) -> Result<(), DwError> {
        Ok(())
    }
    fn resume(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl Task for FailingTask {
    fn required_capability(&self) -> Capability {
        Capability::Edit
    }
    fn task_name(&self) -> &str {
        "故障注入"
    }
    fn driver_name(&self) -> &str {
        PDF_DRIVER
    }
    fn execute_on(
        &self,
        _context: &ExecutionContext<'_>,
        _driver: &dyn dw_core::driver::IDriver,
    ) -> Result<(), DwError> {
        Err(DwError::Execution("故障注入：第二个 Task 失败".to_string()))
    }
}

#[test]
fn transaction_rolls_back_all_completed_tasks_and_discards_diffs() {
    let (session, id) = setup(3, true);
    let before = session.documents.get(id).unwrap().page_layout();

    let document = session.documents.get(id).unwrap();
    let context = ExecutionContext {
        session: &session,
        document,
        workspace: &session.workspace,
        selection: &session.selection,
        capability_set: document.capabilities(),
        config: Config::default(),
        progress: Progress::default(),
        cancel_token: CancelToken::default(),
    };
    let tasks: Vec<Box<dyn Task>> = vec![
        Box::new(dw_core::task::RotatePagesTask::new(vec![1], 90)),
        Box::new(FailingTask),
    ];
    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    let mut task_manager = TaskManager::new();
    let mut events = EventBus::new();

    let error = task_manager
        .submit_all(tasks, &context, &drivers, &mut events)
        .unwrap_err();
    assert!(error.to_string().contains("故障注入"));

    // 第一个 Task 的 90° 必须被回滚，不能留下「半条 Command」。
    assert_eq!(session.documents.get(id).unwrap().page_layout(), before);
    // 本轮 Diff 全部丢弃。
    assert!(task_manager.take_diffs().is_empty());
    // 事件名遵循冻结清单：成功用 TaskFinished，失败用 DriverError。
    let kinds: Vec<&str> = events.events().iter().map(|event| event.kind.as_str()).collect();
    assert!(kinds.contains(&"TaskFinished"));
    assert!(kinds.contains(&"DriverError"));
}
