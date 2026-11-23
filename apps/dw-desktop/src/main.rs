//! Tauri 2 应用入口（骨架）：只负责拉起桌面壳，不承载业务逻辑。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dw_desktop::run();
}
