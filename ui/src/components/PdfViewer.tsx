// PdfViewer：**直接封装 PDF.js**，不自绘 Canvas、不自写 PDF 解析（`00` §9）。
//
// 职责边界：PDF.js 属 UI，不是 Driver。Rust 只提供文件字节与元数据，
// 页面渲染完全在 WebView 内完成，不产生跨 IPC 位图。
//
// 连续滚动策略：每页一个占位 div，进入视口才真正渲染 canvas。
// 这样 1000 页文档的首屏仍然只渲染 1~2 页，不会一次画完所有页。
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';
import { GlobalWorkerOptions, getDocument, type PDFDocumentProxy } from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { readDocumentBytes } from '../ipc';
import { PageOverlays, projectBox, type OverlayObject } from './overlay';

GlobalWorkerOptions.workerSrc = workerUrl;

// 中文 PDF 普遍用 CMap 编码（UniGB-UCS2-H 等），缺 cmaps 会整页乱码；
// 非嵌入字体的标准 14 字体也需要 standard_fonts 才能正确度量。
// 两者随绿色版一起打包在 pdfjs/ 下，随前端根路径解析。
const ASSET_BASE = import.meta.env.BASE_URL || '/';
const CMAP_URL = `${ASSET_BASE}pdfjs/cmaps/`;
const STANDARD_FONT_URL = `${ASSET_BASE}pdfjs/standard_fonts/`;

/** 基准渲染比例：PDF 的 1pt 在 96dpi 下约 1.333px，取 1.25 兼顾清晰与体积。 */
const BASE_SCALE = 1.25;

/** 页与页之间的间距（像素）。 */
const PAGE_GAP = 16;

/** 拖拽超过该像素数才算框选，低于阈值仍按单击处理。 */
const FRAME_THRESHOLD_PX = 4;

/** 框选矩形：相对当前页槽的百分比坐标（0..100）。 */
interface FrameRect {
  page: number;
  left: number;
  top: number;
  width: number;
  height: number;
}

/** 拖拽中的起点 / 当前点与是否越过阈值。 */
interface DragState {
  page: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  active: boolean;
}

/** 把拖拽像素轨迹归一化为页内百分比矩形。 */
function dragFrame(drag: DragState, widthPx: number, heightPx: number): FrameRect {
  const safeWidth = widthPx > 0 ? widthPx : 1;
  const safeHeight = heightPx > 0 ? heightPx : 1;
  const clamp = (value: number) => Math.max(0, Math.min(100, value));
  const left = clamp((Math.min(drag.startX, drag.x) / safeWidth) * 100);
  const top = clamp((Math.min(drag.startY, drag.y) / safeHeight) * 100);
  const right = clamp((Math.max(drag.startX, drag.x) / safeWidth) * 100);
  const bottom = clamp((Math.max(drag.startY, drag.y) / safeHeight) * 100);
  return {
    page: drag.page,
    left,
    top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}

/**
 * 框选命中：不改 Rust / IPC，直接在前端对 Overlay BBox 投影求交。
 * bbox 是 PDF 点坐标（左下原点），projectBox 已处理旋转，这里统一用屏幕百分比比较。
 */
function framedObjectIds(
  pageData: PageOverlayData | undefined,
  rotation: number,
  rect: FrameRect,
): number[] {
  if (!pageData) {
    return [];
  }
  const ids: number[] = [];
  for (const object of pageData.objects) {
    const box = projectBox(object.bbox, pageData.pageWidth, pageData.pageHeight, rotation);
    const intersects =
      rect.left < box.left + box.width
      && rect.left + rect.width > box.left
      && rect.top < box.top + box.height
      && rect.top + rect.height > box.top;
    if (intersects) {
      ids.push(object.objectId);
    }
  }
  return ids;
}

/** Viewer 对外暴露的命令式接口：跳页与适应宽度需要知道滚动容器的实时尺寸。 */
export interface PdfViewerHandle {
  /** 滚动到指定页。 */
  goToPage(page: number): void;
  /** 按容器宽度计算缩放比例并回报。 */
  fitWidth(): void;
}

/** 单页 Overlay 数据：对象树投影里属于该页的几何与对象。 */
export interface PageOverlayData {
  /** 页宽（PDF 点）。 */
  pageWidth: number;
  /** 页高（PDF 点）。 */
  pageHeight: number;
  /** 本页参与高亮的对象。 */
  objects: OverlayObject[];
  /** 第19章：本页 OCR 结果投影（Overlay BBox + confidence）。 */
  ocrResults?: Array<{
    /** 全局 OCR 结果序号，Overlay 审核 / 应用回调必须使用。 */
    resultIndex: number;
    page: number;
    bbox: [number, number, number, number];
    confidence: number;
    text: string;
  }>;
}

/** Viewer 属性：所有状态由外部（ViewModel）传入，Viewer 不持有真值。 */
interface PdfViewerProps {
  /** 活动文档 id；为 null 时不加载。 */
  documentId: number | null;
  /** 缩放比例，来自 view_state。 */
  zoom: number;
  /** 当前页，来自 view_state。 */
  currentPage: number;
  /** 页数变化回调。 */
  onPageCount: (count: number) => void;
  /** 滚动导致当前页变化。 */
  onPageChange: (page: number) => void;
  /** 滚动位置上报（页码 + 偏移）。 */
  onScroll: (page: number, scrollTop: number) => void;
  /** 适应宽度算出的比例回报。 */
  onFitZoom: (zoom: number) => void;
  /** 渲染或加载失败。 */
  onError: (message: string) => void;
  /** 当前显示顺序（原始页码）；为空时按自然页序。 */
  pageOrder: number[];
  /** 原始页码 → 顺时针旋转角。 */
  rotations: Record<number, number>;
  /** 选中的显示位置（1 起）。 */
  selectedPages: number[];
  /** 页结构版本：变化即强制重渲染，缩略图随之刷新。 */
  layoutRevision: number;
  /** 保存版本：变化即重新读取文件字节，否则看不到写盘结果。 */
  saveRevision: number;
  /** 点击页面写入 Session.selection。 */
  onSelectPage: (page: number, mode: 'replace' | 'toggle' | 'range') => void;
  /** 逐页 Overlay 数据（键为原始页码）；为空时不画对象框。 */
  overlayPages?: Record<number, PageOverlayData>;
  /** Hover 对象 id（来自 hit_test 投影）。 */
  hoverObjectId?: number | null;
  /** 选中对象 id 列表（Selection 真值在 Session）。 */
  selectedObjectIds?: number[];
  /** 正在编辑的对象 id：显示 Caret。 */
  editingObjectId?: number | null;
  /** 18C：编辑对象当前文本（Inline Editor 预填）。 */
  editingText?: string;
  /** 18C：Inline Editor 提交。 */
  onCommitText?: (text: string) => void;
  /** 18C：Inline Editor 取消。 */
  onCancelEdit?: () => void;
  /** 18C：Hover 对象种类与优先级（命中反馈）。 */
  hoverKind?: string;
  hoverPriority?: string;
  /** 18C：Primary 选中对象 id。 */
  primaryObjectId?: number | null;
  /** 光标种类：唯一来源是 CursorManager.resolve(toolId)，组件不得自行设置。 */
  cursorKind?: string;
  /** 18 章：当前工具 id，交给 Overlay 手柄决定 Hover 光标。 */
  toolMode?: string;
  /** 18 章：框选结束；objectIds 为空表示空框选，由 App 清选择。 */
  onFrameSelect?: (page: number, objectIds: number[]) => void;
  /** 18 章：Inline Editor 失焦轻提示，交给 App 唯一 ToastStack。 */
  onNotify?: (text: string) => void;
  /** 第19章：OCR Review 决策只读投影。 */
  ocrReviews?: Array<{ result_index: number; decision: string }>;
  /** 第19章：当前激活 OCR 结果序号。 */
  ocrActiveIndex?: number | null;
  /** 第19章：逐结果可 Apply 标记。 */
  ocrCanApply?: boolean[];
  /** 第19章：点击 OCR 框设为当前结果。 */
  onOcrActivate?: (index: number) => void;
  /** 第19章：Overlay 接受 / 拒绝。 */
  onOcrDecide?: (index: number, accept: boolean) => void;
  /** 第19章：Overlay 应用（仍走 Editing Engine）。 */
  onOcrApply?: (index: number) => void;
  /** 右键：交给 App 从 ContextMenuRegistry 收集菜单项。 */
  onPageContextMenu?: (x: number, y: number) => void;
  /** 指针在页面内移动：交给 Controller 做命中测试。 */
  onPagePointerMove?: (
    page: number,
    x: number,
    y: number,
    pageWidthPx: number,
    rotation: number,
  ) => void;
  /** 指针离开页面：清 Hover。 */
  onPagePointerLeave?: () => void;
  /** 单击页面：交给 Controller 命中后写 Selection。 */
  onPageClick?: (
    page: number,
    x: number,
    y: number,
    pageWidthPx: number,
    rotation: number,
    mods: { ctrl: boolean; shift: boolean },
  ) => void;
  /** 双击页面：交给 Controller 进入编辑。 */
  onPageDoubleClick?: (
    page: number,
    x: number,
    y: number,
    pageWidthPx: number,
    rotation: number,
  ) => void;
}

/** 单页 canvas：进入视口才渲染，离开后保留画布避免反复重绘。 */
function PageCanvas({
  pdf,
  pageNumber,
  width,
  height,
  rotation,
}: {
  pdf: PDFDocumentProxy;
  pageNumber: number;
  width: number;
  height: number;
  rotation: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [visible, setVisible] = useState(false);
  const [rendered, setRendered] = useState(false);

  // 进入视口才标记为可见：IntersectionObserver 让长文档只画当前页附近。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setVisible(true);
          }
        }
      },
      { rootMargin: '400px 0px' },
    );
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!visible) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const page = await pdf.getPage(pageNumber);
        if (cancelled) {
          return;
        }
        const canvas = canvasRef.current;
        if (!canvas) {
          return;
        }
        const ratio = window.devicePixelRatio || 1;
        const unrotated = page.getViewport({ scale: 1 });
        const basis = rotation % 180 === 0 ? unrotated.width : unrotated.height;
        const viewport = page.getViewport({ scale: width / basis, rotation });
        canvas.width = Math.floor(viewport.width * ratio);
        canvas.height = Math.floor(viewport.height * ratio);
        canvas.style.width = `${Math.floor(viewport.width)}px`;
        canvas.style.height = `${Math.floor(viewport.height)}px`;
        const context = canvas.getContext('2d');
        if (!context) {
          return;
        }
        context.setTransform(ratio, 0, 0, ratio, 0, 0);
        // pdfjs 6 的 RenderParameters 同时要求 canvas 与 canvasContext。
        await page.render({ canvas, canvasContext: context, viewport }).promise;
        if (!cancelled) {
          setRendered(true);
        }
      } catch {
        // 单页渲染失败不打断整篇文档：占位区保留，用户仍可翻到其他页。
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [visible, pdf, pageNumber, width, rotation]);

  return (
    <div className="pdf-page" style={{ width, height }}>
      <canvas ref={canvasRef} className={rendered ? 'pdf-canvas' : 'pdf-canvas pending'} />
      <span className="pdf-page-number">{pageNumber}</span>
    </div>
  );
}

/** PDF 阅读器：连续滚动 + 缩放 + 跳页。 */
export const PdfViewer = forwardRef<PdfViewerHandle, PdfViewerProps>(function PdfViewer(
  {
    documentId,
    zoom,
    currentPage,
    onPageCount,
    onPageChange,
    onScroll,
    onFitZoom,
    onError,
    pageOrder,
    rotations,
    selectedPages,
    layoutRevision,
    saveRevision,
    onSelectPage,
    overlayPages,
    hoverObjectId = null,
    selectedObjectIds = [],
    editingObjectId = null,
    editingText = '',
    onCommitText,
    onCancelEdit,
    hoverKind = '',
    hoverPriority = '',
    primaryObjectId = null,
    cursorKind = 'arrow',
    toolMode = '',
    onFrameSelect,
    onNotify,
    ocrReviews = [],
    ocrActiveIndex = null,
    ocrCanApply = [],
    onOcrActivate,
    onOcrDecide,
    onOcrApply,
    onPageContextMenu,
    onPagePointerMove,
    onPagePointerLeave,
    onPageClick,
    onPageDoubleClick,
  },
  ref,
) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const pageRefs = useRef<Array<HTMLDivElement | null>>([]);
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  const [pageCount, setPageCount] = useState(0);
  const [baseSize, setBaseSize] = useState({ width: 595, height: 842 });
  const [loading, setLoading] = useState(false);
  /** 上次自适应计算出的可用宽度：避免手动缩放被反复覆盖。 */
  const fittedWidthRef = useRef(0);
  /** 窗口尺寸变化时递增，触发重新自适应。 */
  const [resizeTick, setResizeTick] = useState(0);
  /** 当前 PDF.js 加载任务：切换文档 / 保存后重新读字节时先销毁旧任务。 */
  const loadingTaskRef = useRef<ReturnType<typeof getDocument> | null>(null);
  /** 18 章框选：拖拽状态放 ref，避免 pointermove 每帧触发重渲染。 */
  const dragRef = useRef<DragState | null>(null);
  const [frameRect, setFrameRect] = useState<FrameRect | null>(null);
  /** 框选结束后短暂吞掉 click，避免拖拽被当成单击选择。 */
  const suppressClickUntilRef = useRef(0);
  /** Overlay 手柄 Hover 的临时光标覆盖；始终通过 data-cursor 唯一出口下发。 */
  const [cursorOverride, setCursorOverride] = useState<string | null>(null);

  // 加载文档：拿字节 → PDF.js 解析 → 用第 1 页尺寸作为所有页的基准尺寸。
  useEffect(() => {
    if (documentId === null) {
      setPdf(null);
      setPageCount(0);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const bytes = await readDocumentBytes(documentId);
        if (cancelled) {
          return;
        }
        const task = getDocument({
          data: new Uint8Array(bytes),
          cMapUrl: CMAP_URL,
          cMapPacked: true,
          standardFontDataUrl: STANDARD_FONT_URL,
        });
        loadingTaskRef.current = task;
        const loaded = await task.promise;
        if (cancelled) {
          // 销毁要落在 loadingTask 上，PDFDocumentProxy 没有 destroy。
          void task.destroy();
          return;
        }
        const firstPage = await loaded.getPage(1);
        const viewport = firstPage.getViewport({ scale: 1 });
        setBaseSize({ width: viewport.width, height: viewport.height });
        setPdf(loaded);
        setPageCount(loaded.numPages);
        onPageCount(loaded.numPages);
        setLoading(false);
      } catch (error) {
        if (!cancelled) {
          setLoading(false);
          setPdf(null);
          setPageCount(0);
          onError(error instanceof Error ? error.message : String(error));
        }
      }
    })();
    return () => {
      cancelled = true;
      // 保存成功后要用新字节重建 PDF.js 文档，旧任务必须先销毁以免 worker 泄漏。
      void loadingTaskRef.current?.destroy();
      loadingTaskRef.current = null;
    };
    // onError / onPageCount 由父组件用稳定引用传入，不参与依赖，避免重复加载。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentId, saveRevision]);

  // 页面尺寸随缩放变化：占位高度必须同步，否则滚动定位会错。
  const pageWidth = Math.round(baseSize.width * BASE_SCALE * zoom);
  const pageHeight = Math.round(baseSize.height * BASE_SCALE * zoom);

  // 显示顺序来自后端页布局；旋转 90/270 的页宽高互换。
  const displayOrder = pageOrder.length ? pageOrder : Array.from({ length: pageCount }, (_, i) => i + 1);

  // 打开文档 / 窗口变宽时自动适应宽度：页面铺满可用区域，不留下大片空白。
  // 同一可用宽度只算一次，避免用户手动缩放后被立刻覆盖。
  useEffect(() => {
    if (documentId === null || !pdf || loading) {
      return;
    }
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const available = container.clientWidth - 48;
    if (available <= 0 || Math.abs(available - fittedWidthRef.current) < 2) {
      return;
    }
    fittedWidthRef.current = available;
    const first = displayOrder[0] ?? 1;
    const sideways = (rotations[first] ?? 0) % 180 !== 0;
    const basis = sideways ? baseSize.height : baseSize.width;
    const ratio = available / (basis * BASE_SCALE);
    onFitZoom(Math.min(8, Math.max(0.1, Number(ratio.toFixed(2)))));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentId, pdf, loading, baseSize.width, baseSize.height, resizeTick, onFitZoom]);

  // 窗口尺寸变化 / 页结构变化（旋转会让页宽高互换）时重新自适应。
  const lastRevisionRef = useRef(layoutRevision);
  useEffect(() => {
    if (layoutRevision !== lastRevisionRef.current) {
      lastRevisionRef.current = layoutRevision;
      fittedWidthRef.current = 0;
      setResizeTick((tick) => tick + 1);
    }
  }, [layoutRevision]);

  useEffect(() => {
    const onResize = () => {
      fittedWidthRef.current = 0;
      setResizeTick((tick) => tick + 1);
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // Esc 取消进行中的框选；不 preventDefault，编辑态 Esc 仍由 ShortcutRegistry 处理。
  useEffect(() => {
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || !dragRef.current) {
        return;
      }
      dragRef.current = null;
      setFrameRect(null);
      suppressClickUntilRef.current = Date.now() + 250;
    };
    window.addEventListener('keydown', onEscape);
    return () => window.removeEventListener('keydown', onEscape);
  }, []);

  const handleFramePointerDown = useCallback((
    event: ReactPointerEvent<HTMLDivElement>,
    page: number,
  ) => {
    if (event.button !== 0) {
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    dragRef.current = { page, startX: x, startY: y, x, y, active: false };
    setFrameRect(null);
    // 捕获指针：拖出页面边界仍能收到 move / up。
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }, []);

  const handleFramePointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>, page: number) => {
    const drag = dragRef.current;
    if (!drag || drag.page !== page) {
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    drag.x = event.clientX - rect.left;
    drag.y = event.clientY - rect.top;
    if (!drag.active) {
      const moved = Math.abs(drag.x - drag.startX) > FRAME_THRESHOLD_PX
        || Math.abs(drag.y - drag.startY) > FRAME_THRESHOLD_PX;
      if (!moved) {
        return;
      }
      drag.active = true;
    }
    setFrameRect(dragFrame(drag, rect.width, rect.height));
  }, []);

  const handleFramePointerUp = useCallback((
    event: ReactPointerEvent<HTMLDivElement>,
    page: number,
    rotation: number,
  ) => {
    const drag = dragRef.current;
    if (!drag || drag.page !== page) {
      return;
    }
    dragRef.current = null;
    if (!drag.active) {
      return;
    }
    suppressClickUntilRef.current = Date.now() + 250;
    const rect = event.currentTarget.getBoundingClientRect();
    const ids = framedObjectIds(overlayPages?.[page], rotation, dragFrame(drag, rect.width, rect.height));
    setFrameRect(null);
    onFrameSelect?.(page, ids);
  }, [overlayPages, onFrameSelect]);

  const handleFramePointerCancel = useCallback(() => {
    dragRef.current = null;
    setFrameRect(null);
  }, []);

  const pageBoxes = displayOrder.map((original) => {
    const rotation = rotations[original] ?? 0;
    const sideways = rotation % 180 !== 0;
    return {
      original,
      rotation,
      width: sideways ? pageHeight : pageWidth,
      height: sideways ? pageWidth : pageHeight,
    };
  });
  const offsets = pageBoxes.map((_, index) =>
    pageBoxes.slice(0, index).reduce((sum, box) => sum + box.height + PAGE_GAP, 0),
  );

  // 滚动：算出离容器顶部最近的那一页，作为当前页上报。
  const handleScroll = useCallback(() => {
    const container = containerRef.current;
    if (!container || pageBoxes.length === 0) {
      return;
    }
    const scrollTop = container.scrollTop;
    let index = 0;
    for (let i = 0; i < offsets.length; i += 1) {
      if (offsets[i] <= scrollTop + pageBoxes[i].height / 2) {
        index = i;
      }
    }
    onScroll(index + 1, scrollTop);
  }, [pageBoxes, offsets, onScroll]);

  // 外部改 currentPage（跳页按钮）时把滚动位置对齐过去，但滚动自身触发的变化不重复对齐。
  useEffect(() => {
    const container = containerRef.current;
    if (!container || offsets.length === 0) {
      return;
    }
    const index = Math.min(currentPage - 1, offsets.length - 1);
    const target = offsets[Math.max(0, index)];
    if (Math.abs(container.scrollTop - target) > pageHeight / 2) {
      container.scrollTo({ top: target });
    }
  }, [currentPage, offsets, pageHeight]);

  useImperativeHandle(
    ref,
    () => ({
      goToPage(page: number) {
        const container = containerRef.current;
        if (!container) {
          return;
        }
        const index = Math.min(Math.max(page - 1, 0), Math.max(offsets.length - 1, 0));
        container.scrollTo({ top: offsets[index] ?? 0 });
        onPageChange(page);
      },
      fitWidth() {
        const container = containerRef.current;
        if (!container) {
          return;
        }
        const available = container.clientWidth - 48;
        const ratio = available / (baseSize.width * BASE_SCALE);
        onFitZoom(Math.min(8, Math.max(0.1, Number(ratio.toFixed(2)))));
      },
    }),
    [offsets, baseSize.width, onPageChange, onFitZoom],
  );

  if (documentId === null) {
    return (
      <div className="viewer-empty">
        <div className="document-empty">尚未打开文档</div>
        <div className="document-hint">点击「开始 → 打开」，或按 Ctrl+O。</div>
        <div className="document-hint">打开后可旋转 / 删除 / 移动页面，并支持撤销重做。</div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="viewer-empty">
        <div className="document-hint">正在加载 PDF…</div>
      </div>
    );
  }

  if (!pdf) {
    return (
      <div className="viewer-empty">
        <div className="document-empty">无法显示该文档</div>
        <div className="document-hint">文件可能已损坏、加密或不是有效的 PDF。</div>
      </div>
    );
  }

  return (
    <div
      className="pdf-viewer"
      ref={containerRef}
      data-cursor={cursorOverride ?? cursorKind}
      data-framing={frameRect ? 'true' : 'false'}
      onScroll={handleScroll}
    >
      <div className="pdf-pages">
        {pageBoxes.map((box, index) => (
          <div
            key={`${layoutRevision}-${box.original}`}
            ref={(element) => {
              pageRefs.current[index] = element;
            }}
            className={selectedPages.includes(index + 1) ? 'pdf-page-slot selected' : 'pdf-page-slot'}
            onPointerDown={(event) => handleFramePointerDown(event, box.original)}
            onPointerMove={(event) => handleFramePointerMove(event, box.original)}
            onPointerUp={(event) => handleFramePointerUp(event, box.original, box.rotation)}
            onPointerCancel={handleFramePointerCancel}
            onMouseMove={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              onPagePointerMove?.(
                box.original,
                event.clientX - rect.left,
                event.clientY - rect.top,
                rect.width,
                box.rotation,
              );
            }}
            onMouseLeave={() => {
              setCursorOverride(null);
              onPagePointerLeave?.();
            }}
            onContextMenu={(event) => {
              event.preventDefault();
              onPageContextMenu?.(event.clientX, event.clientY);
            }}
            onClick={(event) => {
              // 框选结束后的 click 是浏览器补发的，不能再写一次单击选择。
              // detail > 1 是双击的第二次 click：它的异步选择会与 onDoubleClick 的
              // select → beginEdit 竞态，甚至把刚建立的选中清掉，必须跳过。
              if (event.detail > 1 || Date.now() < suppressClickUntilRef.current) {
                return;
              }
              // 页选择（Feature-02）与对象选择（17 章）并行：页级仍写 Session.selection.pages。
              const selectMode = event.shiftKey
                ? 'range'
                : event.ctrlKey || event.metaKey
                  ? 'toggle'
                  : 'replace';
              onSelectPage(index + 1, selectMode);
              const rect = event.currentTarget.getBoundingClientRect();
              onPageClick?.(
                box.original,
                event.clientX - rect.left,
                event.clientY - rect.top,
                rect.width,
                box.rotation,
                { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey },
              );
            }}
            onDoubleClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              onPageDoubleClick?.(
                box.original,
                event.clientX - rect.left,
                event.clientY - rect.top,
                rect.width,
                box.rotation,
              );
            }}
          >
            <PageCanvas
              pdf={pdf}
              pageNumber={box.original}
              width={box.width}
              height={box.height}
              rotation={box.rotation}
            />
            <PageOverlays
              pageWidth={overlayPages?.[box.original]?.pageWidth ?? baseSize.width}
              pageHeight={overlayPages?.[box.original]?.pageHeight ?? baseSize.height}
              rotation={box.rotation}
              objects={overlayPages?.[box.original]?.objects ?? []}
              hoverObjectId={hoverObjectId}
              selectedObjectIds={selectedObjectIds}
              editingObjectId={editingObjectId}
              editingText={editingText}
              onCommitText={onCommitText}
              onCancelEdit={onCancelEdit}
              hoverKind={hoverKind}
              hoverPriority={hoverPriority}
              primaryObjectId={primaryObjectId}
              ocrResults={overlayPages?.[box.original]?.ocrResults ?? []}
              toolMode={toolMode}
              onCursorOverride={setCursorOverride}
              notify={onNotify}
              ocrReviews={ocrReviews}
              ocrActiveIndex={ocrActiveIndex}
              ocrCanApply={ocrCanApply}
              onOcrActivate={onOcrActivate}
              onOcrDecide={onOcrDecide}
              onOcrApply={onOcrApply}
            />
            {frameRect?.page === box.original ? (
              <div
                className="dw-marquee"
                data-frame-selection="true"
                style={{
                  left: `${frameRect.left}%`,
                  top: `${frameRect.top}%`,
                  width: `${frameRect.width}%`,
                  height: `${frameRect.height}%`,
                }}
              />
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
});
