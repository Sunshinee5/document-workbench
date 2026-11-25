//! Capability 是静态能力声明（HasXXX），不进入执行链；只作为 Task 入队前的准入校验点（Capability Gate）。
//! 冻结约束：全链路只检查一次；禁止在 Task / Driver 内重复判断能力。

use std::collections::HashSet;

use crate::context::ExecutionContext;
use crate::error::{CapabilityNotSupported, DwError};

/// 文档能力枚举：以 HasCapability 替代 if(pdf) / if(word) / if(image) 条件分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// 渲染。
    Render,
    /// 搜索。
    Search,
    /// OCR 识别。
    Ocr,
    /// 格式转换。
    Convert,
    /// 导出。
    Export,
    /// 打印。
    Print,
    /// 页面级编辑（旋转 / 删除 / 移动，15 起使用）。
    Edit,
    /// 对象级内容编辑（文本 / 图片 / OCR，18 起使用；与 `Edit` 并存，`07` 裁决 7）。
    ObjectEditing,
}

/// 能力集合：挂在 Document 与 ExecutionContext 上。
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    /// 内部能力集合。
    items: HashSet<Capability>,
}

impl CapabilitySet {
    /// 构造空能力集。
    pub fn new() -> Self {
        Self {
            items: HashSet::new(),
        }
    }

    /// 是否具备指定能力。
    pub fn has(&self, capability: Capability) -> bool {
        self.items.contains(&capability)
    }

    /// 新增一项能力。
    pub fn insert(&mut self, capability: Capability) {
        self.items.insert(capability);
    }
}

/// 具备能力的统一接口：Document 等类型实现它，供 Capability Gate 查询。
pub trait HasCapability {
    /// 返回该对象声明的能力集合。
    fn capabilities(&self) -> CapabilitySet;

    /// 是否具备指定能力（默认实现，Gate 只调用此处）。
    fn has_capability(&self, capability: Capability) -> bool {
        self.capabilities().has(capability)
    }
}

/// Capability Gate：全链路唯一的能力校验点，在 Task 创建入队前调用一次。
/// 校验为 true 才创建具体 Task；为 false 返回 TaskRejected，Task 根本不会创建。
pub fn capability_gate(
    context: &ExecutionContext<'_>,
    required: Capability,
) -> Result<(), DwError> {
    if context.capability_set.has(required) {
        Ok(())
    } else {
        Err(DwError::CapabilityNotSupported(CapabilityNotSupported {
            capability: format!("{:?}", required),
            message: "执行上下文未声明该能力，任务不予创建".to_string(),
        }))
    }
}
