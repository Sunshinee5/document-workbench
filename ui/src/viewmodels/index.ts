// ViewModel：**Projection，不是 State**（`01` §2.3.5）。
//
// 真值源只有一处：后端 `Workspace.view_state`（当前页 / 缩放 / 滚动 / 活动文档）。
// 本文件持有的字段只是它的投影，用户操作时写回后端、再以后端返回值为准，
// 禁止在这里另存一份权威副本（两份即两个真值源）。
import {
  appInfo,
  closeDocument,
  listDocuments,
  openDocument,
  pickPdfFile,
  clearPageSelection,
  deletePages,
  getPageLayout,
  movePages,
  rotatePages,
  setPageSelection,
  setViewState,
  type AppInfo,
  type DocumentInfo,
  type PageLayoutInfo,
  type SelectionMode,
} from '../ipc';

/** 页签：只含 document_id 与标题，与后端 `Workspace.tabs` 一致。 */
export interface OpenTab {
  documentId: number;
  title: string;
  pageCount: number;
}

/** 界面只读状态快照。 */
export interface WorkbenchState {
  title: string;
  version: string;
  edition: string;
  status: string;
  /** 已打开文档（页签） */
  tabs: OpenTab[];
  /** 当前活动文档 */
  activeDocumentId: number | null;
  /** 活动文档标题 */
  activeTitle: string;
  /** 活动文档作者 */
  author: string;
  /** 活动文档页数（来自 DocumentMetadata.page_count） */
  pageCount: number;
  /** 当前页（投影自 view_state.current_page） */
  currentPage: number;
  /** 缩放（投影自 view_state.zoom） */
  zoom: number;
  /** 是否正在加载文档 */
  loading: boolean;
  /** 最近一次错误，空串表示无错误 */
  error: string;
  /** 最近一次任务事件 */
  taskEvent: string;
  /** 当前页序（原始页码）；Feature-02 的移动操作会改变它 */
  pageOrder: number[];
  /** 原始页码 → 顺时针旋转角 */
  rotations: Record<number, number>;
  /** 选中的页位置（Session.selection 投影） */
  selectedPages: number[];
  /** 页结构版本号：变化即让查看器重渲染，Thumbnail 也随之刷新 */
  layoutRevision: number;
}

/** 缩放档位：产品只给有限档，避免自由输入造成视图抖动。 */
const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3];

/** 工作台 ViewModel：所有展示值都由后端状态投影而来。 */
export class WorkbenchViewModel {
  private stateValue: WorkbenchState = {
    title: '文档工作台',
    version: '0.1.0',
    edition: '',
    status: '就绪',
    tabs: [],
    activeDocumentId: null,
    activeTitle: '',
    author: '',
    pageCount: 0,
    currentPage: 1,
    zoom: 1,
    loading: false,
    error: '',
    taskEvent: '',
    pageOrder: [],
    rotations: {},
    selectedPages: [],
    layoutRevision: 0,
  };

  private listeners: Array<() => void> = [];

  /** 视图状态写回防抖计时器：滚动会高频触发，不逐次打 IPC。 */
  private viewStateTimer: number | null = null;

  private pendingViewState: { page: number; zoom: number; scroll: number } | null = null;

  /** 当前状态快照：供 React 触发重渲染。 */
  get snapshot(): WorkbenchState {
    return { ...this.stateValue, tabs: [...this.stateValue.tabs] };
  }

  /** 订阅状态变化，返回取消订阅函数。 */
  subscribe(listener: () => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((item) => item !== listener);
    };
  }

  private update(patch: Partial<WorkbenchState>): void {
    this.stateValue = { ...this.stateValue, ...patch };
    for (const listener of this.listeners) {
      listener();
    }
  }

  /** 加载应用信息，并恢复后端已有的文档列表（同一会话内重挂界面用）。 */
  async initialize(): Promise<void> {
    const info: AppInfo = await appInfo();
    this.update({
      title: info.name,
      version: info.version,
      edition: info.edition,
      status: `就绪 · ${info.name} ${info.version}`,
    });
    await this.refreshTabs();
  }

  /** 从后端拉取页签列表：以 `Workspace.tabs` 为准，不在前端自行维护顺序。 */
  private async refreshTabs(): Promise<void> {
    try {
      const summaries = await listDocuments();
      const tabs: OpenTab[] = summaries.map((item) => ({
        documentId: item.document_id,
        title: item.title,
        pageCount: item.page_count,
      }));
      const active = summaries.find((item) => item.active);
      this.update({
        tabs,
        activeDocumentId: active?.document_id ?? this.stateValue.activeDocumentId,
        activeTitle: active?.title ?? this.stateValue.activeTitle,
        pageCount: active?.page_count ?? this.stateValue.pageCount,
      });
    } catch {
      // 浏览器预览环境无后端：保持空列表，界面仍可见。
    }
  }

  /** 选择文件并打开：Action(Open) → CoreState → create → Command → Gate → Task → Driver。 */
  async openFile(): Promise<void> {
    this.update({ error: '', loading: true, status: '正在打开…' });
    try {
      const path = await pickPdfFile();
      if (!path) {
        this.update({ loading: false, status: '已取消打开' });
        return;
      }
      const info: DocumentInfo = await openDocument(path);
      const tabs = [
        ...this.stateValue.tabs.filter((tab) => tab.documentId !== info.document_id),
        { documentId: info.document_id, title: info.title, pageCount: info.page_count },
      ];
      this.update({
        tabs,
        activeDocumentId: info.document_id,
        activeTitle: info.title,
        author: info.author ?? '',
        pageCount: info.page_count,
        currentPage: 1,
        zoom: 1,
        loading: false,
        status: `已打开 · ${info.title} · ${info.page_count} 页 · ${formatSize(info.file_size)}`,
        taskEvent: `文档已就绪（${info.state}）`,
        pageOrder: [],
        rotations: {},
        selectedPages: [],
      });
      await this.refreshLayout(info.document_id);
    } catch (error) {
      this.update({
        loading: false,
        error: messageOf(error),
        status: `打开失败：${messageOf(error)}`,
      });
    }
  }

  /** 切换活动文档：只改视图焦点，不产生 Command。 */
  async selectDocument(documentId: number): Promise<void> {
    const tab = this.stateValue.tabs.find((item) => item.documentId === documentId);
    if (!tab) {
      return;
    }
    this.update({
      activeDocumentId: documentId,
      activeTitle: tab.title,
      pageCount: tab.pageCount,
      currentPage: 1,
      zoom: 1,
      error: '',
      pageOrder: [],
      rotations: {},
      selectedPages: [],
    });
    await this.refreshLayout(documentId);
  }

  /** 关闭文档：移除 Document 与页签。 */
  async closeTab(documentId: number): Promise<void> {
    try {
      await closeDocument(documentId);
      const tabs = this.stateValue.tabs.filter((tab) => tab.documentId !== documentId);
      const wasActive = this.stateValue.activeDocumentId === documentId;
      const next = wasActive ? tabs[0] : undefined;
      this.update({
        tabs,
        activeDocumentId: wasActive ? (next?.documentId ?? null) : this.stateValue.activeDocumentId,
        activeTitle: wasActive ? (next?.title ?? '') : this.stateValue.activeTitle,
        pageCount: wasActive ? (next?.pageCount ?? 0) : this.stateValue.pageCount,
        currentPage: 1,
        status: tabs.length === 0 ? '就绪' : this.stateValue.status,
      });
    } catch (error) {
      this.update({ error: messageOf(error), status: `关闭失败：${messageOf(error)}` });
    }
  }

  /** 按档位缩放：写回 view_state，以后端返回值为准。 */
  setZoom(zoom: number): void {
    const next = Math.min(8, Math.max(0.1, zoom));
    this.update({ zoom: next });
    this.scheduleViewState(this.stateValue.currentPage, next, 0);
  }

  /** 放大一档。 */
  zoomIn(): void {
    const current = this.stateValue.zoom;
    const next =
      ZOOM_STEPS.find((step) => step > current + 0.001) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1];
    this.setZoom(next);
  }

  /** 缩小一档。 */
  zoomOut(): void {
    const current = this.stateValue.zoom;
    const reversed = [...ZOOM_STEPS].reverse();
    const next = reversed.find((step) => step < current - 0.001) ?? ZOOM_STEPS[0];
    this.setZoom(next);
  }

  /** 跳页：页码由界面输入，这里做范围归一化。 */
  goToPage(page: number): void {
    const total = this.stateValue.pageCount || 1;
    const next = Math.min(total, Math.max(1, Math.round(page)));
    this.update({ currentPage: next });
    this.scheduleViewState(next, this.stateValue.zoom, 0);
  }

  /** 上一页。 */
  prevPage(): void {
    this.goToPage(this.stateValue.currentPage - 1);
  }

  /** 下一页。 */
  nextPage(): void {
    this.goToPage(this.stateValue.currentPage + 1);
  }

  /**
   * 滚动上报：由 Viewer 在滚动时调用，只更新投影并防抖写回后端。
   *
   * 写回的不是「文档内容」，因此不产生 Command、不进 History（`01` §2.3.2）。
   */
  reportScroll(page: number, scroll: number): void {
    if (page !== this.stateValue.currentPage) {
      this.update({ currentPage: page });
    }
    this.scheduleViewState(page, this.stateValue.zoom, scroll);
  }

  /** 适应宽度：由 Viewer 算出比例后回报。 */
  reportFitZoom(zoom: number): void {
    this.setZoom(zoom);
  }

  /** 页数以 PDF.js 实际解析结果为准：Driver 读的是文件声明，解析器读到的是真实页数。 */
  reportPageCount(count: number): void {
    if (count > 0 && count !== this.stateValue.pageCount) {
      this.update({ pageCount: count });
    }
  }

  /** 防抖写回 view_state：250ms 内的多次滚动只打一次 IPC。 */
  private scheduleViewState(page: number, zoom: number, scroll: number): void {
    this.pendingViewState = { page, zoom, scroll };
    if (this.viewStateTimer !== null) {
      window.clearTimeout(this.viewStateTimer);
    }
    this.viewStateTimer = window.setTimeout(() => {
      const pending = this.pendingViewState;
      this.viewStateTimer = null;
      this.pendingViewState = null;
      if (!pending) {
        return;
      }
      void setViewState(pending.page, pending.zoom, pending.scroll)
        .then((view) => {
          // 以后端归一化后的值为准：前端不保留自己的版本。
          this.update({ currentPage: view.page, zoom: view.zoom });
        })
        .catch(() => {
          // 浏览器预览环境无后端：保留本地投影即可，不打断使用。
        });
    }, 250);
  }

  // ---- Feature-02：页面编辑。所有方法都只调 IPC，不直接改 Document。 ----

  /** 从后端拉取页布局：打开新文档 / 切换文档后调用，前端不缓存第二份真值。 */
  async refreshLayout(documentId: number): Promise<void> {
    try {
      this.applyLayout(await getPageLayout(documentId));
    } catch {
      // 浏览器预览环境无后端：保持当前投影即可。
    }
  }

  /** 把后端页布局摘要投影到界面：一次更新页码、页序、旋转、选择与缩略图版本号。 */
  private applyLayout(info: PageLayoutInfo): void {
    const rotations: Record<number, number> = {};
    for (const item of info.rotations) {
      rotations[item.page] = item.degrees;
    }
    this.update({
      pageCount: info.page_count,
      pageOrder: info.order,
      rotations,
      selectedPages: info.selected_pages,
      currentPage: info.current_page,
      layoutRevision: this.stateValue.layoutRevision + 1,
      status: `页面已更新 · 共 ${info.page_count} 页`,
    });
  }

  /** 旋转选中页；无选择时旋转当前页。 */
  async rotate(delta: number): Promise<void> {
    const id = this.stateValue.activeDocumentId;
    if (id === null) {
      return;
    }
    try {
      const pages = this.stateValue.selectedPages.length
        ? this.stateValue.selectedPages
        : [this.stateValue.currentPage];
      this.applyLayout(await rotatePages(id, pages, delta));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 删除选中页；无选择时删除当前页。 */
  async deleteSelectedPages(): Promise<void> {
    const id = this.stateValue.activeDocumentId;
    if (id === null) {
      return;
    }
    try {
      const pages = this.stateValue.selectedPages.length
        ? this.stateValue.selectedPages
        : [this.stateValue.currentPage];
      this.applyLayout(await deletePages(id, pages));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 移动一页：from / to 为显示位置（1 起），供拖拽排序调用。 */
  async movePage(from: number, to: number): Promise<void> {
    const id = this.stateValue.activeDocumentId;
    if (id === null) {
      return;
    }
    try {
      this.applyLayout(await movePages(id, from, to));
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 把当前页上移 / 下移一位；工具栏按钮走这里。 */
  async shiftCurrent(delta: -1 | 1): Promise<void> {
    const from = this.stateValue.currentPage;
    const to = from + delta;
    if (to < 1 || to > this.stateValue.pageCount) {
      return;
    }
    await this.movePage(from, to);
    this.update({ currentPage: to });
  }

  /** 写选择集：Ctrl / Shift / 框选三种模式，真值在后端 Session。 */
  async selectPages(pages: number[], mode: SelectionMode): Promise<void> {
    const id = this.stateValue.activeDocumentId;
    if (id === null) {
      return;
    }
    try {
      this.update({ selectedPages: await setPageSelection(id, pages, mode) });
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 清空选择集。 */
  async clearSelection(): Promise<void> {
    try {
      await clearPageSelection();
      this.update({ selectedPages: [] });
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 清空错误提示。 */
  clearError(): void {
    this.update({ error: '' });
  }
}

/** 把异常统一转成可展示的中文文案。 */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** 字节数转可读大小。 */
export function formatSize(bytes: number): string {
  if (bytes <= 0) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
