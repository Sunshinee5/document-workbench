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
    return { name: '文档工作台', version: '0.1.0', edition: 'Stage 2 预览' };
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
  /** 本轮生成并暂存的 Diff 数量；History 写入归后续 Feature。 */
  diff_count: number;
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
