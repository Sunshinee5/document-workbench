# 18C《Desktop Feature Completion》实施记录

规范：`docs/spec/19-Desktop-Feature-Completion-R1.md`（第 20 份，别名 18C）
时间：2026-11-26　结论：**Rust Core 已冻结能力全部具备 UI 入口与反馈，能力映射表全部落点。**

## 一、定位

18C 不新增任何 Rust Core 能力、不改 18 章冻结数据流、不引入第二套业务逻辑。
本章把 Core 已有能力完整暴露到 Desktop，并建立能力映射表作为长期验收基线。

## 二、关键冲突裁决（C-1）

用户要求「EventBus → Desktop 全量订阅」，但 17 章 C-7 冻结「不额外 emit Tauri 事件，避免事件与命令竞态」。
裁决为**双通道分离**：
- 真值通道：命令返回 `EditorSnapshot`（保持 C-7 不变）；
- 反馈通道：新增只读 `core_events(cursor)` 增量拉取事件，只驱动 UI 反馈，不写回 EditorState、不产生 Command/Diff。
游标只增不减、拉取幂等；不新增 emit/listen。

## 三、落点

| 职责 | 文件 |
|---|---|
| `core_events(cursor)` / `core_status` 只读命令 | `apps/dw-desktop/src/ipc.rs` / `lib.rs` |
| 事件反馈桥（游标拉取 + Saving/Success/Failed 翻转） | `ui/src/editor/eventBridge.ts`（新增） |
| IPC 前端封装 | `ui/src/ipc/index.ts` |
| FrameworkContext 反馈字段 | `ui/src/framework/types.ts` |
| Inline Text Editor / Primary 高亮 / 命中优先级 / 对象分区 / 状态栏反馈 | `ui/src/framework/builtins.tsx` |
| 对象树 → Overlay 交互数据透传 | `ui/src/components/PdfViewer.tsx` / `PageOverlays.tsx` |
| 事件 poll 时机 + Save 反馈提示 | `ui/src/App.tsx` |
| Primary / Hover Tag / Inline Editor 样式 | `ui/src/styles/components.css` |

顺手修掉 `content_stream.rs` 一个存量 `unused_mut` 警告（不改行为）。

## 四、能力映射表

见 `docs/spec/19` §三 与 `18C-Desktop-Feature-Completion-测试报告.md`。
18 项 Core 能力逐项对应 UI 落点，全部 ✅。

## 五、验证

见测试报告。摘要：TS strict PASS；vite build PASS；Rust 全量测试全绿；
能力映射关键标识静态核对全部命中。

## 六、未完成（Out of Scope）

OCR / Annotation / AI / Plugin Runtime / Font Embed / 图片替换裁剪 / Path 编辑 / 云同步 / 协同编辑；
Resize / Rotate 光标已注册但手柄拖动不实现（与 18A C-9 一致）。
