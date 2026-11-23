// 工作台主界面：标题栏 + 工具栏 + 标签栏 + 阅读区 + 状态栏，全部中文。
// UI 唯一职责是展示与转发意图：按钮只调 ViewModel，不直接碰 Driver / IPC 业务命令。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { PdfViewer, type PdfViewerHandle } from './components/PdfViewer';
import { WorkbenchViewModel } from './viewmodels';
import './index.css';

export default function App() {
  const viewModel = useMemo(() => new WorkbenchViewModel(), []);
  const [state, setState] = useState(() => viewModel.snapshot);
  const viewerRef = useRef<PdfViewerHandle | null>(null);
  const [pageInput, setPageInput] = useState('1');

  useEffect(() => viewModel.subscribe(() => setState(viewModel.snapshot)), [viewModel]);

  useEffect(() => {
    void viewModel.initialize();
  }, [viewModel]);

  // 页码输入框跟随当前页；用户正在输入时不覆盖由 onChange 自己维护。
  useEffect(() => {
    setPageInput(String(state.currentPage));
  }, [state.currentPage]);

  // Viewer 回调全部用稳定引用：Viewer 以 documentId 为加载依赖，回调变动不该触发重载。
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
  const handleError = useCallback((message: string) => {
    // 渲染失败不覆盖状态栏，避免把「已打开」冲掉；记录到控制台供排查。
    console.error('PDF 渲染失败：', message);
  }, []);

  // 快捷键：Ctrl+O 打开、Ctrl+= / Ctrl+- 缩放、PageUp / PageDown 翻页。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === 'o') {
        event.preventDefault();
        void viewModel.openFile();
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
  }, [viewModel]);

  const hasDocument = state.activeDocumentId !== null;
  const zoomPercent = Math.round(state.zoom * 100);

  const submitPage = () => {
    const parsed = Number.parseInt(pageInput, 10);
    if (Number.isFinite(parsed)) {
      viewModel.goToPage(parsed);
      viewerRef.current?.goToPage(parsed);
    } else {
      setPageInput(String(state.currentPage));
    }
  };

  return (
    <div className="workbench">
      <header className="title-bar">
        <span className="title">{state.title}</span>
        <span className="edition">{state.edition}</span>
      </header>

      <div className="tool-bar">
        <button
          className="tool-button primary"
          type="button"
          onClick={() => void viewModel.openFile()}
        >
          打开 PDF
        </button>
        <span className="tool-divider" />
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewModel.prevPage()}
        >
          上一页
        </button>
        <input
          className="page-input"
          value={pageInput}
          disabled={!hasDocument}
          onChange={(event) => setPageInput(event.target.value)}
          onBlur={submitPage}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              submitPage();
            }
          }}
          aria-label="页码"
        />
        <span className="page-total">/ {state.pageCount || 0}</span>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewModel.nextPage()}
        >
          下一页
        </button>
        <span className="tool-divider" />
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewModel.zoomOut()}
        >
          −
        </button>
        <span className="zoom-label">{zoomPercent}%</span>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewModel.zoomIn()}
        >
          +
        </button>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewerRef.current?.fitWidth()}
        >
          适应宽度
        </button>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          onClick={() => viewModel.setZoom(1)}
        >
          100%
        </button>
        <span className="tool-divider" />
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          title="当前页或选中页逆时针旋转 90°"
          onClick={() => void viewModel.rotate(-90)}
        >
          左转
        </button>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          title="当前页或选中页顺时针旋转 90°"
          onClick={() => void viewModel.rotate(90)}
        >
          右转
        </button>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          title="上移当前页"
          onClick={() => void viewModel.shiftCurrent(-1)}
        >
          上移
        </button>
        <button
          className="tool-button"
          type="button"
          disabled={!hasDocument}
          title="下移当前页"
          onClick={() => void viewModel.shiftCurrent(1)}
        >
          下移
        </button>
        <button
          className="tool-button danger"
          type="button"
          disabled={!hasDocument}
          title="删除当前页或选中页（至少保留一页）"
          onClick={() => void viewModel.deleteSelectedPages()}
        >
          删除页
        </button>
      </div>

      <div className="tab-bar">
        {state.tabs.length === 0 ? (
          <span className="tab-empty">暂无打开的文档</span>
        ) : (
          state.tabs.map((tab) => (
            <div
              key={tab.documentId}
              className={tab.documentId === state.activeDocumentId ? 'tab active' : 'tab'}
              onClick={() => viewModel.selectDocument(tab.documentId)}
            >
              <span className="tab-title" title={tab.title}>
                {tab.title}
              </span>
              <button
                className="tab-close"
                type="button"
                aria-label={`关闭 ${tab.title}`}
                onClick={(event) => {
                  event.stopPropagation();
                  void viewModel.closeTab(tab.documentId);
                }}
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>

      <div className="workspace">
        <aside className="thumbnail-panel">
          <div className="panel-heading">文档信息</div>
          {hasDocument ? (
            <dl className="doc-info">
              <dt>标题</dt>
              <dd>{state.activeTitle || '—'}</dd>
              <dt>作者</dt>
              <dd>{state.author || '—'}</dd>
              <dt>页数</dt>
              <dd>{state.pageCount}</dd>
              <dt>当前页</dt>
              <dd>{state.currentPage}</dd>
              <dt>缩放</dt>
              <dd>{zoomPercent}%</dd>
            </dl>
          ) : null}
          {hasDocument ? (
            <div className="thumbnail-section">
              <div className="panel-heading">页面（可拖拽排序）</div>
              <div className="thumbnail-list">
                {state.pageOrder.map((original, index) => (
                  <div
                    key={`${state.layoutRevision}-${original}`}
                    className={
                      state.selectedPages.includes(index + 1)
                        ? 'thumbnail-item selected'
                        : 'thumbnail-item'
                    }
                    draggable
                    onDragStart={(event) => {
                      event.dataTransfer.setData('text/plain', String(index + 1));
                    }}
                    onDragOver={(event) => event.preventDefault()}
                    onDrop={(event) => {
                      event.preventDefault();
                      const from = Number(event.dataTransfer.getData('text/plain'));
                      if (Number.isFinite(from) && from > 0 && from !== index + 1) {
                        void viewModel.movePage(from, index + 1);
                      }
                    }}
                    onClick={(event) => {
                      const mode = event.shiftKey
                        ? 'range'
                        : event.ctrlKey || event.metaKey
                          ? 'toggle'
                          : 'replace';
                      void viewModel.selectPages([index + 1], mode);
                    }}
                  >
                    <span className="thumbnail-no">{index + 1}</span>
                    <span className="thumbnail-original">原第 {original} 页</span>
                    {(state.rotations[original] ?? 0) !== 0 ? (
                      <span className="thumbnail-rot">{state.rotations[original]}°</span>
                    ) : null}
                  </div>
                ))}
              </div>
              <button
                className="tool-button"
                type="button"
                onClick={() => void viewModel.clearSelection()}
              >
                清空选择
              </button>
            </div>
          ) : null}
          {!hasDocument ? (
            <div className="panel-empty">尚未打开文档</div>
          ) : null}
        </aside>

        <main className="document-area">
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
            onSelectPage={(page, mode) => void viewModel.selectPages([page], mode)}
          />
        </main>
      </div>

      <footer className="status-bar">
        <span className="status-text">{state.status}</span>
        {state.taskEvent ? <span className="task-event">{state.taskEvent}</span> : null}
        {state.error ? <span className="status-error">{state.error}</span> : null}
      </footer>
    </div>
  );
}
