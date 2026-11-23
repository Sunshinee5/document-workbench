//! Runtime：运行环境骨架，只管理字体 / 缓存 / 临时 / 日志 / 配置目录。
//! 冻结约束：Runtime 与 Driver 目录分离，Runtime 不处理业务，也不放第三方 SDK 逻辑；
//! 第三方（DLL / EXE / SDK / Model / CLI）后续在此留下接入点，由 Driver 调用。

use std::path::Path;
use std::path::PathBuf;

/// 绿色版运行目录集合：全部相对应用根目录，保证可整体拷贝。
#[derive(Debug, Clone, Default)]
pub struct RuntimeDirectories {
    /// 字体目录。
    pub fonts: PathBuf,
    /// 缓存目录。
    pub cache: PathBuf,
    /// 临时目录。
    pub temp: PathBuf,
    /// 日志目录。
    pub logs: PathBuf,
    /// 配置目录。
    pub config: PathBuf,
}

impl RuntimeDirectories {
    /// 依据绿色版根目录解析各子目录，全部使用相对路径。
    pub fn from_root(root: &Path) -> Self {
        Self {
            fonts: root.join("resources").join("fonts"),
            cache: root.join("cache"),
            temp: root.join("Recovery").join("temp"),
            logs: root.join("logs"),
            config: root.join("config"),
        }
    }
}

/// 第三方运行时占位：DLL / EXE / SDK / Model / CLI 的接入由 Driver 负责，Runtime 不判断业务。
#[derive(Debug, Clone, Default)]
pub struct ThirdPartyRuntime {}
