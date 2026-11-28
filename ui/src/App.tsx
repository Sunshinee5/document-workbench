// 工作台主界面（规范 16 六区 / 18B Framework Freeze）。
//
// 职责边界：UI 只做「展示投影 + 转发意图」。
// 18B 起：App 不写任何能力 switch；Ribbon / Inspector / StatusBar / Overlay / 快捷键 /
// 右键菜单 / 光标全部从 Framework 注册表驱动。新增 OCR 只需 registerFeature。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Dashboard, recordDashboard } from './components/Dashboard';
import { PdfViewer, type PageOverlayData, type PdfViewerHandle } from './components/PdfViewer';
import { Inspector } from './components/inspector';
import {
  Ribbon,
  StatusBar,
  TitleBar,
} from './components/shell';
import { ContextMenu, ToastStack, useToasts, type MenuItemView } from './components/ui';
import {
  AiPanel,
  AnnotationPanel,
  AttachmentsPanel,
  BookmarksPanel,
  CollaborationPanel,
  DocumentTabs,
  ObjectsPanel,
  OcrWorkspace,
  OutlinePanel,
  PagesPanel,
  PluginPanel,
  SearchPanel,
  SettingsPanel,
  WorkspaceNav,
  type WorkspaceKind,
} from './components/workspace';
import { EditorController, type EditorMode } from './editor/editorState';
import { EventBridge } from './editor/eventBridge';
import {
  desktopFramework,
  initializeFramework,
  type FrameworkActions,
  type FrameworkContext,
  type InspectorDocumentInfo,
  type OverlayObject,
} from './framework';
import type { CommitInfo, InspectorInfo, ObjectNodeInfo } from './ipc';
import { OcrController } from './ocr/ocrController';
import { applyTheme, loadTheme, saveTheme, watchSystemTheme, type ThemeMode } from './theme';
import { WorkbenchViewModel, formatSize } from './viewmodels';
import './index.css';

/** 初始化内置能力（幂等）：所有注册表在 App 渲染前就绪。 */
initializeFramework();

const NAV_DEFAULT_WIDTH = 224;
const NAV_AUTO_COLLAPSE = 1280;

export default function App() {
  const viewModel = useMemo(() => new WorkbenchViewModel(), []);
  const [state, setState] = useState(() => viewModel.snapshot);
  const viewerRef = useRef<PdfViewerHandle | null>(null);

  // ---- 纯 UI 状态：不进 Session / Document，不产生 Command ----
  const [ribbonTab, setRibbonTab] = useState<string>('home');
  const [workspace, setWorkspace] = useState<WorkspaceKind>('pages');
  const [navCollapsed, setNavCollapsed] = useState(() => window.innerWidth <= NAV_AUTO_COLLAPSE);
  const [navWidth, setNavWidth] = useState(NAV_DEFAULT_WIDTH);
  const [mode, setModeState] = useState<string>('browse');
  const [theme, setTheme] = useState<ThemeMode>(() => loadTheme());
  const [focusedObject, setFocusedObject] = useState<ObjectNodeInfo | null>(null);
  const editor = useMemo(() => new EditorController(), []);
  const [editorState, setEditorState] = useState(() => editor.snapshot);
  const eventBridge = useMemo(() => new EventBridge(), []);
  const [feedback, setFeedback] = useState(() => eventBridge.snapshot);
  const ocr = useMemo(() => new OcrController(), []);
  const [ocrState, setOcrState] = useState(() => ocr.snapshot);
  const [inspector, setInspector] = useState<InspectorInfo | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<string[]>([]);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const lastDashboardOcrRef = useRef<number | null>(null);
  const { toasts, push, dismiss } = useToasts();

  useEffect(() => viewModel.subscribe(() => setState(viewModel.snapshot)), [viewModel]);
  useEffect(() => editor.subscribe(() => setEditorState(editor.snapshot)), [editor]);
  useEffect(() => eventBridge.subscribe(() => setFeedback(eventBridge.snapshot)), [eventBridge]);
  useEffect(() => ocr.subscribe(() => setOcrState(ocr.snapshot)), [ocr]);
  useEffect(() => {
    void viewModel.initialize().then(() => eventBridge.poll());
  }, [viewModel, eventBridge]);
  useEffect(() => {
    void ocr.initialize();
  }, [ocr]);

  // Dashboard 最近文件：只记 UI 偏好投影，真值仍来自 ViewModel。
  useEffect(() => {
    if (state.activeDocumentId === null || !state.sourcePath) {
      return;
    }
    const title = state.activeTitle || state.sourcePath.split(/[\\/]/).pop() || '未命名文档';
    recordDashboard('files', {
      id: state.sourcePath,
      title,
      path: state.sourcePath,
      detail: state.pageCount > 0 ? `${state.pageCount} 页` : undefined,
    });
  }, [state.activeDocumentId, state.activeTitle, state.sourcePath, state.pageCount]);

  // Dashboard 最近 OCR：每个会话只记一次，重复轮询不产生重复条目。
  useEffect(() => {
    const session = ocrState.session;
    if (session.state !== 'Completed' || session.session_id === 0) {
      return;
    }
    if (lastDashboardOcrRef.current === session.session_id) {
      return;
    }
    lastDashboardOcrRef.current = session.session_id;
    recordDashboard('ocr', {
      id: `ocr:${session.session_id}`,
      title: state.activeTitle ? `OCR · ${state.activeTitle}` : 'OCR 识别完成',
      detail: `${session.results.length} 条结果`,
    });
  }, [ocrState.session, state.activeTitle]);

  // HiDPI（规范 20 §五）：WebView 已处理像素缩放，这里只登记 DPR 供 CSS Token 使用。
  useEffect(() => {
    const applyScale = () => {
      const scale = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
      document.documentElement.style.setProperty('--dw-ui-scale', String(scale));
    };
    applyScale();
    window.addEventListener('resize', applyScale);
    return () => window.removeEventListener('resize', applyScale);
  }, []);

  useEffect(() => {
    applyTheme(theme);
    saveTheme(theme);
  }, [theme]);
  useEffect(() => watchSystemTheme(() => applyTheme(theme)), [theme]);
  useEffect(() => {
    const onResize = () => setNavCollapsed(window.innerWidth <= NAV_AUTO_COLLAPSE);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // 切文档：解绑旧会话、绑定新会话并清空投影（DT-11）。
  const lastDocumentRef = useRef<number | null>(null);
  useEffect(() => {
    const previous = lastDocumentRef.current;
    const current = state.activeDocumentId;
    lastDocumentRef.current = current;
    setFocusedObject(null);
    setInspector(null);
    setContextMenu(null);
    if (previous !== null && previous !== current) void editor.close(previous);
    if (current !== null) void editor.open(current);
    void ocr.refresh();
  }, [state.activeDocumentId, editor, ocr]);

  // Save 结果反馈：事件桥翻转到 success/failed 时轻提示（不写回真值）。
  const lastSaveFeedbackRef = useRef(feedback.saveFeedback);
  useEffect(() => {
    const previous = lastSaveFeedbackRef.current;
    lastSaveFeedbackRef.current = feedback.saveFeedback;
    if (previous === feedback.saveFeedback) return;
    if (feedback.saveFeedback === 'success') {
      push(feedback.saveMessage || '保存成功', 'success');
    } else if (feedback.saveFeedback === 'failed') {
      push(feedback.saveMessage || '保存失败', 'error');
    }
  }, [feedback.saveFeedback, feedback.saveMessage, push]);

  // 主选对象变化 → 拉 Inspector（只读投影）。
  useEffect(() => {
    const primary = editorState.selection.primary;
    if (primary === null) {
      setInspector(null);
      return;
    }
    let cancelled = false;
    void editor.inspector(primary).then((info) => {
      if (!cancelled) setInspector(info);
    });
    return () => {
      cancelled = true;
    };
  }, [editor, editorState.selection.primary, editorState.revision]);

  useEffect(() => {
    const tree = state.objectTree;
    if (!tree) {
      setExpandedNodes([]);
      return;
    }
    setExpandedNodes(tree.pages.length > 0 ? [`page:${tree.pages[0].page}`] : []);
  }, [state.objectTree]);

  const lastErrorRef = useRef('');
  useEffect(() => {
    if (state.error && state.error !== lastErrorRef.current) {
      lastErrorRef.current = state.error;
      push(state.error, 'error');
    }
    if (!state.error) lastErrorRef.current = '';
  }, [state.error, push]);

  // ---- FrameworkContext：全部来自投影，无业务真值 ----
  const parserText = state.objectTreeLoading
    ? '解析中'
    : state.objectTreeError
      ? '解析失败'
      : state.objectTree
        ? state.objectTree.warnings.length > 0
          ? `警告 ${state.objectTree.warnings.length}`
          : '正常'
        : '—';
  const saveText = state.dirty
    ? '未保存'
    : state.lastSavedAt
      ? `已保存 ${state.lastSavedAt}`
      : '—';
  const ocrAvailable = ocrState.providers.some((provider) => provider.available);
  // 当前 OCR 结果（Overlay / Workspace / Inspector / Ribbon 共用同一只读序号）。
  const activeOcrIndex = ocrState.activeIndex ?? 0;
  const activeOcrResult = ocrState.session.results[activeOcrIndex];
  const activeOcrReview = ocrState.session.reviews[activeOcrIndex];
  const ocrApplyable = Boolean(
    activeOcrResult
    && activeOcrResult.source_object_id !== null
    && activeOcrReview?.decision === '已接受',
  );
  const ocrCanApply = useMemo(
    () => ocrState.session.results.map((result, index) => (
      result.source_object_id !== null
      && ocrState.session.reviews[index]?.decision === '已接受'
      && !ocrState.applying
    )),
    [ocrState.session.results, ocrState.session.reviews, ocrState.applying],
  );

  // 选择校验摘要（只读投影）：primary 必须属于 selected，且选择页与当前页一致。
  const selectionValidation = (() => {
    const selection = editorState.selection;
    if (selection.selected.length === 0) return '无选择';
    if (selection.primary !== null && !selection.selected.includes(selection.primary)) return '失效';
    if (selection.page !== null && selection.page !== state.currentPage) return '跨页';
    return '有效';
  })();

  // 命中优先级反馈：Hover 对象的 z_index（来自对象树只读投影）。
  const hoverPriority = (() => {
    const hover = editorState.hover;
    if (!hover) return '';
    const object = state.objectTree?.pages
      .find((page) => page.page === hover.page)
      ?.objects.find((item) => item.object_id === hover.object_id);
    return object ? `z=${object.z_index}` : '';
  })();

  const toolLabel = desktopFramework.registries.tools.get(mode)?.label ?? mode;

  const ctx = useMemo<FrameworkContext>(() => ({
    mode,
    theme,
    hasDocument: state.activeDocumentId !== null,
    page: state.currentPage,
    pageCount: state.pageCount,
    zoomPercent: Math.round(state.zoom * 100),
    capability: editorState.capability,
    selection: editorState.selection,
    editing: editorState.editing,
    hover: editorState.hover,
    canUndo: state.canUndo,
    canRedo: state.canRedo,
    dirty: state.dirty,
    pendingPatches: editorState.pendingPatches,
    revision: editorState.revision,
    parserText,
    saveText,
    taskEvent: state.taskEvent,
    error: state.error,
    toolLabel,
    hitPriority: hoverPriority,
    selectionValidation,
    saveFeedback: feedback.saveFeedback,
    saveMessage: feedback.saveMessage,
    lastEvent: feedback.lastEvent,
    ocrStateLabel: ocrState.session.state_label,
    ocrProgress: ocrState.session.progress,
    ocrResultCount: ocrState.session.results.length,
    ocrAvailable,
    ocrApplyable,
    ocrProvider: activeOcrResult?.provider || ocrState.provider,
    ocrConfidence: activeOcrResult?.confidence ?? null,
    ocrActiveIndex: ocrState.activeIndex,
    ocrActiveText: activeOcrResult?.text ?? '',
    // 第21章 Search 尚未接入：StatusBar 读取此投影显示「搜索未就绪」。
    searchStateLabel: '搜索未就绪',
  }), [
    mode, theme, state.activeDocumentId, state.currentPage, state.pageCount, state.zoom,
    editorState.capability, editorState.selection, editorState.editing, editorState.hover,
    state.canUndo, state.canRedo, state.dirty, editorState.pendingPatches, editorState.revision,
    parserText, saveText, state.taskEvent, state.error,
    toolLabel, hoverPriority, selectionValidation, feedback.saveFeedback, feedback.saveMessage, feedback.lastEvent,
    ocrState.session.state_label, ocrState.session.progress, ocrState.session.results.length, ocrAvailable, ocrApplyable,
    ocrState.provider, activeOcrResult?.provider, activeOcrResult?.confidence, activeOcrResult?.text,
    ocrState.activeIndex,
  ]);

  // ---- FrameworkActions：意图转发，注册表只声明不持有 ViewModel ----
  const refreshEditor = useCallback(() => {
    const id = state.activeDocumentId;
    if (id !== null) void editor.refresh(id);
  }, [editor, state.activeDocumentId]);

  /** 命令成功后拉取增量事件：真值仍来自命令快照，事件只驱动反馈。 */
  const pollFeedback = useCallback(() => void eventBridge.poll(), [eventBridge]);

  /** 提交结果统一回写两个投影：EditorState（编辑态）与 ViewModel（壳层脏标记）。 */
  const acceptCommit = useCallback((info: CommitInfo | null) => {
    if (!info) {
      push(ocr.snapshot.error || '提交失败', 'error');
      return;
    }
    editor.applySnapshot(info.snapshot);
    viewModel.applyEditorSnapshot(info.snapshot);
    pollFeedback();
    push(`已提交编辑：v${info.before_version} → v${info.after_version}，改写 ${info.operators} 个 Operator`, 'success');
  }, [editor, viewModel, ocr, pollFeedback, push]);

  /** OCR 启动：只把意图交给 Controller，完成/失败统一走 Toast。 */
  const startOcr = useCallback(async (pages: number[], region: [number, number, number, number] | null) => {
    const id = state.activeDocumentId;
    if (id === null) {
      push('请先打开文档，再执行 OCR', 'error');
      return;
    }
    await ocr.start(id, pages, region);
    const session = ocr.snapshot.session;
    if (session.state === 'Failed') {
      push(session.error || 'OCR 识别失败', 'error');
    } else if (session.state === 'Completed') {
      push(`OCR 完成：${session.results.length} 条结果`, 'success');
    }
  }, [state.activeDocumentId, ocr, push]);

  /** OCR 控制命令的异常统一提示，不把 ErrorPanel 写成第二套 Toast。 */
  const checkOcrError = useCallback(() => {
    const error = ocr.snapshot.error;
    if (error) push(error, 'error');
  }, [ocr, push]);

  const actions = useMemo<FrameworkActions>(() => ({
    open: () => void viewModel.openFile().then(pollFeedback),
    // Ctrl+F：切到唯一 Search Workspace 并展开侧栏，不新增第二套搜索 UI。
    openGlobalSearch: () => {
      setWorkspace('search');
      setNavCollapsed(false);
    },
    // Dashboard 最近文件 / 拖入：同一条 Open 链路，只是路径已知。
    openPath: (path) => void viewModel.openPath(path).then(pollFeedback),
    save: () => {
      eventBridge.markSaving();
      void viewModel.save().then(() => {
        refreshEditor();
        pollFeedback();
        // 命令已返回：清 pending，避免后端不写盘（Clean 跳过）时永久停在“保存中…”。
        eventBridge.settleSave(viewModel.snapshot.dirty);
      });
    },
    saveAs: () => {
      eventBridge.markSaving();
      void viewModel.saveAs().then(() => {
        refreshEditor();
        pollFeedback();
        eventBridge.settleSave(viewModel.snapshot.dirty);
      });
    },
    undo: () => void viewModel.undo().then(() => {
      refreshEditor();
      pollFeedback();
    }),
    redo: () => void viewModel.redo().then(() => {
      refreshEditor();
      pollFeedback();
    }),
    rotate: (delta) => void viewModel.rotate(delta).then(pollFeedback),
    deletePages: () => void viewModel.deleteSelectedPages().then(pollFeedback),
    zoomIn: () => viewModel.zoomIn(),
    zoomOut: () => viewModel.zoomOut(),
    fitWidth: () => viewerRef.current?.fitWidth(),
    actualSize: () => viewModel.setZoom(1),
    fullscreen: () => {
      if (document.fullscreenElement) void document.exitFullscreen();
      else void document.documentElement.requestFullscreen?.();
    },
    goToPage: (page) => {
      viewModel.goToPage(page);
      viewerRef.current?.goToPage(page);
    },
    nextPage: () => {
      viewModel.nextPage();
      viewerRef.current?.goToPage(Math.min(state.pageCount, state.currentPage + 1));
    },
    prevPage: () => {
      viewModel.prevPage();
      viewerRef.current?.goToPage(Math.max(1, state.currentPage - 1));
    },
    setMode: (toolId) => {
      const tool = desktopFramework.registries.tools.get(toolId);
      if (!tool?.enabled(ctx)) return;
      setModeState(toolId);
      editor.setMode(toolId as EditorMode);
    },
    setTheme: (next) => setTheme(next),
    beginEdit: () => {
      const primary = editorState.selection.primary;
      if (primary === null) {
        push('请先选中一个文本对象', 'error');
        return;
      }
      void (async () => {
        const info = await editor.inspector(primary);
        await editor.beginEdit(primary, info?.text?.unicode ?? '');
        // 只有 Core 真正确认进入编辑态才切换模式，避免灰按钮/假编辑态。
        const editing = editor.snapshot.editing;
        if (editing.active && editing.objectId === primary) {
          setModeState('edit');
          editor.setMode('edit');
        } else {
          push(editor.snapshot.error || '无法进入编辑：请确认主选是文本对象', 'error');
        }
      })();
    },
    commit: () => void editor.commit().then(acceptCommit),
    cancel: () => void editor.cancel().then(pollFeedback),
    replaceText: (text) => void editor.commit(text).then(acceptCommit),
    selectAll: () => void editor.selectAll(editorState.page).then(pollFeedback),
    clearSelection: () => void editor.clearSelection(editorState.page).then(pollFeedback),

    // ---- 第19章：OCR 动作（全部转发 OcrController） ----
    ocrRecognize: (scope) => {
      if (scope === 'full') {
        void startOcr([], null);
        return;
      }
      if (scope === 'pages') {
        void startOcr([state.currentPage], null);
        return;
      }
      const page = state.objectTree?.pages.find((item) => item.page === state.currentPage);
      if (!page) {
        push('当前页缺少尺寸投影，无法执行区域 OCR', 'error');
        return;
      }
      void startOcr([], [0, 0, page.page_width, page.page_height]);
    },
    ocrPause: () => void ocr.pause().then(checkOcrError),
    ocrResume: () => void ocr.resume().then(checkOcrError),
    ocrCancel: () => void ocr.cancel().then(checkOcrError),
    ocrApply: (index) => void ocr.apply(index).then(acceptCommit),
    ocrApplyActive: () => void ocr.apply(ocrState.activeIndex ?? 0).then(acceptCommit),
    searchInOcr: () => {
      setWorkspace('ocr');
      setRibbonTab('ocr');
      recordDashboard('search', {
        id: `search:${Date.now()}`,
        title: 'OCR 结果内查找',
        detail: `${ctx.ocrResultCount} 条结果`,
      });
    },
    ocrCopyResult: () => {
      // 优先复制当前激活项；没有激活项才回落到首条。
      const index = ocrState.activeIndex;
      const text = (index !== null ? ocr.results[index]?.text : ocr.results[0]?.text) ?? '';
      if (!text) {
        push('没有可复制的 OCR 结果', 'error');
        return;
      }
      const clipboard = navigator.clipboard;
      if (!clipboard) {
        push('当前环境不支持剪贴板', 'error');
        return;
      }
      const label = index !== null ? `第 ${index + 1} 条` : '首条';
      void clipboard.writeText(text)
        .then(() => push(`已复制${label} OCR 文本`, 'success'))
        .catch(() => push('复制失败：剪贴板不可用', 'error'));
    },
  }), [
    viewModel, editor, refreshEditor, pollFeedback, eventBridge, push, acceptCommit, startOcr, checkOcrError,
    ocr, ocrState.activeIndex, state.pageCount, state.currentPage, state.objectTree,
    editorState.selection.primary, editorState.page, ctx,
  ]);

  // ---- 画布回调：命中走 Controller，不自行解析 ----
  const handlePageCount = useCallback((count: number) => viewModel.reportPageCount(count), [viewModel]);
  const handlePageChange = useCallback((page: number) => viewModel.goToPage(page), [viewModel]);
  const handleScroll = useCallback((page: number, scrollTop: number) => viewModel.reportScroll(page, scrollTop), [viewModel]);
  const handleFitZoom = useCallback((zoom: number) => viewModel.reportFitZoom(zoom), [viewModel]);
  const handleError = useCallback((message: string) => push(`渲染失败：${message}`, 'error'), [push]);

  const handlePointerMove = useCallback((page: number, x: number, y: number, widthPx: number, rotation: number) => {
    void editor.hitTest(page, x, y, widthPx, rotation);
  }, [editor]);
  const handlePointerLeave = useCallback(() => editor.clearHover(), [editor]);

  const handlePageClick = useCallback((
    page: number, x: number, y: number, widthPx: number, rotation: number,
    mods: { ctrl: boolean; shift: boolean },
  ) => {
    void (async () => {
      const hit = await editor.hitTest(page, x, y, widthPx, rotation);
      if (hit) {
        await editor.select(page, hit.object_id, { multi: mods.ctrl || mods.shift, ctrl: mods.ctrl, shift: mods.shift });
      } else {
        await editor.clearSelection(page);
      }
    })();
  }, [editor]);

  const handlePageDoubleClick = useCallback((page: number, x: number, y: number, widthPx: number, rotation: number) => {
    void (async () => {
      const hit = await editor.hitTest(page, x, y, widthPx, rotation);
      if (!hit || hit.kind !== 'Text') return;
      const info = await editor.inspector(hit.object_id);
      // Core 的 begin_edit 要求主选就是被编辑对象：先确认选择已生效，再开事务。
      await editor.select(page, hit.object_id);
      if (editor.snapshot.selection.primary !== hit.object_id) {
        push(editor.snapshot.error || '对象选择失败，无法进入编辑', 'error');
        return;
      }
      await editor.beginEdit(hit.object_id, info?.text?.unicode ?? '');
      const editing = editor.snapshot.editing;
      if (editing.active && editing.objectId === hit.object_id) {
        setModeState('edit');
        editor.setMode('edit');
      } else {
        push(editor.snapshot.error || '无法进入编辑：请确认主选是文本对象', 'error');
      }
    })();
  }, [editor, push]);

  /**
   * 18 章框选：Core 没有批量选择 IPC，沿用 editor.select 串行调用。
   * 先选最后一个再倒序追加，使 Core 的「primary = 最后命中」最终落在框选第一个对象。
   * 只在全部完成后 pollFeedback 一次，避免滚动 / 命令事件反复拉取。
   */
  const handleFrameSelect = useCallback((page: number, objectIds: string[]) => {
    void (async () => {
      await editor.clearSelection(page);
      if (objectIds.length === 0) {
        pollFeedback();
        return;
      }
      for (let index = objectIds.length - 1; index >= 0; index -= 1) {
        const objectId = objectIds[index];
        if (index === objectIds.length - 1) {
          await editor.select(page, objectId);
        } else {
          await editor.select(page, objectId, { multi: true, shift: true });
        }
      }
      pollFeedback();
    })();
  }, [editor, pollFeedback]);

  // ---- 快捷键：遍历 ShortcutRegistry，无 switch ----
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const parts: string[] = [];
      if (event.ctrlKey || event.metaKey) parts.push('ctrl');
      if (event.shiftKey) parts.push('shift');
      if (event.altKey) parts.push('alt');
      let key = event.key.toLowerCase();
      if (key === ' ') key = 'space';
      parts.push(key);
      const combo = parts.join('+');
      const bindings = desktopFramework.registries.shortcuts.sorted();
      for (const binding of bindings) {
        if (binding.combo !== combo) continue;
        if (binding.enabled && !binding.enabled(ctx)) continue;
        event.preventDefault();
        binding.run(actions);
        return;
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [ctx, actions]);

  // ---- 投影：对象树 → Overlay 几何（不是 Object 真值） ----
  const overlayPages = useMemo<Record<number, PageOverlayData>>(() => {
    const map: Record<number, PageOverlayData> = {};
    for (const page of state.objectTree?.pages ?? []) {
      const objects: OverlayObject[] = page.objects.map((object) => ({
        objectId: object.object_id,
        bbox: object.bbox,
      }));
      map[page.page] = {
        pageWidth: page.page_width,
        pageHeight: page.page_height,
        objects,
      };
    }
    // 第19章：OCR 结果按页投影到同一 Overlay Stack，不建第二套覆盖层。
    ocrState.session.results.forEach((result, resultIndex) => {
      const data = map[result.page];
      if (!data) return;
      (data.ocrResults ??= []).push({
        // 全局序号：Overlay 审核 / 应用回调不能再用页内局部 index。
        resultIndex,
        page: result.page,
        bbox: result.bbox,
        confidence: result.confidence,
        text: result.text,
      });
    });
    return map;
  }, [state.objectTree, ocrState.session.results]);

  const hasDocument = state.activeDocumentId !== null;

  // 第19章：Busy / Forbidden 也走同一个 data-cursor 出口，不新增第二套光标管理。
  const resolvedCursor = ocrState.session.state === 'Running'
    ? 'busy'
    : desktopFramework.registries.tools.get(mode)?.enabled(ctx) === false
      ? 'forbidden'
      : desktopFramework.resolveCursor(mode);
  // 18 章：浏览 / 选择工具下 Hover 到对象时，按同一 CursorManager 结果叠加 move 语义。
  const cursorKind = (resolvedCursor === 'arrow' || resolvedCursor === 'cross') && editorState.hover
    ? 'move'
    : resolvedCursor;

  // ---- 右键菜单项：来自 ContextMenuRegistry ----
  const menuItems = useMemo<MenuItemView[]>(() => {
    if (!contextMenu) return [];
    return desktopFramework.contextItems(ctx, actions) as MenuItemView[];
  }, [contextMenu, ctx, actions]);

  const inspectorDocument: InspectorDocumentInfo = useMemo(() => ({
    title: state.activeTitle,
    author: state.author,
    pageCount: state.pageCount,
    fileSize: state.fileSize,
    path: state.sourcePath,
    dirty: state.dirty,
    lastSavedAt: state.lastSavedAt,
    parserText,
  }), [state.activeTitle, state.author, state.pageCount, state.fileSize, state.sourcePath, state.dirty, state.lastSavedAt, parserText]);

  // 工作区面板：分类固定十类，未来分类由 workspaces.ts 的 enabled 控制。
  const activePanel = (() => {
    switch (workspace) {
      case 'pages':
        return (
          <PagesPanel
            hasDocument={hasDocument}
            pageOrder={state.pageOrder}
            rotations={state.rotations}
            selectedPages={state.selectedPages}
            currentPage={state.currentPage}
            layoutRevision={state.layoutRevision}
            onSelectPages={(pages, selectMode) => void viewModel.selectPages(pages, selectMode)}
            onMovePage={(from, to) => void viewModel.movePage(from, to)}
            onClearSelection={() => void viewModel.clearSelection()}
          />
        );
      case 'objects':
        return (
          <ObjectsPanel
            hasDocument={hasDocument}
            tree={state.objectTree}
            loading={state.objectTreeLoading}
            error={state.objectTreeError}
            focusedObjectId={focusedObject?.object_id ?? null}
            expanded={expandedNodes}
            onToggle={(id) =>
              setExpandedNodes((current) =>
                current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
              )
            }
            onFocusObject={setFocusedObject}
            onLocatePage={(page) => actions.goToPage(page)}
          />
        );
      case 'bookmarks': return <BookmarksPanel />;
      case 'outline': return <OutlinePanel />;
      case 'attachments': return <AttachmentsPanel />;
      case 'ocr':
        return (
          <OcrWorkspace
            state={ocrState}
            hasDocument={hasDocument}
            onStart={(pages, region) => void startOcr(pages, region)}
            onProvider={(name) => ocr.setProvider(name)}
            onPause={() => void ocr.pause().then(checkOcrError)}
            onResume={() => void ocr.resume().then(checkOcrError)}
            onCancel={() => void ocr.cancel().then(checkOcrError)}
            onDecide={(index, accept) => void ocr.decide(index, accept)}
            onApply={(index) => void ocr.apply(index).then(acceptCommit)}
            onClearCache={() => void ocr.clearCache()}
            activeIndex={ocrState.activeIndex}
            onActivate={(index) => ocr.setActiveIndex(index)}
          />
        );
      case 'annotation': return <AnnotationPanel />;
      case 'ai': return <AiPanel />;
      case 'search': return <SearchPanel />;
      case 'plugin': return <PluginPanel />;
      case 'settings': return <SettingsPanel />;
      case 'collaboration': return <CollaborationPanel />;
      default: return null;
    }
  })();

  return (
    <div className="dw-app">
      <TitleBar
        appTitle={state.title}
        documentTitle={hasDocument ? state.activeTitle : ''}
        dirty={state.dirty}
        edition={state.edition}
        version={state.version}
        recoveryAvailable={state.recoveryAvailable}
      />

      <Ribbon
        ctx={ctx}
        actions={actions}
        activeTab={ribbonTab}
        onTabChange={setRibbonTab}
        theme={theme}
      />

      <div className="dw-main">
        <WorkspaceNav
          active={workspace}
          onActiveChange={setWorkspace}
          collapsed={navCollapsed}
          onToggleCollapsed={() => setNavCollapsed((value) => !value)}
          width={navWidth}
          onWidthChange={setNavWidth}
        >
          {activePanel}
        </WorkspaceNav>

        <div className="dw-center">
          <DocumentTabs
            tabs={state.tabs}
            activeDocumentId={state.activeDocumentId}
            onSelect={(documentId) => void viewModel.selectDocument(documentId)}
            onClose={(documentId) => void viewModel.closeTab(documentId)}
          />
          <div className="dw-canvas">
            {hasDocument ? (
              <PdfViewer
                ref={viewerRef}
                documentId={state.activeDocumentId}
                zoom={state.zoom}
                currentPage={state.currentPage}
                onPageCount={handlePageCount}
                onPageChange={handlePageChange}
                onScroll={handleScroll}
                onFitZoom={handleFitZoom}
                onError={handleError}
                pageOrder={state.pageOrder}
                rotations={state.rotations}
                selectedPages={state.selectedPages}
                layoutRevision={state.layoutRevision}
                saveRevision={state.saveRevision}
                onSelectPage={(page, selectMode) => void viewModel.selectPages([page], selectMode)}
                overlayPages={overlayPages}
                hoverObjectId={editorState.hover?.object_id ?? null}
                selectedObjectIds={editorState.selection.selected}
                editingObjectId={editorState.editing.objectId}
                editingText={inspector?.text?.unicode ?? ''}
                onCommitText={(text) => actions.replaceText(text)}
                onCancelEdit={() => actions.cancel()}
                hoverKind={editorState.hover?.kind_label ?? ''}
                hoverPriority={hoverPriority}
                primaryObjectId={editorState.selection.primary}
                cursorKind={cursorKind}
                toolMode={mode}
                onFrameSelect={handleFrameSelect}
                onNotify={(text) => push(text, 'info')}
                ocrReviews={ocrState.session.reviews}
                ocrActiveIndex={ocrState.activeIndex}
                ocrCanApply={ocrCanApply}
                onOcrActivate={(index) => ocr.setActiveIndex(index)}
                onOcrDecide={(index, accept) => void ocr.decide(index, accept).then(checkOcrError)}
                onOcrApply={(index) => void ocr.apply(index).then(acceptCommit)}
                onPagePointerMove={handlePointerMove}
                onPagePointerLeave={handlePointerLeave}
                onPageClick={handlePageClick}
                onPageDoubleClick={handlePageDoubleClick}
                onPageContextMenu={(x, y) => setContextMenu({ x, y })}
              />
            ) : (
              <Dashboard actions={actions} />
            )}
            <ToastStack toasts={toasts} onDismiss={dismiss} />
          </div>
        </div>

        <Inspector
          ctx={ctx}
          actions={actions}
          hasDocument={hasDocument}
          document={inspectorDocument}
          inspector={inspector}
          focusedPageRotation={inspector ? (state.rotations[inspector.page] ?? 0) : null}
          formatSize={(bytes) => (bytes > 0 ? formatSize(bytes) : '—')}
        />
      </div>

      <StatusBar ctx={ctx} actions={actions} taskEvent={state.taskEvent} error={state.error} />

      {contextMenu ? (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={menuItems}
          onClose={() => setContextMenu(null)}
        />
      ) : null}
    </div>
  );
}
