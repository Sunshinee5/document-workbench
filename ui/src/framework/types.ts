// 18B Framework 扩展点类型（规范 18 §二 / §三）。
//
// 冻结约束（AD-09 / AD-19 / AD-20）：
// - 注册表只存**声明**（id / label / order / 谓词 / 渲染函数），不存业务真值。
// - InspectorInfo / ObjectNodeInfo / EditingSession 永远不在注册表里。
// - 所有扩展点经 registerFeature 注册；组件只遍历注册表，禁止 switch 分派。
import type { ReactNode } from 'react';
import type { IconName } from '../components/ui/Icon';
import type { ThemeMode } from '../theme';
import type {
  CapabilityInfo,
  HitInfo,
  InspectorInfo,
  SelectionInfo,
} from '../ipc';

// ---------------------------------------------------------------------------
// 通用：框架上下文（全部是投影，来源 EditorState / ViewState / 状态栏投影）
// ---------------------------------------------------------------------------

/** 注册表谓词可见的唯一上下文：只读投影，无写方法、无真值对象。 */
export interface FrameworkContext {
  /** 当前工具 id（来自 ToolRegistry 的注册项）。 */
  mode: string;
  /** 当前主题模式（来自 localStorage 偏好，纯 UI 状态）。 */
  theme: ThemeMode;
  hasDocument: boolean;
  page: number;
  pageCount: number;
  zoomPercent: number;
  capability: CapabilityInfo;
  selection: SelectionInfo;
  editing: { active: boolean; objectId: string | null };
  hover: HitInfo | null;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  pendingPatches: number;
  revision: string;
  parserText: string;
  saveText: string;
  taskEvent: string;
  error: string;
  /** 18C：当前工具 label（来自 ToolRegistry）。 */
  toolLabel: string;
  /** 18C：最近命中优先级投影（来自 HitTest 反馈）。 */
  hitPriority: string;
  /** 18C：选择校验摘要（来自 SelectionInfo 一致性只读投影）。 */
  selectionValidation: string;
  /** 18C：保存态反馈（来自 EventBridge，不是 DocumentSaveState 真值）。 */
  saveFeedback: 'idle' | 'saving' | 'success' | 'failed';
  /** 18C：保存反馈描述（含增量标识）。 */
  saveMessage: string;
  /** 18C：最近事件中文描述。 */
  lastEvent: string;
  /** 第19章：OCR 会话状态文案。 */
  ocrStateLabel: string;
  /** 第19章：OCR 进度百分比。 */
  ocrProgress: number;
  /** 第19章：OCR 结果数量。 */
  ocrResultCount: number;
  /** 第19章：是否有 OCR 引擎可用。 */
  ocrAvailable: boolean;
  /** 第19章：首个结果是否已接受且可写回文本对象。 */
  ocrApplyable: boolean;
  /** 第19章投影：当前 OCR Provider 名（没有数据时由 Inspector 显示「—」）。 */
  ocrProvider?: string;
  /** 第19章投影：首条 OCR 结果的置信度 0..1（没有数据时显示「—」）。 */
  ocrConfidence?: number | null;
  /** 第19章投影：当前 OCR 结果序号（Overlay 与 Workspace 共用，只读）。 */
  ocrActiveIndex?: number | null;
  /** 第19章投影：当前 OCR 结果文本（Inspector 展示用）。 */
  ocrActiveText?: string;
  /** 第21章占位投影：Search 状态文案；未接入时由 StatusBar 显示「搜索未就绪」。 */
  searchStateLabel?: string;
}

/** App 注入的动作集合：注册表只声明意图，不持有 ViewModel / IPC。 */
export interface FrameworkActions {
  open(): void;
  /** Ctrl+F：打开 / 聚焦 Search Workspace（21 章占位入口，任何状态可用）。 */
  openGlobalSearch?(): void;
  /** Dashboard 拖入 / 最近文件：按已知本地路径打开，仍走唯一 Open 链路。 */
  openPath?(path: string): void;
  save(): void;
  saveAs(): void;
  undo(): void;
  redo(): void;
  rotate(delta: number): void;
  deletePages(): void;
  zoomIn(): void;
  zoomOut(): void;
  fitWidth(): void;
  actualSize(): void;
  fullscreen(): void;
  goToPage(page: number): void;
  nextPage(): void;
  prevPage(): void;
  setMode(toolId: string): void;
  setTheme(theme: ThemeMode): void;
  beginEdit(): void;
  commit(): void;
  cancel(): void;
  replaceText(text: string): void;
  selectAll(): void;
  clearSelection(): void;
  /** 第19章：OCR 动作（由 App 注入 OcrController）。 */
  ocrRecognize(scope: 'full' | 'pages' | 'region'): void;
  ocrPause(): void;
  ocrResume(): void;
  ocrCancel(): void;
  ocrApply(index: number): void;
  /** 应用当前激活的 OCR 结果（Overlay / Workspace 共用的只读序号）。 */
  ocrApplyActive?(): void;
  /** Ctrl+F：OCR 结果内检索（全文 Search 属 21 章）。 */
  searchInOcr(): void;
  /** 复制首个 OCR 结果文本（不属对象剪贴板编辑）。 */
  ocrCopyResult(): void;
}

/** 能力需求名：Ribbon 按钮启用只由这些命名谓词决定（AD-13）。 */
export const NEEDS = {
  always: () => true,
  /** 未接入能力统一禁用：显示入口但永远不可点，由 reason 说明归属章节。 */
  never: () => false,
  document: (ctx: FrameworkContext) => ctx.hasDocument,
  undo: (ctx: FrameworkContext) => ctx.hasDocument && ctx.canUndo,
  redo: (ctx: FrameworkContext) => ctx.hasDocument && ctx.canRedo,
  selection: (ctx: FrameworkContext) => ctx.selection.primary !== null,
  text: (ctx: FrameworkContext) => ctx.capability.can_edit_text,
  image: (ctx: FrameworkContext) => ctx.capability.can_edit_image,
  path: (ctx: FrameworkContext) => ctx.capability.can_edit_path,
  editing: (ctx: FrameworkContext) => ctx.editing.active,
  multiPage: (ctx: FrameworkContext) => ctx.pageCount > 1,
  ocrAvailable: (ctx: FrameworkContext) => ctx.ocrAvailable,
  ocrResults: (ctx: FrameworkContext) => ctx.ocrResultCount > 0,
  ocrApplyable: (ctx: FrameworkContext) => ctx.ocrApplyable,
  ocrRunning: (ctx: FrameworkContext) => ctx.ocrStateLabel.includes('识别中'),
  ocrPaused: (ctx: FrameworkContext) => ctx.ocrStateLabel.includes('已暂停'),
} satisfies Record<string, (ctx: FrameworkContext) => boolean>;

export type NeedName = keyof typeof NEEDS;

// ---------------------------------------------------------------------------
// 1. Tool Framework（AD-11）
// ---------------------------------------------------------------------------

/** 光标种类（AD-14）：与 CSS `data-cursor` 值一一对应。 */
export type CursorKind =
  | 'arrow'
  | 'cross'
  | 'ibeam'
  | 'resize'
  | 'move'
  | 'rotate'
  | 'ocr'
  | 'annotation'
  | 'ai'
  /** 第19章：OCR Running 全局忙态。 */
  | 'busy'
  /** 第19章：无能力 / 禁用工具的统一禁止态。 */
  | 'forbidden';

/** 一个可激活工具。未接入能力也注册，只是 enabled=false。 */
export interface ToolContribution {
  id: string;
  label: string;
  icon: IconName;
  order: number;
  /** 该工具激活时的光标种类；唯一来源，Canvas 不得自行设置。 */
  cursor: CursorKind;
  enabled: (ctx: FrameworkContext) => boolean;
  /** 禁用文案（写清归属章节），不隐藏入口。 */
  reason?: string;
}

// ---------------------------------------------------------------------------
// 2. Overlay Framework（AD-10）
// ---------------------------------------------------------------------------

/** 覆盖层顺序（16 章 §六冻结）；注册项按 layer 的 order 排序。 */
export interface OverlayLayerDef {
  id: string;
  order: number;
  /** 层容器额外 class（如编辑态 is-active），由注册项声明，框架不特判。 */
  className?: (props: OverlayRenderProps) => string;
}

/** 传给 Overlay 渲染函数的只读几何投影。 */
export interface OverlayRenderProps {
  pageWidth: number;
  pageHeight: number;
  rotation: number;
  objects: OverlayObject[];
  hoverObjectId: string | null;
  selectedObjectIds: string[];
  editingObjectId: string | null;
  /** 18C：编辑对象当前文本（投影，草稿仍由 Inline Editor 本地持有）。 */
  editingText?: string;
  /** 18C：Inline Editor 提交（走 editor_commit）。 */
  onCommitText?: (text: string) => void;
  /** 18C：Inline Editor 取消（走 editor_cancel）。 */
  onCancelEdit?: () => void;
  /** 18C：命中优先级提示（hover 时显示）。 */
  hoverPriority?: string;
  /** 18C：Hover 对象种类中文名。 */
  hoverKind?: string;
  /** 18C：Primary 选中对象 id（单独高亮）。 */
  primaryObjectId?: string | null;
  /** 第19章：OCR 结果投影（Overlay 画 BBox + confidence）。 */
  ocrResults?: Array<{
    /** 全局 OCR 结果序号（App 投影时写入），不是页内局部序号。 */
    resultIndex: number;
    page: number;
    bbox: [number, number, number, number];
    confidence: number;
    text: string;
  }>;
  /** 18 章：当前工具 id，用于 Overlay 手柄的 Hover 光标策略。 */
  toolMode?: string;
  /** 18 章：Overlay 手柄 Hover 时覆盖 Viewer 的 data-cursor（离开传 null 复原）。 */
  onCursorOverride?: (cursor: string | null) => void;
  /** 18 章：非模态轻提示（Inline Editor 失焦提示）；由 App 注入唯一 ToastStack。 */
  notify?: (text: string) => void;
  /** 第19章：Review 决策只读投影（result_index → decision）。 */
  ocrReviews?: Array<{ result_index: number; decision: string }>;
  /** 第19章：当前激活结果序号；Overlay 与 Workspace 共用同一只读投影。 */
  ocrActiveIndex?: number | null;
  /** 第19章：逐结果是否可 Apply（已接受且有 source_object_id）。 */
  ocrCanApply?: boolean[];
  /** 第19章：点击 OCR 框设为当前结果。 */
  onOcrActivate?: (index: number) => void;
  /** 第19章：Overlay 轻操作条接受 / 拒绝，转发 OcrController.decide。 */
  onOcrDecide?: (index: number, accept: boolean) => void;
  /** 第19章：Overlay 轻操作条应用，转发 OcrController.apply → Editing Engine。 */
  onOcrApply?: (index: number) => void;
}

/** 对象几何投影（不是 Object 真值）；由对象树 IPC 快照投影得到。 */
export interface OverlayObject {
  /** u64 对象标识的字符串形式，与 hit/select/inspector 全线同类型。 */
  objectId: string;
  bbox: [number, number, number, number];
  /** 对象种类英文枚举名（Text / Image / Path），供编辑态校验对象类型。 */
  kind: string;
}

/** 一个覆盖层贡献：layer id 决定层级，render 决定内容。 */
export interface OverlayContribution {
  id: string;
  /** 所属层：必须是已注册 layer id。 */
  layer: string;
  order: number;
  enabled?: (ctx: FrameworkContext) => boolean;
  render: (props: OverlayRenderProps) => ReactNode | null;
}

// ---------------------------------------------------------------------------
// 3. Inspector Framework（AD-12）
// ---------------------------------------------------------------------------

/** Inspector 文档级投影（由 ViewModel 快照投影，不是 Document 真值）。 */
export interface InspectorDocumentInfo {
  title: string;
  author: string;
  pageCount: number;
  fileSize: number;
  path: string;
  dirty: boolean;
  lastSavedAt: string;
  parserText: string;
}

/** 传给 Inspector 分区渲染函数的只读数据。 */
export interface InspectorRenderProps {
  ctx: FrameworkContext;
  document: InspectorDocumentInfo;
  inspector: InspectorInfo | null;
  focusedPageRotation: number | null;
  actions: FrameworkActions;
  formatSize: (bytes: number) => string;
}

/** 一个 Inspector 分区：matches 决定是否显示，render 负责内容。 */
export interface InspectorContribution {
  id: string;
  title: string;
  order: number;
  matches: (ctx: FrameworkContext) => boolean;
  render: (props: InspectorRenderProps) => ReactNode;
}

// ---------------------------------------------------------------------------
// 4. Ribbon Framework（AD-13）
// ---------------------------------------------------------------------------

/** 六个页签固定（16 章 §四），仍由注册表提供顺序。 */
export interface RibbonTabContribution {
  id: string;
  label: string;
  order: number;
}

/** 页签内分组。 */
export interface RibbonGroupContribution {
  id: string;
  tab: string;
  label: string;
  order: number;
}

/** 一条命令：启用只由 needs 谓词决定，active 表示切换态。 */
export interface RibbonCommandContribution {
  id: string;
  tab: string;
  group: string;
  label: string;
  icon: IconName;
  order: number;
  variant?: 'primary' | 'danger';
  needs: NeedName[];
  active?: (ctx: FrameworkContext) => boolean;
  /** 禁用原因；缺省时由 needs 拼装。 */
  reason?: string;
  run: (actions: FrameworkActions) => void;
}

// ---------------------------------------------------------------------------
// 5. Context Menu Framework（AD-15）
// ---------------------------------------------------------------------------

/** 菜单项：来自 Provider，组件只遍历，不写死。 */
export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: IconName;
  disabled?: boolean;
  title?: string;
  separatorBefore?: boolean;
  run: () => void;
}

/** 一个右键菜单 Provider。 */
export interface ContextMenuProvider {
  id: string;
  order: number;
  items: (ctx: FrameworkContext, actions: FrameworkActions) => ContextMenuItem[];
}

// ---------------------------------------------------------------------------
// 6. Cursor Framework（AD-14）
// ---------------------------------------------------------------------------

/** 工具 → 光标规则；resolve(toolId) 是唯一出口。 */
export interface CursorRule {
  /** id 即 toolId。 */
  id: string;
  cursor: CursorKind;
}

// ---------------------------------------------------------------------------
// 7. Shortcut Framework（AD-20）
// ---------------------------------------------------------------------------

/** 一条快捷键绑定：combo 形如 `ctrl+shift+s` / `f2` / `escape`。 */
export interface ShortcutBinding {
  id: string;
  combo: string;
  order: number;
  enabled?: (ctx: FrameworkContext) => boolean;
  run: (actions: FrameworkActions) => void;
}

// ---------------------------------------------------------------------------
// 8. Theme Framework（AD-16）
// ---------------------------------------------------------------------------

/** Token 名册：值仍在 tokens.css 的 CSS Variables 里，这里只登记名字。 */
export interface ThemeToken {
  id: string;
  /** CSS 变量名，如 `--dw-accent`。 */
  cssVar: string;
  /** 用途说明，禁止在组件里写死色值。 */
  usage: string;
}

// ---------------------------------------------------------------------------
// 9. Dock Framework（AD-18，只冻结结构）
// ---------------------------------------------------------------------------

/** 六个固定槽位；不实现浮动，不实现布局保存。 */
export type DockSlot = 'titlebar' | 'ribbon' | 'navigator' | 'canvas' | 'inspector' | 'statusbar';

/** 一个槽位里的窗格声明。 */
export interface DockPane {
  id: string;
  slot: DockSlot;
  order: number;
  label: string;
  enabled?: (ctx: FrameworkContext) => boolean;
}

// ---------------------------------------------------------------------------
// 10. Status Framework（AD-17）
// ---------------------------------------------------------------------------

/** 自定义状态项渲染（页码输入等交互项），只收投影与动作。 */
export type StatusRender = (ctx: FrameworkContext, actions: FrameworkActions) => ReactNode;

/** 状态栏分组：左 / 中 / 右；缺省归入左组（兼容既有注册项）。 */
export type StatusGroup = 'left' | 'center' | 'right';

/** 一个状态栏项：value 或 render 二选一。 */
export interface StatusItem {
  id: string;
  label: string;
  order: number;
  /** 所属分组；未声明时按左组处理，不改变既有注册表语义。 */
  group?: StatusGroup;
  value?: (ctx: FrameworkContext) => string;
  tone?: (ctx: FrameworkContext) => 'danger' | 'warn' | 'ok' | undefined;
  render?: StatusRender;
}

// ---------------------------------------------------------------------------
// Feature：一次注册所有扩展点（AD-20 唯一入口）
// ---------------------------------------------------------------------------

/** 一个能力包；OCR / Annotation / AI / Plugin 以后只写这个对象。 */
export interface DesktopFeature {
  id: string;
  tools?: ToolContribution[];
  overlayLayers?: OverlayLayerDef[];
  overlays?: OverlayContribution[];
  inspectors?: InspectorContribution[];
  ribbonTabs?: RibbonTabContribution[];
  ribbonGroups?: RibbonGroupContribution[];
  ribbonCommands?: RibbonCommandContribution[];
  contextMenu?: ContextMenuProvider[];
  cursors?: CursorRule[];
  shortcuts?: ShortcutBinding[];
  themeTokens?: ThemeToken[];
  dockPanes?: DockPane[];
  statusItems?: StatusItem[];
}
