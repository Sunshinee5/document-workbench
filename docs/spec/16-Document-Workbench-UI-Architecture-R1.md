# 规范 16 · Document Workbench UI Architecture R1

Version：R1.0
Compatible with: Architecture Freeze **R1.1**
Scope：Desktop UI / Workspace / Interaction
Status：Architecture Freeze（UI 结构冻结；技术栈升版未冻结）
Priority：High
Depends：`00` Architecture Freeze / `04` Stage Roadmap / `15` Integration & Regression

> 本章冻结全项目唯一的 UI 架构。以后所有功能不得自行增加窗口、独立 Toolbar 或第二套交互，只能新增 Workspace、Ribbon 页签与 Inspector 面板。

---

## 一、目标与定位

建立唯一 UI 架构，统一 Desktop / Workspace / Ribbon / Sidebar / Canvas / Inspector / StatusBar / AI Workspace。

产品定位：**Professional Document Workspace**，不是 PDF Viewer / Reader。参考 Adobe Acrobat Pro、VS Code、Figma、Cursor 的**工作台形态**。

## 二、固定布局（生命周期内永久保持）

```
┌────────────────────────────────────────────────────────────┐
│ TitleBar                                                   │
├────────────────────────────────────────────────────────────┤
│ Ribbon                                                     │
├──────┬──────────────────────────────┬──────────────────────┤
│      │                              │                      │
│ Left │          PDF Canvas          │     Inspector        │
│Panel │                              │                      │
├──────┴──────────────────────────────┴──────────────────────┤
│ StatusBar                                                  │
└────────────────────────────────────────────────────────────┘
```

多文档页签作为 **Workspace 顶部文档上下文区**存在，不算第二套 UI（§十九 C-4）。

## 三、Workspace

固定分类：Pages / Bookmarks / Outline / Objects / OCR / Annotation / AI / Batch / Attachments / Layers（Future）。

每新增能力 → **新增 Workspace**；禁止新增 Dock Window。

## 四、Ribbon

固定页签：Home / Edit / OCR / Annotation / AI / View。禁止 Toolbar 无限增长。

- Home：Open / Save / Save As / Undo / Redo / Print / Export
- Edit：Browse / Select / Edit Text / Replace Image / Crop / Delete / Rotate / OCR
- OCR：Recognize / Merge / Correct / Export
- Annotation：Highlight / Underline / Comment / Signature
- AI：Summarize / Translate / Rewrite / Extract / Chat
- View：Zoom / Rotate / Split / Fullscreen / Theme

未实现的能力**显示禁用态**，不得隐藏、不得弹"敬请期待"浮窗。

## 五、Left Sidebar

Workspace Navigation：📄 Pages / 🔖 Bookmarks / 📚 Outline / 🧩 Objects / 🔍 OCR / 💬 Annotation / 🤖 AI / 📎 Attachments。支持折叠与拖拽宽度。

## 六、Canvas 与 Overlay Stack

Canvas 固定 **PDF.js**，禁止重新实现渲染器。Canvas 只负责 Rendering；全部交互在 Overlay。

Overlay 顺序固定：

```
PDF → Text → Selection → Editing → Annotation → OCR → AI → Debug
```

禁止直接修改 PDF Render。

## 七、Selection / Editing 交互

- Hover → Hover Highlight
- 点击 → Selection
- 双击 → BeginEdit
- Edit Mode：IBeam + Caret，支持 BeginEdit / Commit / Cancel，统一 `EditingSession`

## 八、Inspector

右侧固定，禁止按功能弹窗。分区：Document（Metadata / Page Count / Author / Size / Permissions）、Text（Font / Size / Color / Unicode / ObjectId / Rotation）、Image（Width / Height / Compression / Replace / Crop）、Annotation（Author / Reply / Opacity / Color）、AI（Prompt / History / Result / Context）。

## 九、StatusBar

固定：Zoom / Page / Selection / Cursor / Dirty / OCR / AI / Memory / Parser / Save。示例：`220% | Page 3/15 | Selection(Text) | Dirty | OCR Ready | Parser OK`。

## 十、鼠标模式

Browse / Select / Edit / OCR / Annotation / AI。Cursor 由统一 Mode 切换，禁止模块自行改 Cursor。

## 十一、对象树 / 后续 Workspace

Objects Workspace 显示 `Page → Text / Image / Path / Unknown`，支持点击定位、Hover 联动、Selection 联动。

> 落地口径（C-11）：当前对象级 Selection 尚无 IPC 桥接，Objects Workspace 只做**只读聚焦**（focus）——高亮与 Inspector 数据来自对象树投影，属 UI 导航状态，不是 Selection 真值；`SelectionState` 仍唯一在 Session。待 18 章选择 / 编辑 IPC 落地后改为真联动。

OCR（19）、Annotation（20）、AI（21）Workspace 各按用户文档字段接入 Inspector；AI 禁止浮窗，统一 Workspace。

## 十二、主题 / 组件 / 响应式

- 主题：Light / Dark / System；颜色统一 **Design Token（CSS Variables）**，禁止硬编码颜色。
- 组件：Button / Dropdown / Menu / Ribbon / Panel / Inspector / Tree / PropertyGrid / Dialog / Toast，统一一套实现，禁止重复实现。
- 响应式：1920 / 1600 / 1440 / 1280；低宽度自动折叠 Sidebar。

## 十三、UI 生命周期

| 章节 | UI 更新 |
|---|---|
| 17 Save | Ribbon Save State、StatusBar Dirty/Save |
| 18 Object Editing | Edit Ribbon、Objects Workspace、Selection Overlay、Inspector(Text/Image) |
| 19 OCR | OCR Workspace、OCR Overlay、OCR Inspector |
| 20 Annotation | Annotation Workspace、Comment Panel、Annotation Overlay |
| 21 AI | AI Workspace、AI Chat、Prompt Inspector |
| 22 Batch | Batch Workspace、Task Queue |
| 23 Plugin | Plugin Marketplace、Plugin Panel |

**禁止功能完成而 UI 没有入口。**

## 十四、Architecture Freeze

永久采用 `TitleBar → Ribbon → Workspace → Canvas → Inspector → StatusBar`；禁止新增第二套 UI、禁止功能绕过 Workspace。

## 十五、Rust / TS Mapping

| 概念 | 文件 | 状态 |
|---|---|---|
| Design Token / 主题 | `ui/src/styles/tokens.css` / `ui/src/theme.ts` | 本章新增 |
| 统一组件 | `ui/src/components/ui/*` | 本章新增 |
| TitleBar / Ribbon / StatusBar | `ui/src/components/shell/*` | 本章新增 |
| Workspace Navigation | `ui/src/components/workspace/*` | 本章新增 |
| Overlay Stack | `ui/src/components/overlay/*` | 本章新增 |
| Inspector | `ui/src/components/inspector/*` | 本章新增 |
| 对象树投影（解析侧） | `crates/dw-pdf/src/object_tree_view.rs` | 本章新增（只读） |
| 对象树投影（IPC） | `apps/dw-desktop/src/ipc.rs` / `ui/src/ipc/index.ts` | 本章新增（只读） |

---

## 十六、冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | §十三技术栈建议 **React 19 + Zustand + TanStack Query + shadcn/ui + Radix + Lucide + cmdk + Motion + Resizable Panels + TanStack Virtual + Tailwind**，与项目冻结技术栈 **React 18 + TS 5 + Vite 5** 冲突，且 `00` 冻结「改技术路线需 R2.0」 | **UI 结构冻结采纳，技术栈升版不采纳**。用户原文用"建议"且 Architecture Freeze 条款只冻结布局与组件职责。本次用 React 18 + Vite 5 + CSS Variables + 自建统一组件实现全部结构，不引入新依赖。React 19 / Tailwind / shadcn 等如需采用，须用户明确拍板 R2.0 |
| C-2 | 用户称"文档16"，但 `spec/15` 已是第 16 份规范 | 落为 `spec/16-Document-Workbench-UI-Architecture-R1.md`，规范总数 16 → 17 |
| C-3 | §十五追溯要求 17/18 章 UI 同步，但 17/18 已完成且 UI 只有 File 菜单与扁平工具栏 | 本次补做：Ribbon Save State、StatusBar Dirty/Save、Edit Ribbon、Objects Workspace、Selection Overlay、Inspector(Text/Image)。属**规范生效**，不是新增编辑能力 |
| C-4 | 固定布局图没有多文档页签，但 Workspace 必须支持多文档 | 页签作为 **Workspace 顶部文档上下文条**，归属 Workspace 区域；不算"第二套 UI" |
| C-5 | §十一组件规范要求 shadcn/ui + Radix，但 C-1 不引入新依赖 | 在 `ui/src/components/ui/` 自建**唯一一套**基础组件（Button / Panel / Tree / PropertyGrid / Toast 等），语义与文档对齐；禁止各功能重复实现 |
| C-6 | §十三图标建议 Lucide，但未安装 | 统一 `Icon` 组件用文本符号表达，集中管理；不逐模块散落图标字符 |
| C-7 | §八 Overlay Stack 与 PDF.js 自带 Text Layer 的关系 | PDF.js 仍只渲染；Overlay 是一组**独立 DOM 层**叠在 Canvas 容器上，Text Layer 为最底层只读层，交互全在上层，不改 PDF Render |
| C-8 | Inspector 要显示对象字段，但后端 IPC 未暴露对象树 | 新增只读 `get_object_tree` IPC：从 Document 源文件解析并投影为 JSON 摘要（page / id / kind / bbox / font / unicode）。**不新增编辑能力**，只读 |
| C-9 | §十五「禁止功能完成 UI 没入口」与 18 章已完成但无对象编辑入口 | 本次提供 Edit Ribbon + Objects Workspace + Inspector 编辑入口；实际编辑命令执行沿用已冻结的 18 链，UI 只转发 |
| C-10 | Light/Dark/System 主题需记住用户选择，但不能新增 Store / 改 Session | 主题是**纯 UI 偏好**，存 `localStorage`，不进 Session / Workspace / Document，不产生 Command / Diff |
| C-11 | §十一要求 Objects Workspace「Selection 联动」，但 18-05 `SelectionState` 尚无 IPC 桥接；UI 若自持对象选择即出现第二真值源 | **本次只做只读聚焦（focus）**：对象高亮与 Inspector 数据来自对象树投影，属 UI 导航状态，不写 Selection。`SelectionState` 仍唯一在 Session；待 18 章选择 / 编辑 IPC 落地后改为真联动 |

## 十七、验收标准

必须：✅ 固定六区布局 ✅ Workspace 分类 ✅ Ribbon 六页签 ✅ Overlay 顺序 ✅ Inspector ✅ StatusBar ✅ Design Token ✅ 响应式 ✅ 17/18 UI 入口。
不得：❌ 新增独立窗口 ❌ 新增独立 Toolbar ❌ 第二套交互 ❌ 硬编码颜色 ❌ 技术栈升版。
