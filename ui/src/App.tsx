// 工作台主界面（规范 16）：TitleBar → Ribbon → Workspace → Canvas → Inspector → StatusBar。
//
// 职责边界：UI 只做「展示投影 + 转发意图」。
// 按钮只调 ViewModel / 直接调 IPC 命令，不解析文档、不改 Document、不自己存真值。
// 六区结构永久固定：新增能力只能加 Workspace / Ribbon 页签 / Inspector 分区（§十四）。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { PdfViewer, type PageOverlayData, type PdfViewerHandle } from './components/PdfViewer';
import { Inspector } from './components/inspector';
import {
  MOUSE_MODE_LABELS,
  Ribbon,
  StatusBar,
  TitleBar,
  type MouseMode,
  type RibbonTab,
} from './components/shell';
import { ToastStack, useToasts } from './components/ui';
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
import type { ObjectNodeInfo } from './ipc';
import { applyTheme, loadTheme, saveTheme, watchSystemTheme, type ThemeMode } from './theme';
import { WorkbenchViewModel, formatSize } from './viewmodels';
import './index.css';

/** 侧栏默认宽度（像素）：与 Design Token 保持一致。 */
const NAV_DEFAULT_WIDTH = 224;
/** 低于该视口宽度自动折叠侧栏（§十二 响应式）。 */
const NAV_AUTO_COLLAPSE = 1280;

export default function App() {
  const viewModel = useMemo(() => new WorkbenchViewModel(), []);
  const [state, setState] = useState(() => viewModel.snapshot);
  const viewerRef = useRef<PdfViewerHandle | null>(null);

  // ---- 纯 UI 状态：不进 Session / Document，不产生 Command ----
  const [ribbonTab, setRibbonTab] = useState<RibbonTab>('home');
  const [workspace, setWorkspace] = useState<WorkspaceKind>('pages');
  const [navCollapsed, setNavCollapsed] = useState(() => window.innerWidth <= NAV_AUTO_COLLAPSE);
  const [navWidth, setNavWidth] = useState(NAV_DEFAULT_WIDTH);
  const [mode, setMode] = useState<MouseMode>('browse');
  const [theme, setTheme] = useState<ThemeMode>(() => loadTheme());
  /** 聚焦对象：只读导航高亮，不是 18-05 SelectionState 真值。 */
  const [focusedObject, setFocusedObject] = useState<ObjectNodeInfo | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<string[]>([]);
  const { toasts, push, dismiss } = useToasts();

  useEffect(() => viewModel.subscribe(() => setState(viewModel.snapshot)), [viewModel]);

  useEffect(() => {
    void viewModel.initialize();
  }, [viewModel]);

  // ---- 主题：CSS Variables 切换，纯 UI 偏好（C-10） ----
  useEffect(() => {
    applyTheme(theme);
    saveTheme(theme);
  }, [theme]);

  useEffect(() => watchSystemTheme(() => applyTheme(theme)), [theme]);

  // ---- 响应式：低宽度自动折叠侧栏，恢复宽度后展开 ----
  useEffect(() => {
    const onResize = () => {
      setNavCollapsed(window.innerWidth <= NAV_AUTO_COLLAPSE);
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // 切换文档时清空聚焦对象，避免 Inspector 显示上一个文档的对象。
  useEffect(() => {
    setFocusedObject(null);
  }, [state.activeDocumentId]);

  // 对象树就绪后默认展开第一页，让 Workspace 一打开就有内容。
  useEffect(() => {
    const tree = state.objectTree;
    if (!tree) {
      setExpandedNodes([]);
      return;
    }
    setExpandedNodes(tree.pages.length > 0 ? [`page:${tree.pages[0].page}`] : []);
  }, [state.objectTree]);

  // 错误统一走轻提示；同一错误只提示一次，避免重渲染刷屏。
  const lastErrorRef = useRef('');
  useEffect(() => {
    if (state.error && state.error !== lastErrorRef.current) {
      lastErrorRef.current = state.error;
      push(state.error, 'error');
    }
    if (!state.error) {
      lastErrorRef.current = '';
    }
  }, [state.error, push]);

  // ---- 画布回调：全部稳定引用，避免 Viewer 反复重载 ----
  const handlePageCount = useCallback(
    (count: number) => viewModel.reportPageCount(count),
    [viewModel],
  );
  const handlePageChange = useCallback((page: number) => viewModel.goToPage(page), [viewModel]);
  const handleScroll = useCallback(
    (page: number, scrollTop: number) => viewModel.reportScroll(page, scrollTop),
    [viewModel],
  );
  const handleFitZoom = useCallback((zoom: number) => viewModel.reportFitZoom(zoom), [viewModel]);
  const handleError = useCallback(
    (message: string) => {
      push(`渲染失败：${message}`, 'error');
    },
    [push],
  );

  // ---- 命令转发 ----
  const goToPage = useCallback(
    (page: number) => {
      viewModel.goToPage(page);
      viewerRef.current?.goToPage(page);
    },
    [viewModel],
  );

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) {
      void document.exitFullscreen();
    } else {
      void document.documentElement.requestFullscreen?.();
    }
  }, []);

  // 快捷键：与 Ribbon 一一对应，不引入 Ribbon 之外的命令。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (event.key === 'F11') {
        event.preventDefault();
        toggleFullscreen();
        return;
      }
      if (modifier && key === 'o') {
        event.preventDefault();
        void viewModel.openFile();
        return;
      }
      if (modifier && key === 's') {
        event.preventDefault();
        if (event.shiftKey) {
          void viewModel.saveAs();
        } else {
          void viewModel.save();
        }
        return;
      }
      if (modifier && key === 'z' && !event.shiftKey) {
        event.preventDefault();
        void viewModel.undo();
        return;
      }
      if (modifier && (key === 'y' || (key === 'z' && event.shiftKey))) {
        event.preventDefault();
        void viewModel.redo();
        return;
      }
      if (modifier && (event.key === '=' || event.key === '+')) {
        event.preventDefault();
        viewModel.zoomIn();
        return;
      }
      if (modifier && event.key === '-') {
        event.preventDefault();
        viewModel.zoomOut();
        return;
      }
      if (modifier && event.key === '0') {
        event.preventDefault();
        viewModel.setZoom(1);
        return;
      }
      if (event.key === 'PageDown') {
        event.preventDefault();
        viewModel.nextPage();
        return;
      }
      if (event.key === 'PageUp') {
        event.preventDefault();
        viewModel.prevPage();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [viewModel, toggleFullscreen]);

  // ---- 投影：对象树 → 逐页 Overlay 数据 ----
  const overlayPages = useMemo<Record<number, PageOverlayData>>(() => {
    const map: Record<number, PageOverlayData> = {};
    for (const page of state.objectTree?.pages ?? []) {
      map[page.page] = {
        pageWidth: page.page_width,
        pageHeight: page.page_height,
        objects: page.objects.map((object) => ({ objectId: object.object_id, bbox: object.bbox })),
      };
    }
    return map;
  }, [state.objectTree]);

  const hasDocument = state.activeDocumentId !== null;
  const zoomPercent = Math.round(state.zoom * 100);

  // 状态栏十项：解析器状态来自对象树投影，OCR / AI / 内存暂无数据源。
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

  const selectionText =
    state.selectedPages.length === 0
      ? 'Selection(None)'
      : `Selection(Page ${state.selectedPages.join(',')})`;

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
            onLocatePage={goToPage}
          />
        );
      case 'bookmarks':
        return <BookmarksPanel />;
      case 'outline':
        return <OutlinePanel />;
      case 'attachments':
        return <AttachmentsPanel />;
      case 'ocr':
        return <OcrPanel />;
      case 'annotation':
        return <AnnotationPanel />;
      case 'ai':
        return <AiPanel />;
      default:
        return <PagesPanel
          hasDocument={hasDocument}
          pageOrder={state.pageOrder}
          rotations={state.rotations}
          selectedPages={state.selectedPages}
          currentPage={state.currentPage}
          layoutRevision={state.layoutRevision}
          onSelectPages={(pages, selectMode) => void viewModel.selectPages(pages, selectMode)}
          onMovePage={(from, to) => void viewModel.movePage(from, to)}
          onClearSelection={() => void viewModel.clearSelection()}
        />;
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
        activeTab={ribbonTab}
        onTabChange={setRibbonTab}
        hasDocument={hasDocument}
        pageCount={state.pageCount}
        canUndo={state.canUndo}
        canRedo={state.canRedo}
        mode={mode}
        onModeChange={setMode}
        theme={theme}
        onThemeChange={setTheme}
        onOpen={() => void viewModel.openFile()}
        onSave={() => void viewModel.save()}
        onSaveAs={() => void viewModel.saveAs()}
        onUndo={() => void viewModel.undo()}
        onRedo={() => void viewModel.redo()}
        onRotate={(delta) => void viewModel.rotate(delta)}
        onDeletePages={() => void viewModel.deleteSelectedPages()}
        onZoomIn={() => viewModel.zoomIn()}
        onZoomOut={() => viewModel.zoomOut()}
        onFitWidth={() => viewerRef.current?.fitWidth()}
        onActualSize={() => viewModel.setZoom(1)}
        onFullscreen={toggleFullscreen}
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
              focusedObjectId={focusedObject?.object_id ?? null}
              mode={mode}
            />
            <ToastStack toasts={toasts} onDismiss={dismiss} />
          </div>
        </div>

        <Inspector
          hasDocument={hasDocument}
          document={{
            title: state.activeTitle,
            author: state.author,
            pageCount: state.pageCount,
            fileSize: state.fileSize,
            path: state.sourcePath,
            dirty: state.dirty,
            lastSavedAt: state.lastSavedAt,
            parserText,
          }}
          focusedObject={focusedObject}
          focusedPageRotation={
            focusedObject ? (state.rotations[focusedObject.page] ?? 0) : null
          }
          formatSize={(bytes) => (bytes > 0 ? formatSize(bytes) : '—')}
        />
      </div>

      <StatusBar
        zoomPercent={zoomPercent}
        page={state.currentPage}
        pageCount={state.pageCount}
        selectionText={selectionText}
        modeText={MOUSE_MODE_LABELS[mode]}
        dirty={state.dirty}
        ocrText="OCR 未接入"
        aiText="AI 未接入"
        memoryText="—"
        parserText={parserText}
        saveText={saveText}
        taskEvent={state.taskEvent}
        error={state.error}
        onGoToPage={goToPage}
      />
    </div>
  );
}
