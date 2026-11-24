//! Feature-04 整链集成测试：验证 Save / Save As 走完整执行链但不进 History。
//!
//! 覆盖：`SaveCommand → build_tasks() → Capability Gate → ExecutionGraph → SaveTask → Driver`
//! 与 `records_history() = false`、`diff() = None`、状态回落 Clean、save_point 前置数据。

use std::sync::Arc;

use dw_core::capability::HasCapability;
use dw_core::command::{
    Command, OpenDocumentCommand, RotatePagesCommand, SaveAsCommand, SaveCommand,
};
use dw_core::context::{CancelToken, Config, ExecutionContext, Progress};
use dw_core::document::{DocumentId, DocumentKind, DocumentSaveState, DocumentState, PageLayout};
use dw_core::driver::DriverManager;
use dw_core::eventbus::EventBus;
use dw_core::session::Session;
use dw_core::task::TaskManager;
use dw_pdf::PdfDriver;
use lopdf::{dictionary, Document as LopdfDocument, Object};

/// 生成一份两页 PDF，供保存链测试。
fn write_sample_pdf(path: &std::path::Path) {
    let mut pdf = LopdfDocument::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let mut page_ids = Vec::new();
    for index in 1..=2 {
        let content = pdf.add_object(lopdf::Stream::new(
            dictionary! {},
            format!("BT (Page {}) Tj ET", index).into_bytes(),
        ));
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
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
            "Count" => 2,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(path).expect("样例 PDF 应保存成功");
}

/// 组装一份指定保存状态的会话 + 文档；生命周期状态固定为 Editing，验证两维正交。
fn setup(path: &std::path::Path, save_state: DocumentSaveState) -> (Session, DocumentId) {
    let mut session = Session::create();
    let kind = DocumentKind::Pdf;
    let id = session.documents.create(kind, kind.declared_capabilities());
    let document = session.documents.get(id).expect("文档必须存在");
    document.set_source_path(path);
    let size = std::fs::metadata(path).map(|item| item.len()).unwrap_or(0);
    document.mutate(|inner| {
        inner.metadata.file_size = size;
        // 不记录修改时间：增量可用性判定在缺时间戳时只比对文件大小。
    });
    document.initialize_page_layout(PageLayout::from_count(2));
    document.set_state(DocumentState::Editing);
    document.set_save_state(save_state);
    (session, id)
}

/// 运行一条保存 Command，返回任务收口的 Diff 数量。
fn run<C: Command>(session: &Session, id: DocumentId, command: &C) -> Vec<dw_core::diff::Diff> {
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
    command.execute(&context).expect("命令前置应成功");
    let tasks = command.build_tasks(&context);

    let mut drivers = DriverManager::new();
    drivers.register(Arc::new(PdfDriver::new()));
    let mut task_manager = TaskManager::new();
    let mut events = EventBus::new();
    task_manager
        .submit_all(tasks, &context, &drivers, &mut events)
        .expect("保存链应成功");
    task_manager.take_diffs()
}

#[test]
fn save_command_runs_full_chain_without_diff_and_returns_clean() {
    let root = std::env::temp_dir().join(format!("dw-save-flow-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    let path = root.join("doc.pdf");
    write_sample_pdf(&path);
    let (session, id) = setup(&path, DocumentSaveState::Dirty);

    let command = SaveCommand::new(&path, true);
    assert!(!command.records_history());
    let diffs = run(&session, id, &command);
    assert!(diffs.is_empty(), "保存不产生 Document Diff");

    let outcome = command.outcome().expect("应有保存结果");
    assert!(outcome.incremental);
    assert!(outcome.bytes_written > 0);
    let document = session.documents.get(id).expect("文档必须存在");
    assert_eq!(document.save_state(), DocumentSaveState::Clean);
    assert_eq!(
        document.state(),
        DocumentState::Editing,
        "生命周期与保存状态正交"
    );
    assert_eq!(document.source_path().as_deref(), Some(path.as_path()));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn clean_document_save_is_skipped() {
    let root = std::env::temp_dir().join(format!("dw-save-skip-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    let path = root.join("doc.pdf");
    write_sample_pdf(&path);
    let before = std::fs::read(&path).expect("源文件可读");
    let (session, id) = setup(&path, DocumentSaveState::Clean);

    let command = SaveCommand::new(&path, true);
    assert!(run(&session, id, &command).is_empty());
    assert!(command.outcome().is_none(), "Clean 时不应调用 Driver 写盘");
    assert_eq!(std::fs::read(&path).expect("源文件仍可读"), before);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn save_as_command_updates_source_path_and_keeps_original() {
    let root = std::env::temp_dir().join(format!("dw-save-as-flow-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    let source = root.join("doc.pdf");
    let target = root.join("copy.pdf");
    write_sample_pdf(&source);
    let before = std::fs::read(&source).expect("源文件可读");
    let (session, id) = setup(&source, DocumentSaveState::Dirty);

    let command = SaveAsCommand::new(&target, false);
    assert!(!command.records_history());
    assert!(run(&session, id, &command).is_empty());

    let outcome = command.outcome().expect("应有保存结果");
    assert!(!outcome.incremental);
    assert_eq!(outcome.path, target);
    assert!(target.is_file());
    assert_eq!(std::fs::read(&source).expect("源文件仍可读"), before);
    let document = session.documents.get(id).expect("文档必须存在");
    assert_eq!(document.source_path().as_deref(), Some(target.as_path()));
    assert_eq!(document.save_state(), DocumentSaveState::Clean);
    assert_eq!(
        document.state(),
        DocumentState::Editing,
        "生命周期与保存状态正交"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// FT-05：保存后新建会话重新打开同一 PDF，第一页旋转仍存在（跨进程可验证的硬证据）。
#[test]
fn ft05_saved_rotation_survives_reopen() {
    let root = std::env::temp_dir().join(format!("dw-ft05-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    let path = root.join("doc.pdf");
    write_sample_pdf(&path);

    // 第一次「进程」：旋转 → 保存。
    {
        let (session, id) = setup(&path, DocumentSaveState::Dirty);
        let rotate = RotatePagesCommand::new(vec![1], 90);
        assert_eq!(run(&session, id, &rotate).len(), 1);
        let save = SaveCommand::new(&path, true);
        assert!(run(&session, id, &save).is_empty());
        assert!(save.outcome().expect("保存结果").incremental);
    }

    // 硬证据 A：完全绕过 Workbench / PdfDriver，直接用 lopdf 从磁盘读取 /Rotate。
    // 这是「真正写盘」的硬证据——不依赖任何内存态或驱动缓存。
    {
        let disk_pdf = LopdfDocument::load(&path).expect("磁盘 PDF 必须能被 lopdf 打开");
        let pages = disk_pdf.get_pages();
        let first_id = *pages.get(&1).expect("磁盘 PDF 必须含第一页");
        let dict = disk_pdf
            .get_dictionary(first_id)
            .expect("第一页必须是字典对象");
        let rotate = dict.get(b"Rotate").expect("第一页必须写有 /Rotate");
        let (_, resolved) = disk_pdf.dereference(rotate).expect("/Rotate 必须可解引用");
        assert_eq!(
            resolved.as_i64().expect("/Rotate 必须是整数"),
            90,
            "磁盘文件中的第一页 /Rotate 必须为 90"
        );
    }

    // 硬证据 B：模拟关闭程序后重启：全新的 Session / Document，只从磁盘文件读取。
    let (reopened, reopened_id) = setup(&path, DocumentSaveState::Clean);
    // 全新进程没有内存页布局：清空后由 PdfDriver 打开时从文件重建。
    reopened
        .documents
        .get(reopened_id)
        .expect("文档必须存在")
        .mutate(|inner| inner.pages = PageLayout::default());
    let open = OpenDocumentCommand::new(&path);
    assert!(run(&reopened, reopened_id, &open).is_empty());
    let document = reopened.documents.get(reopened_id).expect("文档必须存在");
    assert_eq!(
        document.page_layout().rotation_of(1),
        90,
        "重新打开后第一页仍须旋转"
    );
    assert_eq!(
        document.save_state(),
        DocumentSaveState::Clean,
        "重新打开是 Clean"
    );
    let _ = std::fs::remove_dir_all(root);
}
