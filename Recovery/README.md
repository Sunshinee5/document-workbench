# 会话恢复目录（占位）

骨架阶段无真实恢复数据。后续结构约定：

- 多 Session 多 Recovery：每个会话一个独立子目录，例如 `Recovery/<session-id>/`。
- 每个子目录内保存 `session.json`（会话状态）与 Checkpoint 文件。
- Checkpoint 优先落盘，内存仅保留最近一个；每 100 条 Command 生成一次。
- 恢复 = Checkpoint + 后续 Diff；不保存 Task / Driver / Runtime 对象。
- 关闭会话时清理对应临时数据。
