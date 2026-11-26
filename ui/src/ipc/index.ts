// Tauri 2 invoke 封装：直接走官方内部通道，不引入额外 npm 依赖。
interface TauriInternals {
  invoke?: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
}

type TauriWindow = Window & { __TAURI_INTERNALS__?: TauriInternals };

/** 调用后端命令；浏览器预览环境下抛出可识别错误，由 ViewModel 降级处理。 */
function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const internals = (window as TauriWindow).__TAURI_INTERNALS__;
  if (internals?.invoke) {
    return internals.invoke<T>(command, args);
  }
  return Promise.reject(new Error('当前不在桌面环境中'));
}

/** 应用信息：对应后端 AppInfo。 */
export interface AppInfo {
  name: string;
  version: string;
  edition: string;
  /** 上次异常退出是否留下恢复记录 */
  recovery_available: boolean;
}

/** 会话信息：对应后端 SessionInfo。 */
export interface SessionInfo {
  state: string;
  document_count: number;
  /** 当前会话总数 */
  session_count: number;
  task_event: string;
}

/** 文档信息：对应后端 DocumentInfo，一次返回元数据，不再单开 get_metadata。 */
export interface DocumentInfo {
  document_id: number;
  title: string;
  author: string | null;
  page_count: number;
  file_size: number;
  modified_time: number | null;
  path: string;
  state: string;
}

/** 已打开文档摘要：对应后端 OpenDocumentSummary。 */
export interface OpenDocumentSummary {
  document_id: number;
  title: string;
  page_count: number;
  path: string;
  active: boolean;
}

/** 视图状态：对应后端 Workspace.view_state，是当前页 / 缩放 / 滚动的唯一真值源。 */
export interface ViewStateInfo {
  page: number;
  zoom: number;
  scroll: number;
  active_document_id: number | null;
}

/** 查询应用信息；浏览器预览时返回默认值，保证界面可见。 */
export async function appInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>('app_info');
  } catch {
    return {
      name: '文档工作台',
      version: '0.1.0',
      edition: 'Stage 3 预览',
      recovery_available: false,
    };
  }
}

/** 健康检查：证明 IPC 通道可用。 */
export async function ping(): Promise<string> {
  try {
    return await invoke<string>('ping');
  } catch {
    return 'pong';
  }
}

/** 新建会话：调用后端跑通一次空流程。 */
export async function newSession(): Promise<SessionInfo> {
  return invoke<SessionInfo>('new_session');
}

/**
 * 弹出系统文件对话框选 PDF，返回绝对路径。
 *
 * 走 dialog 插件而非 `<input type="file">`：只有原生对话框能拿到真实路径，
 * Rust 侧 `open_document` 需要路径来创建 Document 并让 Driver 读元数据。
 */
export async function pickPdfFile(): Promise<string | null> {
  const result = await invoke<string | string[] | null>('plugin:dialog|open', {
    options: {
      multiple: false,
      directory: false,
      title: '打开 PDF',
      filters: [{ name: 'PDF 文档', extensions: ['pdf'] }],
    },
  });
  if (Array.isArray(result)) {
    return result[0] ?? null;
  }
  return result;
}

/** 打开文档：后端按 `00` §8.5 完成 create → Context → Command → Gate → Task → Driver。 */
export async function openDocument(path: string): Promise<DocumentInfo> {
  return invoke<DocumentInfo>('open_document', { path });
}

/** 关闭文档：移除 Document 与页签。 */
export async function closeDocument(documentId: number): Promise<void> {
  return invoke<void>('close_document', { documentId });
}

/** 读取文档原始字节：交给 PDF.js 在 WebView 内渲染，不产生跨 IPC 位图。 */
export async function readDocumentBytes(documentId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>('read_document_bytes', { documentId });
}

/** 当前会话已打开的文档列表。 */
export async function listDocuments(): Promise<OpenDocumentSummary[]> {
  return invoke<OpenDocumentSummary[]>('list_documents');
}

/**
 * 写回视图状态：ViewModel 不自己存真值，写回 `Workspace.view_state` 后以后端返回值投影。
 *
 * 返回归一化后的视图状态，前端以它为唯一展示依据。
 */
export async function setViewState(
  page: number,
  zoom: number,
  scroll: number,
): Promise<ViewStateInfo> {
  return invoke<ViewStateInfo>('set_view_state', { page, zoom, scroll });
}

/** 已注册驱动名称：诊断用。 */
export async function driverNames(): Promise<string[]> {
  return invoke<string[]>('driver_names');
}

/** 通用命令入口：保留给后续 Command 复用。 */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}

/** 单页旋转角：后端页布局摘要的一部分。 */
export interface PageRotationInfo {
  page: number;
  degrees: number;
}

/** 页布局摘要：一次编辑后前端只消费这一份投影，不另存真值。 */
export interface PageLayoutInfo {
  document_id: number;
  page_count: number;
  order: number[];
  rotations: PageRotationInfo[];
  current_page: number;
  selected_pages: number[];
  /** 本轮写入 History 的 Diff 数量。 */
  diff_count: number;
  /** 文档是否有未保存修改。 */
  dirty: boolean;
}

/** 旋转页面：pages 为空时后端退化为当前页。 */
export async function rotatePages(
  documentId: number,
  pages: number[],
  degrees: number,
): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('rotate_pages', { documentId, pages, degrees });
}

/** 删除页面：后端保证至少保留一页。 */
export async function deletePages(
  documentId: number,
  pages: number[],
): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('delete_pages', { documentId, pages });
}

/** 移动页面：from / to 均为当前显示顺序的 1 起位置。 */
export async function movePages(
  documentId: number,
  from: number,
  to: number,
): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('move_pages', { documentId, from, to });
}

/** 选择模式：框选替换 / Ctrl 追加 / Shift 连选。 */
export type SelectionMode = 'replace' | 'toggle' | 'range';

/** 写选择集：真值在后端 Session.selection。 */
export async function setPageSelection(
  documentId: number,
  pages: number[],
  mode: SelectionMode,
): Promise<number[]> {
  return invoke<number[]>('set_page_selection', { documentId, pages, mode });
}

/** 清空选择集。 */
export async function clearPageSelection(): Promise<void> {
  return invoke<void>('clear_page_selection');
}

/** 查询页布局：切换文档或界面重挂时拉取，前端不缓存第二份真值。 */
export async function getPageLayout(documentId: number): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('get_page_layout', { documentId });
}

/** 撤销：只回放 Diff，不重跑 Driver。 */
export async function undo(): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('undo');
}

/** 重做：只回放 Diff，不重跑 Driver。 */
export async function redo(): Promise<PageLayoutInfo> {
  return invoke<PageLayoutInfo>('redo');
}

/** 是否可撤销：工具栏禁用态。 */
export async function canUndo(): Promise<boolean> {
  return invoke<boolean>('can_undo');
}

/** 是否可重做：工具栏禁用态。 */
export async function canRedo(): Promise<boolean> {
  return invoke<boolean>('can_redo');
}

/** 对象节点：Objects Workspace 的叶子，Inspector 的数据源（规范 16 C-8，只读）。 */
export interface ObjectNodeInfo {
  object_id: number;
  page: number;
  /** 对象种类英文枚举名（Text / Image / Path / Unknown 等）。 */
  kind: string;
  /** 对象种类中文文案。 */
  kind_label: string;
  /** PDF 坐标边界 [x, y, width, height]，原点左下。 */
  bbox: [number, number, number, number];
  z_index: number;
  stream_ref: number | null;
  lifecycle: string;
  version: number;
  font_id: string | null;
  font_size: number | null;
  unicode: string | null;
  glyph_count: number;
}

/** 单页对象：Objects Workspace 第一层。 */
export interface ObjectPageInfo {
  page: number;
  /** 页宽（PDF 点），Overlay 坐标映射用。 */
  page_width: number;
  /** 页高（PDF 点），Overlay 坐标映射用。 */
  page_height: number;
  object_count: number;
  objects: ObjectNodeInfo[];
}

/** 对象树投影：一次返回整篇结构。 */
export interface ObjectTreeInfo {
  document_id: number;
  page_count: number;
  object_count: number;
  pages: ObjectPageInfo[];
  warnings: string[];
}

/**
 * 读取对象树投影：后端走 `PDF → Parser → ObjectMapper → ObjectTree`，只读不写盘。
 *
 * 这是 Objects Workspace 与 Inspector 的唯一数据源；UI 不得自己解析 PDF。
 */
export async function getObjectTree(documentId: number): Promise<ObjectTreeInfo> {
  return invoke<ObjectTreeInfo>('get_object_tree', { documentId });
}

/** 保存结果：对应后端 SaveInfo；蛇形字段直接透传，不改驼峰。 */
export interface SaveInfo {
  document_id: number;
  path: string;
  dirty: boolean;
  incremental: boolean;
  bytes_written: number;
  elapsed_ms: number;
}

/** 保存当前文档：目标路径取自后端 Document.source_path。 */
export async function saveDocument(): Promise<SaveInfo> {
  return invoke<SaveInfo>('save_document');
}

/** 另存为：目标路径由原生对话框给出。 */
export async function saveAsDocument(path: string): Promise<SaveInfo> {
  return invoke<SaveInfo>('save_as_document', { path });
}

/** 是否可保存：有活动文档且处于 Dirty。 */
export async function canSave(): Promise<boolean> {
  return invoke<boolean>('can_save');
}

/** 活动文档是否脏：标题栏 `*` 的唯一真值投影。 */
export async function isDirty(): Promise<boolean> {
  return invoke<boolean>('is_dirty');
}

/** 弹出原生「另存为」对话框，返回绝对路径；取消返回 null。 */
export async function pickSavePath(defaultPath?: string): Promise<string | null> {
  const result = await invoke<string | null>('plugin:dialog|save', {
    options: {
      title: '另存为',
      defaultPath,
      filters: [{ name: 'PDF 文档', extensions: ['pdf'] }],
    },
  });
  return result;
}

// ---------------------------------------------------------------------------
// 17 章：Desktop Integration（UI Bridge）。UI 只消费这些命令，不解析 PDF。
// ---------------------------------------------------------------------------

/** 命中结果：Hover / Selection 的几何来源。 */
export interface HitInfo {
  page: number;
  object_id: number;
  kind: string;
  kind_label: string;
  bbox: [number, number, number, number];
}

/** 选择投影（真值在 Session.selection_state）。 */
export interface SelectionInfo {
  page: number | null;
  primary: number | null;
  selected: number[];
  version: number;
}

/** 编辑事务投影（真值在 EditingEngine）。 */
export interface EditingInfo {
  active: boolean;
  object_id: number | null;
}

/** 能力投影：Ribbon 只按它决定按钮亮/灰。 */
export interface CapabilityInfo {
  can_edit: boolean;
  can_edit_text: boolean;
  can_edit_image: boolean;
  can_edit_path: boolean;
}

/** EditorState 快照：字段全部由 Core 填充。 */
export interface EditorSnapshot {
  document_id: number;
  page: number;
  zoom: number;
  selection: SelectionInfo;
  editing: EditingInfo;
  capability: CapabilityInfo;
  can_undo: boolean;
  can_redo: boolean;
  dirty: boolean;
  pending_patches: number;
  revision: number;
}

/** 提交结果。 */
export interface CommitInfo {
  object_id: number;
  page: number;
  before_version: number;
  after_version: number;
  operators: number;
  snapshot: EditorSnapshot;
}

/** 文本分区。 */
export interface TextInspector {
  unicode: string | null;
  font_id: string;
  font_size: number;
  glyph_count: number;
  color: string | null;
}

/** 图片分区。 */
export interface ImageInspector {
  pixel_width: number;
  pixel_height: number;
  colorspace: string;
  xref: number;
  resolution_dpi: number | null;
  compression: string | null;
}

/** 路径分区。 */
export interface PathInspector {
  command_count: number;
  stroke: string | null;
  fill: string | null;
  line_width: number | null;
}

/** Inspector 数据。 */
export interface InspectorInfo {
  object_id: number;
  page: number;
  kind: string;
  kind_label: string;
  bbox: [number, number, number, number];
  z_index: number;
  lifecycle: string;
  version: number;
  text: TextInspector | null;
  image: ImageInspector | null;
  path: PathInspector | null;
}

/** 绑定文档：解析对象树并清空选择 / 事务。 */
export async function editorOpen(documentId: number): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_open', { documentId });
}

/** 解绑：恢复默认状态（DT-11）。 */
export async function editorClose(documentId: number): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_close', { documentId });
}

/** 拉取快照（不解析）。 */
export async function editorState(documentId: number): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_state', { documentId });
}

/** 命中测试：x/y 为页面左上角起算的 CSS 像素。 */
export async function editorHitTest(
  documentId: number,
  page: number,
  x: number,
  y: number,
  pageWidthPx: number,
  rotation: number,
): Promise<HitInfo | null> {
  return invoke<HitInfo | null>('editor_hit_test', {
    documentId,
    page,
    x,
    y,
    pageWidthPx,
    rotation,
  });
}

/** 写选择。 */
export async function editorSelect(
  documentId: number,
  page: number,
  objectId: number,
  multi: boolean,
  ctrl: boolean,
  shift: boolean,
): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_select', {
    documentId,
    page,
    objectId,
    multi,
    ctrl,
    shift,
  });
}

/** 清空选择。 */
export async function editorClearSelection(
  documentId: number,
  page: number,
): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_clear_selection', { documentId, page });
}

/** 全选当前页对象。 */
export async function editorSelectAll(
  documentId: number,
  page: number,
): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_select_all', { documentId, page });
}

/** 开始编辑。 */
export async function editorBeginEdit(
  documentId: number,
  objectId: number,
  text: string,
): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_begin_edit', { documentId, objectId, text });
}

/** 提交编辑；`text` 为编辑框里的最终文本。 */
export async function editorCommit(documentId: number, text?: string): Promise<CommitInfo> {
  return invoke<CommitInfo>('editor_commit', { documentId, text: text ?? null });
}

/** 取消编辑。 */
export async function editorCancel(documentId: number): Promise<EditorSnapshot> {
  return invoke<EditorSnapshot>('editor_cancel', { documentId });
}

/** Inspector 数据。 */
export async function editorInspector(objectId: number): Promise<InspectorInfo | null> {
  return invoke<InspectorInfo | null>('editor_inspector', { objectId });
}

// ---------------------------------------------------------------------------
// 19 章（18C）：Desktop Feature Completion。只读事件游标与运行态投影。
// ---------------------------------------------------------------------------

/** 单条事件投影：kind / message，不携带 payload（真值仍走命令快照）。 */
export interface CoreEvent {
  seq: number;
  kind: string;
  message: string;
}

/** 事件游标结果：cursor 为最新事件总数。 */
export interface EventCursor {
  cursor: number;
  events: CoreEvent[];
}

/** 只读核心状态投影：保存状态与最近保存反馈。 */
export interface CoreStatus {
  document_id: number;
  save_state: string;
  last_save_message: string;
  save_failed: boolean;
  save_incremental: boolean;
  event_count: number;
}

/** 增量拉取事件；cursor=0 表示从头拉取。 */
export async function coreEvents(cursor: number): Promise<EventCursor> {
  return invoke<EventCursor>('core_events', { cursor });
}

/** 读取运行态投影（保存状态 / 最近保存事件）。 */
export async function coreStatus(): Promise<CoreStatus> {
  return invoke<CoreStatus>('core_status');
}
