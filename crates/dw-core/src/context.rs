//! ExecutionContext 是一次 Task 执行时临时创建的上下文，随 Task 创建/释放，不是 Session 成员。
//! 内容以 Reference（借用）形式持有 Session / Document / Workspace / Selection，
//! 另含执行期数据 CapabilitySet / Config / Progress / CancelToken；字段严格固定为 8 个，禁止新增。
//! Capability Gate 在此统一校验；借用语义避免按值持有造成的循环引用。

use crate::capability::CapabilitySet;
use crate::document::Document;
use crate::selection::Selection;
use crate::session::Session;
use crate::workspace::Workspace;

/// 冻结字段之一：应用配置。
#[derive(Debug, Clone, Default)]
pub struct Config {}

/// 冻结字段之一：执行进度。
#[derive(Debug, Clone, Default)]
pub struct Progress {}

/// 冻结字段之一：取消令牌。
#[derive(Debug, Clone, Default)]
pub struct CancelToken {}

/// Task 执行期临时上下文：严格 8 个字段，前 4 项为引用（Reference），后 4 项按值持有。
///
/// 生命周期：`Task 创建 → Context 创建 → Task 执行 → Task 结束 → Context 释放`。
/// 由 TaskManager 在 Task 创建时构造，Task 结束时释放；不归入 Session 成员。
/// Session 自身按值持有 Workspace 与 Selection，Context 仅借用它们，不重复拥有、不构成循环。
#[derive(Debug, Clone)]
pub struct ExecutionContext<'a> {
    /// 1. 会话引用（共享状态，横切全链路；不按值持有）。
    pub session: &'a Session,
    /// 2. 当前文档引用（统一 Document 多态；不按值持有）。
    pub document: &'a Document,
    /// 3. 工作区引用（Session 的成员；不按值持有）。
    pub workspace: &'a Workspace,
    /// 4. 选择集引用（Session 的成员；不按值持有）。
    pub selection: &'a Selection,
    /// 5. 能力集合（Capability Gate 校验依据，执行期数据按值持有）。
    pub capability_set: CapabilitySet,
    /// 6. 配置（执行期数据，按值持有）。
    pub config: Config,
    /// 7. 进度（执行期数据，按值持有）。
    pub progress: Progress,
    /// 8. 取消令牌（执行期数据，按值持有）。
    pub cancel_token: CancelToken,
}
