//! 集成测试：打开文档的完整执行链（`00` §8.5 的落地验证）。
//!
//! 覆盖的链路：
//! `DocumentStore::create()`（数据分配）→ `ExecutionContext`（8 字段）
//! → `OpenDocumentCommand` → `build_tasks()` → `Capability Gate`
//! → `ExecutionGraph` → `OpenDocumentTask` → `PdfDriver`（lopdf）
//! → `Document.state: Created → Loading → Ready`
//!
//! 这个测试存在的意义：P0-1 / P0-2 说的是「打开时 Document 不存在 → Gate 必拒 → 打不开」。
//! 本测试就是那句话的反证——只要先 `create()` 再走链，就能真的打开一个 PDF。

use std::path::Path;
use std::sync::Arc;

use dw_core::capability::HasCapability;
use dw_core::command::{Command, OpenDocumentCommand};
use dw_core::context::{CancelToken, Config, ExecutionContext, Progress};
use dw_core::document::{DocumentKind, DocumentState};
use dw_core::driver::{DriverManager, PDF_DRIVER};
use dw_core::eventbus::EventBus;
use dw_core::lifecycle::IExecutable;
use dw_core::session::Session;
use dw_core::task::TaskManager;
use dw_pdf::PdfDriver;
use lopdf::{dictionary, Document as Pdf, Object, Stream};

/// 生成一份 3 页的样例 PDF。
fn write_sample_pdf(path: &Path, pages: u32) {
    let mut pdf = Pdf::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let font_id = pdf.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = pdf.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_ids = Vec::new();
    for index in 1..=pages {
        let content = format!("BT /F1 24 Tf 72 720 Td (Page {}) Tj ET", index);
        let content_id = pdf.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        page_ids.push(page_id);
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    pdf.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    let info_id = pdf.add_object(dictionary! {
        "Title" => Object::string_literal("Feature-01 验收样张"),
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.trailer.set("Info", info_id);
    pdf.compress();
    pdf.save(path).expect("样例 PDF 应能保存");
}

/// 打开流程的端到端验证：状态机必须走到 Ready，元数据必须真实落地。
#[test]
fn open_document_reaches_ready_with_metadata() {
    let path = std::env::temp_dir().join("dw-feature01-open-flow.pdf");
    write_sample_pdf(&path, 3);

    // ① 数据分配：DocumentStore::create() —— 不是业务执行，不进 Command/Task（`00` §8.5）。
    let mut session = Session::create();
    let kind = DocumentKind::from_path(&path).expect("扩展名应识别为 PDF");
    let document_id = session.documents.create(kind, kind.declared_capabilities());
    let document = session
        .documents
        .get(document_id)
        .expect("文档应已注册");
    document.set_source_path(&path);
    session.workspace.add_tab(document_id, "Feature-01 验收样张");
    assert_eq!(document.state(), DocumentState::Created);

    // ② 组装 Context（严格 8 字段），能力来自 create() 时的声明。
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

    // ③ Command → build_tasks → Gate → ExecutionGraph → Task → Driver。
    let command = OpenDocumentCommand::new(&path);
    command.execute(&context).expect("命令级前置应通过");
    let tasks = command.build_tasks(&context);
    assert_eq!(tasks.len(), 1, "打开命令只产一个 Task");

    let mut task_manager = TaskManager::new();
    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    assert!(drivers.names().contains(&PDF_DRIVER.to_string()));
    let mut events = EventBus::new();

    task_manager
        .submit_all(tasks, &context, &drivers, &mut events)
        .expect("打开应成功：Gate 依据 create() 时的声明，不应被判 TaskRejected");

    // ④ 状态机与元数据。
    assert_eq!(document.state(), DocumentState::Ready);
    let metadata = document.metadata();
    assert_eq!(metadata.page_count, 3);
    assert_eq!(metadata.title, "Feature-01 验收样张");
    assert!(metadata.file_size > 0);
    assert!(events.last_event().is_some(), "应发布 TaskCompleted 事件");

    let _ = std::fs::remove_file(&path);
}

/// 打开不存在的文件：必须失败，且失败后不推进到 Ready。
#[test]
fn open_missing_file_fails_and_keeps_state() {
    let mut session = Session::create();
    let kind = DocumentKind::Pdf;
    let document_id = session.documents.create(kind, kind.declared_capabilities());
    let document = session.documents.get(document_id).unwrap();
    document.set_source_path(std::env::temp_dir().join("dw-不存在的文件.pdf"));

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

    let command = OpenDocumentCommand::new(std::env::temp_dir().join("dw-不存在的文件.pdf"));
    let tasks = command.build_tasks(&context);
    let mut task_manager = TaskManager::new();
    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    let mut events = EventBus::new();

    let result = task_manager.submit_all(tasks, &context, &drivers, &mut events);
    assert!(result.is_err(), "文件不存在必须报错");
    assert_ne!(
        document.state(),
        DocumentState::Ready,
        "失败后不得停在 Ready"
    );
}

/// 能力声明是 Gate 的唯一依据：把 PDF 的 Render 摘掉，打开必须被 TaskRejected。
#[test]
fn gate_rejects_when_render_not_declared() {
    use dw_core::capability::CapabilitySet;

    let path = std::env::temp_dir().join("dw-feature01-gate.pdf");
    write_sample_pdf(&path, 1);

    let mut session = Session::create();
    // 故意声明空能力集：模拟「create() 时没写 declared_capabilities」的错误实现。
    let document_id = session
        .documents
        .create(DocumentKind::Pdf, CapabilitySet::new());
    let document = session.documents.get(document_id).unwrap();
    document.set_source_path(&path);

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

    let command = OpenDocumentCommand::new(&path);
    let tasks = command.build_tasks(&context);
    let mut task_manager = TaskManager::new();
    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    let mut events = EventBus::new();

    let error = task_manager
        .submit_all(tasks, &context, &drivers, &mut events)
        .expect_err("空能力集必须被 Gate 拒绝");
    let text = error.to_string();
    assert!(text.contains("任务被拒绝"), "对外唯一出口应是 TaskRejected：{}", text);

    let _ = std::fs::remove_file(&path);
}
