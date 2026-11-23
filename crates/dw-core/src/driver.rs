//! Driver 只做四件事：调第三方、转数据、统一错误（DriverError）、能力检测。
//! 冻结约束：Driver 目录与 Runtime 分离；DriverManager 是唯一驱动管理者；统一使用 ExecutionContext。

use std::collections::HashMap;
use std::sync::Arc;

use crate::capability::{Capability, CapabilitySet};
use crate::context::ExecutionContext;
use crate::document::Document;
use crate::error::{DriverError, DwError};
use crate::lifecycle::{HealthStatus, IExecutable, IHealthCheck};

/// Stage 1 内置空驱动的名称：集中定义一处，避免调用点硬编码中文字符串。
pub const EMPTY_DRIVER: &str = "空驱动";

/// PDF 驱动的注册名：`Task::driver_name()` 与配置里的键都用它，禁止散落字面量。
pub const PDF_DRIVER: &str = "pdf";

/// Driver trait：第三方适配层，只做适配四件事，不处理业务。
///
/// `Send + Sync` 约束：`DriverManager` 以 `Arc<dyn IDriver>` 持有驱动（`00` §2.2 冻结
/// `resolve` 返回 `Arc<dyn IDriver>`），且 `DriverManager` 自身要放入 Tauri State
/// （`Mutex<DriverManager>`）。`Arc<T>: Send` 要求 `T: Send + Sync`，故 `dyn IDriver`
/// 必须同时满足 `Send + Sync`，否则 `CoreState` 无法进入 Tauri State（`E0277`）。
/// 页编辑能力接口：`03` §5.5 定义的四个能力接口之一的可达入口。
///
/// 它不是新层、不是 Manager，只是 `PdfDriver` 已实现的能力通过 `IDriver` 暴露出来，
/// 让 Task 能经 Driver 调用业务参数不同的批量操作（`ExecutionContext` 固定 8 字段，
/// 业务参数不能塞进 Context）。默认返回 `None`，不影响 Stage 1 空驱动。
pub trait IPageEdit: Send + Sync {
    /// 旋转指定页（页码为当前显示顺序的 1 起位置）。
    fn rotate_pages(
        &self,
        document: &Document,
        pages: &[u32],
        degrees: i32,
    ) -> Result<(), DriverError>;

    /// 删除指定页（页码为当前显示顺序的 1 起位置）。
    fn delete_pages(&self, document: &Document, pages: &[u32]) -> Result<(), DriverError>;

    /// 把 `from` 位置的页移动到 `to` 位置（均为当前显示顺序的 1 起位置）。
    fn move_pages(
        &self,
        document: &Document,
        from: u32,
        to: u32,
    ) -> Result<(), DriverError>;
}

pub trait IDriver: IExecutable + IHealthCheck + Send + Sync {
    /// 驱动名称。
    fn name(&self) -> &str;
    /// 声明该驱动提供的能力。
    fn capabilities(&self) -> CapabilitySet;
    /// 页编辑能力；不具备的驱动保持默认 `None`。
    fn page_editor(&self) -> Option<&dyn IPageEdit> {
        None
    }
}

/// 驱动注册表：按名称登记 Driver 实例，供 DriverManager 查询。
pub struct DriverRegistry {
    /// 名称到驱动的映射：`Arc` 共享所有权，解析后可脱离注册表锁使用。
    drivers: HashMap<String, Arc<dyn IDriver>>,
}

impl DriverRegistry {
    /// 构造空注册表。
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }

    /// 登记驱动实例。
    pub fn register(&mut self, driver: Arc<dyn IDriver>) {
        self.drivers.insert(driver.name().to_string(), driver);
    }

    /// 按名称解析驱动：返回 `Arc` 克隆，调用方无需继续持有注册表锁。
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn IDriver>> {
        self.drivers.get(name).cloned()
    }

    /// 已注册驱动名称（升序）：供启动期配置校验使用（`03` E10）。
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.drivers.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for DriverRegistry {
    /// 默认构造空注册表。
    fn default() -> Self {
        Self::new()
    }
}

/// 唯一 DriverManager：负责 Driver 的发现、加载与生命周期，不进入执行链。
pub struct DriverManager {
    /// 内部注册表。
    registry: DriverRegistry,
}

impl DriverManager {
    /// 构造驱动管理者。
    pub fn new() -> Self {
        Self {
            registry: DriverRegistry::new(),
        }
    }

    /// 登记 Stage 1 内置空驱动：不调用任何第三方，仅用于跑通执行链。
    pub fn register_empty_driver(&mut self) {
        self.registry.register(Arc::new(NullDriver::new()));
    }

    /// 登记任意驱动实例：由组装层（`CoreState`）在启动时调用。
    pub fn register(&mut self, driver: Arc<dyn IDriver>) {
        self.registry.register(driver);
    }

    /// 按名称解析驱动：返回 `Arc<dyn IDriver>`，查找完成即释放锁，调用期间不持 Manager 锁。
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn IDriver>> {
        self.registry.resolve(name)
    }

    /// 已注册驱动名称：启动期校验配置里的驱动名是否存在（`03` E10）。
    ///
    /// 缺此方法则「配置指向不存在的 Driver 要报错不回退」这条规则无法实现。
    pub fn names(&self) -> Vec<String> {
        self.registry.names()
    }
}

impl Default for DriverManager {
    /// 默认构造驱动管理者。
    fn default() -> Self {
        Self::new()
    }
}

/// Stage 1 空驱动：实现 Driver 四项职责，但不调用任何第三方能力。
/// 它是 IDriver 的具体实现，不是新层、不是 Manager、不是 Context。
#[derive(Debug, Clone, Default)]
pub struct NullDriver {}

impl NullDriver {
    /// 构造空驱动。
    pub fn new() -> Self {
        Self {}
    }
}

impl IExecutable for NullDriver {
    /// 初始化空驱动。
    fn initialize(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 执行空驱动：Stage 1 不产生任何真实文档操作。
    fn execute(&self, _context: &ExecutionContext<'_>) -> Result<(), DwError> {
        Ok(())
    }

    /// 取消空驱动。
    fn cancel(&mut self) -> Result<(), DwError> {
        Ok(())
    }

    /// 释放空驱动。
    fn dispose(&mut self) -> Result<(), DwError> {
        Ok(())
    }
}

impl IHealthCheck for NullDriver {
    /// 健康检查：空驱动始终健康。
    fn health(&self) -> Result<HealthStatus, DwError> {
        Ok(HealthStatus::Healthy)
    }

    /// 版本查询。
    fn version(&self) -> Result<String, DwError> {
        Ok("0.1.0".to_string())
    }
}

impl IDriver for NullDriver {
    /// 驱动名称：取自唯一常量，重命名驱动只需改 `EMPTY_DRIVER`。
    fn name(&self) -> &str {
        EMPTY_DRIVER
    }

    /// 声明能力：Stage 1 仅声明渲染，供一次空流程通过 Capability Gate。
    fn capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::new();
        set.insert(Capability::Render);
        set
    }
}
