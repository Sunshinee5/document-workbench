//! 桌面应用库：Tauri IPC 命令注册、启动与运行环境初始化。
//! 执行链起点在 UI / ViewModel，此处只做 IPC 透传，不实现任何业务。

pub mod ipc;

use std::fs;
use std::path::PathBuf;

use dw_runtime::RuntimeDirectories;

/// 启动桌面应用：初始化绿色版运行环境后拉起 Tauri 窗口。
pub fn run() {
    initialize_environment();
    tauri::Builder::default()
        // 文件选择对话框：UI 选文件后拿到绝对路径，再交给 open_document（路径式打开）。
        .plugin(tauri_plugin_dialog::init())
        // 应用组装层：三个唯一 Manager 与唯一 EventBus 由 Tauri State 长生命周期持有。
        .manage(ipc::CoreState::new())
        // 正常退出清理 Recovery；异常退出不会触发，记录保留到下次启动提示恢复。
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let store = dw_core::recovery::RecoveryStore::new(".");
                match store.clear() {
                    Ok(()) => dw_log::info("recovery", "正常退出，已清理恢复记录"),
                    Err(error) => dw_log::warn("recovery", &format!("清理恢复记录失败：{}", error)),
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::app_info,
            ipc::ping,
            ipc::new_session,
            ipc::open_document,
            ipc::close_document,
            ipc::read_document_bytes,
            ipc::get_object_tree,
            ipc::list_documents,
            ipc::set_view_state,
            ipc::driver_names,
            ipc::rotate_pages,
            ipc::delete_pages,
            ipc::move_pages,
            ipc::set_page_selection,
            ipc::clear_page_selection,
            ipc::get_page_layout,
            ipc::undo,
            ipc::redo,
            ipc::can_undo,
            ipc::can_redo,
            ipc::save_document,
            ipc::save_as_document,
            ipc::can_save,
            ipc::is_dirty
        ])
        .run(tauri::generate_context!())
        .expect("文档工作台启动失败");
}

/// 注册 IPC invoke_handler：实际注册在 run() 中由 generate_handler! 完成，保留入口供调用方引用。
pub fn register_handlers() {
    // 命令已由 run() 中的 tauri::generate_handler! 宏统一注册，此处不再重复。
}

/// 初始化绿色版运行环境：解析目录、读取基础配置、写启动日志。
fn initialize_environment() {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let directories = RuntimeDirectories::from_root(&root);

    for directory in [
        &directories.config,
        &directories.logs,
        &directories.cache,
        &directories.temp,
    ] {
        let _ = fs::create_dir_all(directory);
    }

    let config_path = directories.config.join("app.json");
    let config_text = fs::read_to_string(&config_path).unwrap_or_else(|_| "{}".to_string());
    let title = serde_json::from_str::<serde_json::Value>(&config_text)
        .ok()
        .and_then(|value| {
            value
                .get("title")
                .and_then(|item| item.as_str())
                .map(|text| text.to_string())
        })
        .unwrap_or_else(|| "文档工作台".to_string());

    // 统一走 dw_log，禁止再直接 OpenOptions 另写一套日志。
    dw_log::init(&root);
    dw_log::info(
        "startup",
        &format!(
            "文档工作台启动：配置={}，标题={}",
            config_path.display(),
            title
        ),
    );
}
