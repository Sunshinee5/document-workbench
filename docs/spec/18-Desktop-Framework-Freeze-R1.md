# 规范 18 · Desktop Framework Freeze R1

Version：R1.0（用户原文别名 **18B**）
Compatible with: Architecture Freeze **R1.1**
Scope：Desktop UI 框架 / 扩展点冻结 / 注册表协议
Status：Architecture Freeze
Priority：High
Depends：`00` / `16` UI Architecture / `17` Desktop Integration（18A）

> 18A 已打通 Rust ↔ Desktop 桥接、EditorState 投影、Overlay、Inspector、Ribbon 联动。
> 18B **不重复**这些内容，也不新增任何编辑业务。唯一目标：把 Desktop 的扩展方式冻结为「注册」，
> 使后续 OCR / Annotation / AI / Plugin 只能建立在框架上，且**不需要修改框架代码**。

---

## 一、冻结后的 Desktop 结构

```
Desktop
├── UI Components          （规范 16 已冻结：Button / Panel / Tree / PropertyGrid / Toast / Icon）
├── Tool Framework
├── Overlay Framework
├── Inspector Framework
├── Ribbon Framework
├── Context Menu Framework
├── Cursor Framework
├── Shortcut Framework
├── Theme Framework
├── Dock Framework
└── Status Framework
```

新增 OCR = `registerFeature(ocrFeature)`，一次注册 Tool + Overlay + Inspector + Ribbon + ContextMenu。
**禁止**在 App / Ribbon / Inspector / PageOverlays / StatusBar 里新增 `switch` 或 `if (feature)` 分派。

---

## 二、AD-09 ~ AD-20 落地口径

| 编号 | 用户原文要点 | 落地口径 |
|---|---|---|
| AD-09 | Desktop 不得成为第二业务层；只负责 UI / 输入 / ViewState / IPC；禁止 PDF / Editing / Save / History / Diff / Object | 框架只存**声明式注册项与投影**。所有写操作仍经 `EditorController → IPC → Rust Core`；UI 不解析 PDF、不生成 Diff、不写 History、不持 Object 真值 |
| AD-10 | OverlayManager 唯一 Overlay；允许 Selection / Hover / Caret / BoundingBox；后续 OCR / Search / Annotation / AI 注册；禁止 Canvas 直接画 | `OverlayRegistry` 按 `layer + order` 渲染；Canvas 只保留 PDF.js 渲染，全部覆盖层由注册表贡献；现有四层注册为 builtin |
| AD-11 | ToolManager 唯一 Tool；Select / Text / Image / OCR / Annotation / AI 全部注册；禁止 Canvas 自己维护 Tool | `ToolRegistry` 统一激活；`mode` 仍是唯一 UI 交互态，但取值必须是注册项 id；未接入工具注册为禁用态（保留入口，不隐藏） |
| AD-12 | InspectorRegistry 唯一 Inspector；后续新增 Inspector 无需改 Inspector | `InspectorRegistry` 按 `order` 渲染分区；`matches(ctx)` 决定显示；Inspector 组件内无对象类型 `switch` |
| AD-13 | Ribbon 必须 Capability 驱动；`Replace Text → Need(Text) + Need(Selection) + Need(Editing)` | `RibbonRegistry` 每条命令声明 `needs: CapabilityNeed[]`，统一谓词求值启停；六个页签 ID / 顺序冻结不变 |
| AD-14 | CursorManager 统一 Arrow / Cross / IBeam / Resize / Move / Rotate / OCR / Annotation / AI；禁止模块 `setCursor()` | `CursorManager.resolve(toolId)` 唯一出口；Canvas 用 `data-cursor` 属性 + CSS 变量映射；组件不得写 `style.cursor` |
| AD-15 | ContextMenu 统一 Provider；插件注册；禁止 switch | `ContextMenuRegistry` 收集 Provider 的 items，按 `order` 合并；Canvas 右键只调框架，不写死菜单 |
| AD-16 | ThemeManager 统一 Theme Token；禁止组件写死颜色 | `ThemeManager` 管模式（Light / Dark / System）与 token 名册；样式继续用 `tokens.css` 的 CSS Variables，组件不出现色值字面量 |
| AD-17 | StatusBar 统一 Page / Zoom / Selection / Editing / Dirty / Undo / Redo；后续 OCR / AI 扩展 | `StatusBarRegistry` 按 `order` 渲染，仍只有一个状态栏；builtin 保留规范 16 十项并补 Undo / Redo |
| AD-18 | Dock 统一布局；建议支持 Navigator / Canvas / Inspector / Search / AI / Console；18B 不实现浮动、不实现 Dock Save，只冻结结构 | `DockManager` 冻结六个槽位（titlebar / ribbon / navigator / canvas / inspector / statusbar）与默认窗格归属；Search / AI 作为 Workspace 分类接入，不是浮动 Dock；本章无浮动、无布局序列化 |
| AD-19 | Desktop 不得缓存 Object / Selection / EditingSession，只允许 Projection | 重申 17 章 C-3 / C-4 / AD-01~08：注册表只存**声明**（id / label / order / 谓词），投影数据每次来自 `EditorState` 快照；不得把 `InspectorInfo` / `ObjectNodeInfo` 写进注册表 |
| AD-20 | 所有 Feature 必须注册；禁止新增 switch | 统一 `desktopFramework.registerFeature(feature)` 原子入口；十类扩展点全部注册表驱动；重复 id 注册报错，防止静默覆盖 |

---

## 三、Feature 注册协议（冻结）

```ts
interface DesktopFeature {
  id: string;
  tools?: DesktopTool[];
  overlays?: OverlayContribution[];
  inspectors?: InspectorContribution[];
  ribbon?: RibbonCommand[];
  contextMenu?: ContextMenuProvider[];
  cursors?: CursorRule[];
  shortcuts?: ShortcutBinding[];
  themes?: ThemeToken[];
  dockPanes?: DockPane[];
  statusItems?: StatusItem[];
}
```

- `builtin` feature 在框架初始化时注册（现有 18A 能力全部迁入，行为不变）。
- 注册是**原子**的：任一扩展点 id 冲突则整体回滚，避免半注册状态。
- 命令执行经 `FrameworkActions` 注入（由 App 提供），注册表本身不 import ViewModel / IPC，保持可测试。
- 新增能力的唯一动作：写一个 feature 对象并注册；框架文件零改动。

---

## 四、目录落点（Rust / TS Mapping）

| 概念 | 文件 | 状态 |
|---|---|---|
| 注册表内核 + `registerFeature` | `ui/src/framework/core.ts` | 本章新增 |
| 扩展点类型（十类） | `ui/src/framework/types.ts` | 本章新增 |
| builtin feature（现有能力迁入） | `ui/src/framework/builtins.tsx` | 本章新增 |
| 框架聚合出口 | `ui/src/framework/index.ts` | 本章新增 |
| Ribbon / Inspector / StatusBar 改为消费注册表 | `ui/src/components/{shell,inspector}/*` | 本章修改 |
| Overlay 改为消费注册表 | `ui/src/components/overlay/PageOverlays.tsx` | 本章修改 |
| Canvas 光标 / 右键改为消费注册表 | `ui/src/components/PdfViewer.tsx` | 本章修改 |
| 快捷键改为消费注册表 | `ui/src/App.tsx` | 本章修改 |
| ThemeManager | `ui/src/theme.ts` | 本章扩展 |
| Rust Core / IPC | 无改动 | 18B 是纯框架层收敛 |

**零新增依赖**：仍为 React 18.3.1 / TS 5.6.3 / Vite 5.4.11 / pdfjs-dist 6.5.438。

---

## 五、验收（DT-01 ~ DT-12）

| 编号 | 判据 |
|---|---|
| DT-01 | 十类扩展点全部由注册表提供，`registerFeature` 是唯一入口 |
| DT-02 | 重复 id 注册整体回滚，不产生半注册状态 |
| DT-03 | 移除 builtin 之外所有能力后，现有浏览 / 选择 / 文字编辑行为不变 |
| DT-04 | 新增一个假 OCR feature（只声明注册项），Tool / Overlay / Inspector / Ribbon / ContextMenu 全部出现，框架文件零改动 |
| DT-05 | Ribbon 按钮启用只由 `needs` 谓词决定，代码无对象类型判断 |
| DT-06 | Canvas 不含 `style.cursor`，光标只由 `CursorManager.resolve` 决定 |
| DT-07 | Inspector 分区来自注册表，新增分区不需要改 Inspector 组件 |
| DT-08 | Overlay 层来自注册表，Canvas 无直接绘制覆盖层 |
| DT-09 | 右键菜单来自 Provider，无 `switch` 分派 |
| DT-10 | 状态栏项来自注册表，仍只有一个状态栏 |
| DT-11 | 注册表不持有 `InspectorInfo` / `ObjectNodeInfo` / `EditingSession` 真值，只存声明与谓词 |
| DT-12 | `tsc --noEmit` + `vite build` + 核心 `cargo test` 全绿 |

---

## 六、Out of Scope（本章禁止实现）

OCR、AI、Annotation、Search、Plugin、Cloud、Collaboration、Font Embed、Image Replace、Path Editing、Clipboard，
以及 Dock 浮动 / Dock 布局保存。以上全部下一章；框架只提供注册位置。

---

## 七、冲突裁决

| # | 冲突 | 裁决 |
|---|---|---|
| C-1 | 用户称「18B」，`spec/17` 已给 18A | 落为 `spec/18-Desktop-Framework-Freeze-R1.md`（第 19 份），标题保留别名 18B |
| C-2 | AD 编号从 AD-09 开始，与 17 章 AD-01~08 是否重复 | 接续编号，不重编；本章 AD-09~20 共 12 条 |
| C-3 | AD-09 禁止 Desktop 负责 Object，但 17 章允许几何投影与 Inspector 投影 | 禁止的是**真值与业务实现**；只读投影仍允许（17 C-3 / C-4 不变） |
| C-4 | AD-18 建议 Dock 支持多面板，但 18B 明确不实现浮动 / Dock Save | 只冻结槽位与窗格归属；Search / AI 继续走 Workspace 分类（16 章 §三） |
| C-5 | AD-17 状态项清单（含 Undo / Redo）与 16 章十项不完全一致 | 用注册表统一：builtin 保留十六章十项 + 补 Undo / Redo；仍只有一个 StatusBar |
| C-6 | 17 章 Mapping 写了 `ui/src/editor/shortcuts.ts`，实际未落地；右键菜单也缺失 | 18B 用 Shortcut / ContextMenu Framework 补齐；属框架收口，不是新增业务 |
| C-7 | AD-16 要求 Token 统一，但现有颜色已写在 `tokens.css` | 保留 CSS Variables 作为唯一色值源；ThemeManager 只管模式与 token 名册，不重写样式文件 |
| C-8 | 验收要求「新增 OCR 只注册」但 OCR 本身 Out of Scope | 用「假 OCR feature」验证扩展点成立，不实现任何 OCR 识别逻辑 |

---

## 八、完成标准

- 十类 Framework 全部注册表驱动，App 内无 `switch` 分派具体业务能力。
- 新增能力 = 注册 feature；不需要修改框架代码。
- 18A 已有的浏览 / 对象选择 / 文字编辑 / 保存链路行为不变。
- Desktop 仍是 Rust Core 唯一 UI 桥接层，没有第二业务层。
