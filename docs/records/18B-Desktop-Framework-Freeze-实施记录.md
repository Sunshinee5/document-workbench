# 18B《Desktop Framework Freeze》实施记录

规范：`docs/spec/18-Desktop-Framework-Freeze-R1.md`（第 19 份，别名 18B）
时间：2026-11-26　结论：**Desktop 扩展方式冻结为注册制，18A 行为不退化。**

## 一、定位

18A 解决「能不能用」，18B 解决「以后还要不要重构」。
本章不新增编辑业务，只把已有能力的组织方式改为注册表驱动：
新增 OCR / Annotation / AI / Plugin 以后只写 feature 对象并 `registerFeature()`，
不需要修改 App / Ribbon / Inspector / Overlay / StatusBar 任何框架代码。

## 二、落点

| 职责 | 文件 |
|---|---|
| 注册表内核（原子注册 + 排序 + needs 谓词 + 光标解析） | `ui/src/framework/core.ts` |
| 十类扩展点类型 | `ui/src/framework/types.ts` |
| 18A 能力迁入（builtin feature） | `ui/src/framework/builtins.tsx` |
| 聚合出口 / 唯一注册入口 | `ui/src/framework/index.ts` |
| ThemeManager（token 名册） | `ui/src/theme.ts`（扩展） |
| Ribbon / StatusBar / Inspector | `ui/src/components/{shell,inspector}/*`（改为遍历注册表） |
| Overlay Stack | `ui/src/components/overlay/PageOverlays.tsx`（改为遍历注册表） |
| 右键菜单 | `ui/src/components/ui/ContextMenu.tsx`（新增）+ `PdfViewer.tsx` |
| 快捷键 / 动作注入 / 框架上下文 | `ui/src/App.tsx` |

Rust Core 与 IPC **零改动**——18B 是纯前端框架层收敛。

## 三、AD-09 ~ AD-20 落地

- **AD-09 / AD-19**：注册表只存声明（id / label / order / 谓词 / 渲染函数）；
  `InspectorInfo` / `ObjectNodeInfo` / `EditingSession` 不进注册表，投影仍来自 EditorState 快照。
- **AD-10**：Overlay 层与内容都来自 `OverlayRegistry`；Canvas 只保留 PDF.js 渲染。
- **AD-11**：工具 id = 模式值；未接入工具注册为禁用态，入口不隐藏。
- **AD-12**：Inspector 分区来自 `InspectorRegistry`，新增分区不改 Inspector 组件。
- **AD-13**：Ribbon 命令 `needs: NeedName[]`，统一由 `NEEDS` 命名谓词求值。
- **AD-14**：`CursorManager.resolve(toolId)` → `data-cursor` → CSS；组件无 `style.cursor`。
- **AD-15**：右键菜单来自 Provider，App 只调 `contextItems()`，无 switch。
- **AD-16**：ThemeManager 登记 token 名册；色值仍在 `tokens.css`。
- **AD-17**：状态栏项来自 `StatusBarRegistry`，仍只有一个状态栏。
- **AD-18**：六槽位结构冻结；不实现浮动、不实现 Dock Save。
- **AD-20**：`registerFeature()` 是唯一入口，原子注册，重复 id 整体回滚。

## 四、冲突裁决

- **C-1 / C-2**：编号 18（18A 占 17），AD 接续 AD-09~20。
- **C-3**：AD-09 禁的是业务真值与实现，不是只读投影（与 17 章 C-3/C-4 一致）。
- **C-4**：Dock 只冻结槽位；Search / AI 继续走 Workspace 分类。
- **C-5**：状态栏 builtin 保留 16 章十项 + Undo / Redo，仍一个状态栏。
- **C-6**：17 章 Mapping 里写了但未落地的 `shortcuts.ts` 与右键菜单，本次用 Framework 补齐。
- **C-7**：不重写 CSS 色值，ThemeManager 只登记 token 名。
- **C-8**：用「假 OCR feature」验证扩展点，不实现 OCR 识别。

## 五、验证

见 `18B-Desktop-Framework-Freeze-测试报告.md`。摘要：
TS `tsc --noEmit` PASS；`vite build` PASS；注册表自检 10/10 PASS；
`cargo test -p dw-core -p dw-pdf -j 1`（GNU 工具链）全绿。

## 六、未完成（明确列出，不假装支持）

与 18A 相同：Ctrl+C/V、Delete、图片替换/裁剪、Path 描边、Resize/Rotate 手柄拖动、
Text.Color / Image.Compression / Path.LineWidth 采集。本章不实现 OCR / AI / Annotation / Search / Plugin。
