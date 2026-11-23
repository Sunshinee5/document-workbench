//! 统一错误定义：DriverError / CapabilityNotSupported / TaskRejected 全部收敛到顶层 DwError。

use std::fmt;

/// 项目统一错误枚举：全链路只向上抛这一种顶层错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwError {
    /// 能力不支持：Capability Gate 校验失败。
    CapabilityNotSupported(CapabilityNotSupported),
    /// 任务被拒绝：准入校验未通过。
    TaskRejected(TaskRejected),
    /// 驱动错误：第三方调用失败的归一化表达。
    Driver(DriverError),
    /// 通用执行错误。
    Execution(String),
    /// 用户或系统取消。
    Cancelled,
}

/// 能力不支持错误的详细信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityNotSupported {
    /// 请求的能力名称。
    pub capability: String,
    /// 补充说明。
    pub message: String,
}

/// 任务被拒绝错误的详细信息：Capability Gate 失败时 Task 根本不会创建。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRejected {
    /// 拒绝原因。
    pub reason: String,
}

/// Driver 统一错误：Driver 四项职责之一，即把第三方错误归一化为本类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverError {
    /// 出错的驱动名称。
    pub driver: String,
    /// 第三方原始错误信息。
    pub message: String,
}

impl fmt::Display for DwError {
    /// 中文错误展示。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DwError::CapabilityNotSupported(error) => {
                write!(formatter, "能力不支持: {} ({})", error.capability, error.message)
            }
            DwError::TaskRejected(error) => write!(formatter, "任务被拒绝: {}", error.reason),
            DwError::Driver(error) => {
                write!(formatter, "驱动错误 [{}]: {}", error.driver, error.message)
            }
            DwError::Execution(message) => write!(formatter, "执行错误: {}", message),
            DwError::Cancelled => write!(formatter, "操作已取消"),
        }
    }
}

impl std::error::Error for DwError {}
