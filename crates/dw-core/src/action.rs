//! Action 表示交互来源（用户 / AI / 插件 / 宏），是 Command 的输入，不处理业务。
//! 冻结约束：AI 唯一身份是 Command Generator，因此 ActionSource::Ai 只产生 Command，禁止直连 Runtime。

/// Action 来源枚举：对应冻结定义 User / AI / Plugin / Macro。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    /// 用户交互（按钮 / 菜单）。
    User,
    /// AI 生成的命令。
    Ai,
    /// 插件触发。
    Plugin,
    /// 宏触发。
    Macro,
}

/// Action trait：把一次交互归一化为可生成 Command 的输入。
pub trait Action {
    /// 返回交互来源。
    fn source(&self) -> ActionSource;
    /// 返回动作名称。
    fn name(&self) -> &str;
}

/// 用户动作：由 ViewModel 产生，只描述意图，不处理业务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAction {
    /// 动作名称。
    name: String,
    /// 动作来源。
    source: ActionSource,
}

impl UserAction {
    /// 构造用户动作。
    pub fn new(name: impl Into<String>, source: ActionSource) -> Self {
        Self {
            name: name.into(),
            source,
        }
    }

    /// 构造来自用户的动作。
    pub fn from_user(name: impl Into<String>) -> Self {
        Self::new(name, ActionSource::User)
    }
}

impl Action for UserAction {
    /// 返回交互来源。
    fn source(&self) -> ActionSource {
        self.source
    }

    /// 返回动作名称。
    fn name(&self) -> &str {
        &self.name
    }
}
