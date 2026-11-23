//! Office Driver 骨架：只做四件事（调第三方 / 转数据 / 统一错误 / 能力检测），当前全部空实现。
//! 第三方占位：LibreOffice（MVP 默认）、ONLYOFFICE（后续 Driver）。

use dw_core::capability::CapabilitySet;
use dw_core::context::ExecutionContext;
use dw_core::driver::IDriver;
use dw_core::error::DwError;
use dw_core::lifecycle::{HealthStatus, IExecutable, IHealthCheck};

/// Office 驱动骨架：实现 Driver 四项职责，函数体全部为空。
#[derive(Debug, Clone, Default)]
pub struct OfficeDriver {}

impl IExecutable for OfficeDriver {
    /// 初始化（空实现）。
    fn initialize(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }

    /// 执行（空实现）。
    fn execute(&self, _context: &ExecutionContext) -> Result<(), DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }

    /// 取消（空实现）。
    fn cancel(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }

    /// 释放（空实现）。
    fn dispose(&mut self) -> Result<(), DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }
}

impl IHealthCheck for OfficeDriver {
    /// 健康检查（空实现）。
    fn health(&self) -> Result<HealthStatus, DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }

    /// 版本查询（空实现）。
    fn version(&self) -> Result<String, DwError> {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }
}

impl IDriver for OfficeDriver {
    /// 驱动名称（空实现）。
    fn name(&self) -> &str {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }

    /// 声明能力（空实现）。
    fn capabilities(&self) -> CapabilitySet {
        // TODO(Stage 4)：LibreOffice 默认
        todo!()
    }
}

/// LibreOffice 适配器占位（MVP 默认，外部进程方式调用）。
#[derive(Debug, Clone, Default)]
pub struct LibreOfficeAdapter {}

/// ONLYOFFICE 适配器占位（后续 Driver）。
#[derive(Debug, Clone, Default)]
pub struct OnlyOfficeAdapter {}
