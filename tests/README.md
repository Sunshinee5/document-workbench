# 测试目录（占位）

骨架级测试目前放在核心库内部（`crates/dw-core/src/lib.rs` 的 `#[cfg(test)]` 模块），
验证冻结类型可构造、Checkpoint 间隔常量正确。

后续补充：

- 单元层：Command 撤销/重做、Capability Gate 校验、ExecutionGraph 依赖分析。
- 集成层：UI → ViewModel → Action → Command → Task → Driver 全链路。
- 不在此目录引入 Qt / Java / pdf-lib 等被禁技术栈。
