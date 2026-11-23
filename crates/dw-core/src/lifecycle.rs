//! 统一生命周期：基类 IExecutable + 扩展 IPausable（Task）/ IHealthCheck（Driver）。
//! 冻结约束：Session 是状态生命周期（Create→Initialize→Running→Closing→Disposed），不继承 IExecutable。

use crate::context::ExecutionContext;
use crate::error::DwError;

/// 统一生命周期基类：Initialize / Execute / Cancel / Dispose，Execute 携带全链路唯一上下文。
pub trait IExecutable {
    /// 初始化。
    fn initialize(&mut self) -> Result<(), DwError>;
    /// 执行：统一携带 ExecutionContext，Capability Gate 已在入队前完成校验。
    fn execute(&self, context: &ExecutionContext) -> Result<(), DwError>;
    /// 取消。
    fn cancel(&mut self) -> Result<(), DwError>;
    /// 释放资源。
    fn dispose(&mut self) -> Result<(), DwError>;
}

/// Task 扩展生命周期：暂停 / 恢复。
pub trait IPausable: IExecutable {
    /// 暂停。
    fn pause(&mut self) -> Result<(), DwError>;
    /// 恢复。
    fn resume(&mut self) -> Result<(), DwError>;
}

/// 健康状态：供 Driver 健康检查返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// 健康。
    Healthy,
    /// 降级可用。
    Degraded,
    /// 不可用。
    Unhealthy,
}

/// Driver 扩展生命周期：健康检查与版本查询。
pub trait IHealthCheck {
    /// 健康检查。
    fn health(&self) -> Result<HealthStatus, DwError>;
    /// 版本查询。
    fn version(&self) -> Result<String, DwError>;
}
