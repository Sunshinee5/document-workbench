// 工作台主界面（规范 16 六区 / 18B Framework Freeze）。
//
// 职责边界：UI 只做「展示投影 + 转发意图」。
// 18B 起：App 不写任何能力 switch；Ribbon / Inspector / StatusBar / Overlay / 快捷键 /
// 右键菜单 / 光标全部从 Framework 注册表驱动。新增 OCR 只需 registerFeature。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  DocumentTabs,
  ObjectsPanel,
  OcrPanel,
  OutlinePanel,
  PagesPanel,
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
import type { InspectorInfo, ObjectNodeInfo } from './ipc';
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
  const [inspector, setInspector] = useState<InspectorInfo | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<string[]>([]);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const { toasts, push, dismiss } = useToasts();

  useEffect(() => viewModel.subscribe(() => setState(viewModel.snapshot)), [viewModel]);
  useEffect(() => editor.subscribe(() => setEditorState(editor.snapshot)), [editor]);
  useEffect(() => eventBridge.subscribe(() => setFeedback(eventBridge.snapshot)), [eventBridge]);
  useEffect(() => {
    void viewModel.initialize().then(() => eventBridge.poll());
  }, [viewModel, eventBridge]);

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
  }, [state.activeDocumentId, editor]);

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
  }), [
    mode, theme, state.activeDocumentId, state.currentPage, state.pageCount, state.zoom,
    editorState.capability, editorState.selection, editorState.editing, editorState.hover,
    state.canUndo, state.canRedo, state.dirty, editorState.pendingPatches, editorState.revision,
    parserText, saveText, state.taskEvent, state.error,
    toolLabel, hoverPriority, selectionValidation, feedback.saveFeedback, feedback.saveMessage, feedback.lastEvent,
  ]);

  // ---- FrameworkActions：意图转发，注册表只声明不持有 ViewModel ----
  const refreshEditor = useCallback(() => {
    const id = state.activeDocumentId;
    if (id !== null) void editor.refresh(id);
  }, [editor, state.activeDocumentId]);

  /** 命令成功后拉取增量事件：真值仍来自命令快照，事件只驱动反馈。 */
  const pollFeedback = useCallback(() => void eventBridge.poll(), [eventBridge]);

  const actions = useMemo<FrameworkActions>(() => ({
    open: () => void viewModel.openFile().then(pollFeedback),
    save: () => {
      eventBridge.markSaving();
      void viewModel.save().then(() => {
        refreshEditor();
        pollFeedback();
      });
    },
    saveAs: () => {
      eventBridge.markSaving();
      void viewModel.saveAs().then(() => {
        refreshEditor();
        pollFeedback();
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
      if (primary === null) return;
      void (async () => {
        const info = await editor.inspector(primary);
        await editor.beginEdit(primary, info?.text?.unicode ?? '');
        setModeState('edit');
        editor.setMode('edit');
      })();
    },
    commit: () => void editor.commit().then((info) => {
      pollFeedback();
      if (info) push(`已提交编辑：v${info.before_version} → v${info.after_version}，改写 ${info.operators} 个 Operator`, 'success');
    }),
    cancel: () => void editor.cancel().then(pollFeedback),
    replaceText: (text) => void editor.commit(text).then((info) => {
      pollFeedback();
      if (info) push(`已提交编辑：v${info.before_version} → v${info.after_version}，改写 ${info.operators} 个 Operator`, 'success');
    }),
    selectAll: () => void editor.selectAll(editorState.page).then(pollFeedback),
    clearSelection: () => void editor.clearSelection(editorState.page).then(pollFeedback),
  }), [viewModel, editor, refreshEditor, pollFeedback, eventBridge, push, state.pageCount, state.currentPage, editorState.selection.primary, editorState.page, ctx]);

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
      await editor.select(page, hit.object_id);
      await editor.beginEdit(hit.object_id, info?.text?.unicode ?? '');
      setModeState('edit');
      editor.setMode('edit');
    })();
  }, [editor]);

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
    return map;
  }, [state.objectTree]);

  const hasDocument = state.activeDocumentId !== null;

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
      case 'ocr': return <OcrPanel />;
      case 'annotation': return <AnnotationPanel />;
      case 'ai': return <AiPanel />;
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
              cursorKind={desktopFramework.resolveCursor(mode)}
              onPagePointerMove={handlePointerMove}
              onPagePointerLeave={handlePointerLeave}
              onPageClick={handlePageClick}
              onPageDoubleClick={handlePageDoubleClick}
              onPageContextMenu={(x, y) => setContextMenu({ x, y })}
            />
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
