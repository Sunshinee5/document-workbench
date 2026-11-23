//! Feature-03 集成测试：Diff 双向回放、Cursor / Redo 分支、Checkpoint。
//! 事务回滚用例在 `page_edit_flow.rs` 中已有，本文件不重复。

use dw_core::capability::CapabilitySet;
use dw_core::checkpoint::CheckpointStore;
use dw_core::command::CommandKind;
use dw_core::diff::{DeletePageDiff, Diff, DiffRef, MovePageDiff, RotationDiff};
use dw_core::document::{Document, DocumentId, DocumentKind, DocumentState, PageLayout};
use dw_core::session::{CommandRecord, History};

fn document_with(count: usize) -> Document {
    let document = Document::new(DocumentId(1), DocumentKind::Pdf, CapabilitySet::new());
    document.initialize_page_layout(PageLayout::from_count(count));
    document.set_state(DocumentState::Ready);
    document
}

fn record(id: u64) -> CommandRecord {
    CommandRecord {
        command_id: id,
        command_type: CommandKind::RotatePage,
        params: serde_json::json!({ "id": id }),
        diff_reference: vec![DiffRef(format!("cmd{:03}-1", id))],
        timestamp: id,
        version: 1,
    }
}

#[test]
fn rotation_diff_apply_and_revert() {
    let document = document_with(3);
    let diff = Diff::rotation(&RotationDiff {
        pages: vec![1, 3],
        degrees: 90,
    });
    diff.apply(&document).expect("apply 应成功");
    assert_eq!(document.page_layout().rotation_of(1), 90);
    assert_eq!(document.page_layout().rotation_of(3), 90);
    diff.revert(&document).expect("revert 应成功");
    assert_eq!(document.page_layout().rotation_of(1), 0);
    assert_eq!(document.page_layout().rotation_of(3), 0);
    diff.apply(&document).expect("redo 应成功");
    assert_eq!(document.page_layout().rotation_of(1), 90);
}

#[test]
fn delete_diff_restores_original_pages_and_rotation() {
    let document = document_with(5);
    document.set_page_layout({
        let mut layout = document.page_layout();
        layout.rotations.insert(3, 180);
        layout
    });
    let diff = Diff::delete_page(&DeletePageDiff {
        pages: vec![1, 3],
        removed: vec![1, 3],
        rotations: vec![(1, 0), (3, 180)],
    });
    diff.apply(&document).expect("apply 应成功");
    assert_eq!(document.page_layout().order, vec![2, 4, 5]);
    assert_eq!(document.page_layout().rotations.len(), 0);

    diff.revert(&document).expect("revert 应成功");
    assert_eq!(document.page_layout().order, vec![1, 2, 3, 4, 5]);
    assert_eq!(document.page_layout().rotation_of(3), 180);
    assert_eq!(document.page_layout().rotation_of(1), 0);
}

#[test]
fn move_diff_apply_and_revert() {
    let document = document_with(4);
    let diff = Diff::move_page(&MovePageDiff { from: 1, to: 4 });
    diff.apply(&document).expect("apply 应成功");
    assert_eq!(document.page_layout().order, vec![2, 3, 4, 1]);
    diff.revert(&document).expect("revert 应成功");
    assert_eq!(document.page_layout().order, vec![1, 2, 3, 4]);
}

#[test]
fn history_cursor_and_redo_branch_truncation() {
    let mut history = History::new();
    history.record(record(1));
    history.record(record(2));
    history.record(record(3));
    assert_eq!(history.cursor(), 3);
    assert!(!history.can_redo());

    history.undo();
    history.undo();
    assert_eq!(history.cursor(), 1);
    assert!(history.can_undo());
    assert!(history.can_redo());
    assert_eq!(history.current().map(|item| item.command_id), Some(1));

    let removed = history.truncate_redo();
    assert_eq!(removed.len(), 2);
    assert_eq!(removed[0].command_id, 2);
    assert_eq!(removed[1].command_id, 3);
    assert!(!history.can_redo());

    history.record(record(4));
    assert_eq!(history.entries().len(), 2);
    assert_eq!(history.entries()[1].command_id, 4);
    assert_eq!(history.entries()[1].version, 1);
}

#[test]
fn checkpoint_snapshot_roundtrip() {
    let root = std::env::temp_dir().join(format!("dw-history-test-{}", std::process::id()));
    let document = document_with(8);
    let store = CheckpointStore::new(&root);
    let reference = store.save(100, &document).expect("落盘应成功");
    let loaded = store.load(&reference).expect("读取应成功");
    assert_eq!(loaded.command_id, 100);
    assert_eq!(loaded.snapshot.pages.len(), 8);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn one_hundred_consecutive_undo_redo_do_not_panic() {
    let document = document_with(2);
    let diff = Diff::rotation(&RotationDiff {
        pages: vec![1],
        degrees: 90,
    });
    for _ in 0..100 {
        diff.apply(&document).unwrap();
        diff.revert(&document).unwrap();
    }
    assert_eq!(document.page_layout().rotation_of(1), 0);
}
