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
  canRedo,
  canUndo,
  deletePages,
  getPageLayout,
  movePages,
  redo,
  rotatePages,
  pickSavePath,
  saveAsDocument,
  saveDocument,
  setPageSelection,
  undo,
  type SaveInfo,
  setViewState,
  getObjectTree,
  type AppInfo,
  type DocumentInfo,
  type ObjectTreeInfo,
  type PageLayoutInfo,
  type SelectionMode,
} from '../ipc';

/** 页签：只含 document_id 与标题，与后端 `Workspace.tabs` 一致。 */
export interface OpenTab {
  documentId: number;
  title: string;
  pageCount: number;
  /** 源文件绝对路径（Inspector 展示用）。 */
  path: string;
  /** 文件大小（字节）；重挂界面时后端摘要不含该字段，记 0 表示未知。 */
  fileSize: number;
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
  /** 活动文档源文件路径 */
  sourcePath: string;
  /** 活动文档文件大小（字节），0 表示未知 */
  fileSize: number;
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
  /** 是否可撤销（History.cursor 投影） */
  canUndo: boolean;
  /** 是否可重做（History.cursor 投影） */
  canRedo: boolean;
  /** 文档是否有未保存修改 */
  dirty: boolean;
  /** 保存次数：变化即让查看器重新读取文件字节 */
  saveRevision: number;
  /** 最近一次保存时间（HH:MM），用于状态栏展示 */
  lastSavedAt: string;
  /** 上次异常退出是否留下恢复记录 */
  recoveryAvailable: boolean;
  /**
   * 对象树投影（只读，来自 `get_object_tree`）。
   *
   * 它投影的是**源文件**结构，不是内存里的未保存编辑：
   * 保存成功后重新拉取，未保存的改动不会出现在这里。
   */
  objectTree: ObjectTreeInfo | null;
  /** 对象树是否正在解析。 */
  objectTreeLoading: boolean;
  /** 对象树解析失败原因；空串表示无错误。 */
  objectTreeError: string;
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
    sourcePath: '',
    fileSize: 0,
    currentPage: 1,
    zoom: 1,
    loading: false,
    error: '',
    taskEvent: '',
    pageOrder: [],
    rotations: {},
    selectedPages: [],
    layoutRevision: 0,
    canUndo: false,
    canRedo: false,
    dirty: false,
    saveRevision: 0,
    lastSavedAt: '',
    recoveryAvailable: false,
    objectTree: null,
    objectTreeLoading: false,
    objectTreeError: '',
  };

  private listeners: Array<() => void> = [];

  /** 视图状态写回防抖计时器：滚动会高频触发，不逐次打 IPC。 */
  private viewStateTimer: number | null = null;

  private pendingViewState: { page: number; zoom: number; scroll: number } | null = null;

  /** 是否有未保存修改：标题栏 `*` 与菜单禁用态的唯一前端投影。 */
  get isDirty(): boolean {
    return this.stateValue.dirty;
  }

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
      recoveryAvailable: info.recovery_available,
      status: info.recovery_available
        ? `检测到上次未保存状态，请确认是否需要恢复 · ${info.name} ${info.version}`
        : `就绪 · ${info.name} ${info.version}`,
    });
    await this.refreshTabs();
    await this.refreshHistoryState();
  }

  /** 从后端拉取页签列表：以 `Workspace.tabs` 为准，不在前端自行维护顺序。 */
  private async refreshTabs(): Promise<void> {
    try {
      const summaries = await listDocuments();
      const tabs: OpenTab[] = summaries.map((item) => ({
        documentId: item.document_id,
        title: item.title,
        pageCount: item.page_count,
        path: item.path,
        fileSize: 0,
      }));
      const active = summaries.find((item) => item.active);
      this.update({
        tabs,
        activeDocumentId: active?.document_id ?? this.stateValue.activeDocumentId,
        activeTitle: active?.title ?? this.stateValue.activeTitle,
        pageCount: active?.page_count ?? this.stateValue.pageCount,
        sourcePath: active?.path ?? this.stateValue.sourcePath,
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
        {
          documentId: info.document_id,
          title: info.title,
          pageCount: info.page_count,
          path: info.path,
          fileSize: info.file_size,
        },
      ];
      this.update({
        tabs,
        activeDocumentId: info.document_id,
        activeTitle: info.title,
        author: info.author ?? '',
        pageCount: info.page_count,
        sourcePath: info.path,
        fileSize: info.file_size,
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
      await this.refreshHistoryState();
      await this.loadObjectTree();
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
      sourcePath: tab.path,
      fileSize: tab.fileSize,
      currentPage: 1,
      zoom: 1,
      error: '',
      pageOrder: [],
      rotations: {},
      selectedPages: [],
    });
    await this.refreshLayout(documentId);
    await this.refreshHistoryState();
    await this.loadObjectTree();
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
        sourcePath: wasActive ? (next?.path ?? '') : this.stateValue.sourcePath,
        fileSize: wasActive ? (next?.fileSize ?? 0) : this.stateValue.fileSize,
        currentPage: 1,
        status: tabs.length === 0 ? '就绪' : this.stateValue.status,
        // 关掉最后一个文档后对象树必须清空，否则 Workspace 还显示已关闭文档的结构。
        objectTree: tabs.length === 0 ? null : this.stateValue.objectTree,
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

  /** 刷新 History 游标投影：撤销 / 重做按钮的禁用态以它为准。 */
  async refreshHistoryState(): Promise<void> {
    try {
      const [undoable, redoable] = await Promise.all([canUndo(), canRedo()]);
      this.update({ canUndo: undoable, canRedo: redoable });
    } catch {
      // 浏览器预览环境无后端：按钮保持禁用即可。
    }
  }

  /** 撤销：后端只回放 Diff，前端消费布局摘要。 */
  async undo(): Promise<void> {
    try {
      const info = await undo();
      this.applyLayout(info);
      await this.refreshHistoryState();
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

  /** 重做：后端只回放 Diff，前端消费布局摘要。 */
  async redo(): Promise<void> {
    try {
      const info = await redo();
      this.applyLayout(info);
      await this.refreshHistoryState();
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
  }

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
      dirty: info.dirty,
      status: `页面已更新 · 共 ${info.page_count} 页 · 已记录 ${info.diff_count} 条 Diff · ${info.dirty ? '未保存' : '已保存'}`,
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
      await this.refreshHistoryState();
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
      await this.refreshHistoryState();
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
      await this.refreshHistoryState();
    } catch (error) {
      this.update({ error: messageOf(error) });
    }
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

  // ---- Feature-04：保存 / 另存为。真值在后端 save_point 与 DocumentState。 ----

  /** 保存当前文档：Clean 时后端直接跳过写盘，不发 SaveStarted。 */
  async save(): Promise<void> {
    if (this.stateValue.activeDocumentId === null) {
      return;
    }
    this.update({ error: '', status: '正在保存…' });
    try {
      this.applySaveInfo(await saveDocument());
    } catch (error) {
      const text = messageOf(error);
      this.update({ error: text, status: `保存失败：${text}` });
    }
  }

  /** 另存为：先走原生路径对话框，再把路径交给 SaveAsCommand。 */
  async saveAs(): Promise<void> {
    if (this.stateValue.activeDocumentId === null) {
      return;
    }
    this.update({ error: '' });
    try {
      const path = await pickSavePath(this.stateValue.activeTitle || undefined);
      if (!path) {
        this.update({ status: '已取消另存为' });
        return;
      }
      this.update({ status: '正在另存为…' });
      const info = await saveAsDocument(path);
      this.applySaveInfo(info);
      // 页签标题真值在 Workspace.tabs，保存后刷新投影。
      await this.refreshTabs();
      const title = fileNameOf(info.path);
      this.update({ activeTitle: title });
    } catch (error) {
      const text = messageOf(error);
      this.update({ error: text, status: `另存为失败：${text}` });
    }
  }

  /** 把后端 SaveInfo 投影到界面：清脏、版本号递增（触发 PDF.js 重新读字节）。 */
  private applySaveInfo(info: SaveInfo): void {
    const now = new Date();
    const time = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
    this.update({
      dirty: info.dirty,
      error: '',
      lastSavedAt: time,
      sourcePath: info.path,
      saveRevision: this.stateValue.saveRevision + 1,
      layoutRevision: this.stateValue.layoutRevision + 1,
      status: `已保存：${time}`,
      taskEvent: `${info.incremental ? '增量保存' : '全量保存'} · ${formatSize(info.bytes_written)} · ${info.elapsed_ms}ms`,
    });
    // 源文件字节已变，对象树必须重新解析，否则 Workspace 展示的是旧结构。
    void this.loadObjectTree();
  }

  /**
   * 拉取对象树投影：Objects Workspace 与 Inspector 的唯一数据源。
   *
   * 只读：不产生 Command / Diff，不改 Document；解析在 dw-pdf 内完成。
   */
  async loadObjectTree(): Promise<void> {
    const id = this.stateValue.activeDocumentId;
    if (id === null) {
      this.update({ objectTree: null, objectTreeLoading: false, objectTreeError: '' });
      return;
    }
    this.update({ objectTreeLoading: true, objectTreeError: '' });
    try {
      const tree = await getObjectTree(id);
      this.update({ objectTree: tree, objectTreeLoading: false });
    } catch (error) {
      // 浏览器预览环境无后端时保持空树，不打断界面。
      this.update({
        objectTree: null,
        objectTreeLoading: false,
        objectTreeError: messageOf(error),
      });
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

/** 从完整路径取文件名：Save As 后页签标题用。 */
function fileNameOf(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
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
