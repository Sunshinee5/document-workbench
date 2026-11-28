// PageOverlays：单页 Overlay Stack（规范 16 §六 / 18B AD-10）。
//
// 18B 起：层顺序与内容都来自 OverlayRegistry，本组件**不画**任何覆盖层，
// 只把只读几何投影传给注册项的 render。禁止 Canvas 直接画 Overlay。
import { Fragment } from 'react';
import { desktopFramework } from '../../framework';
import type { OverlayObject, OverlayRenderProps } from '../../framework';

export type { OverlayObject } from '../../framework';

/** 屏幕百分比矩形：兼容旧导出，算法与 builtins 保持一致。 */
export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** 保留 projectBox 供既有调用方使用（内置渲染函数已在 builtins.tsx）。 */
export function projectBox(
  bbox: [number, number, number, number],
  pageWidth: number,
  pageHeight: number,
  rotation: number,
): Box {
  const safeWidth = pageWidth > 0 ? pageWidth : 1;
  const safeHeight = pageHeight > 0 ? pageHeight : 1;
  const [x, y, width, height] = bbox;
  const u0 = x / safeWidth;
  const u1 = (x + width) / safeWidth;
  const v0 = 1 - (y + height) / safeHeight;
  const v1 = 1 - y / safeHeight;
  const corners: Array<[number, number]> = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
  const angle = ((rotation % 360) + 360) % 360;
  const mapped = corners.map(([u, v]) => {
    if (angle === 90) return [1 - v, u] as [number, number];
    if (angle === 180) return [1 - u, 1 - v] as [number, number];
    if (angle === 270) return [v, 1 - u] as [number, number];
    return [u, v] as [number, number];
  });
  const xs = mapped.map(([u]) => u);
  const ys = mapped.map(([, v]) => v);
  const left = Math.min(...xs);
  const top = Math.min(...ys);
  return {
    left: left * 100,
    top: top * 100,
    width: (Math.max(...xs) - left) * 100,
    height: (Math.max(...ys) - top) * 100,
  };
}

export interface PageOverlaysProps {
  pageWidth: number;
  pageHeight: number;
  rotation: number;
  objects: OverlayObject[];
  hoverObjectId: string | null;
  selectedObjectIds: string[];
  editingObjectId: string | null;
  editingText?: string;
  onCommitText?: (text: string) => void;
  onCancelEdit?: () => void;
  hoverKind?: string;
  hoverPriority?: string;
  primaryObjectId?: string | null;
  /** 第19章：当前页 OCR 结果投影（只读，来自 OcrController）。 */
  ocrResults?: OverlayRenderProps['ocrResults'];
  /** 18 章：当前工具 id（手柄 Hover 光标策略）。 */
  toolMode?: string;
  /** 18 章：手柄 Hover → Viewer data-cursor 覆盖。 */
  onCursorOverride?: (cursor: string | null) => void;
  /** 18 章：Inline Editor 失焦轻提示。 */
  notify?: (text: string) => void;
  /** 第19章：Review 决策只读投影。 */
  ocrReviews?: OverlayRenderProps['ocrReviews'];
  /** 第19章：当前激活结果序号。 */
  ocrActiveIndex?: number | null;
  /** 第19章：逐结果可 Apply 标记。 */
  ocrCanApply?: boolean[];
  /** 第19章：点击 OCR 框设为当前结果。 */
  onOcrActivate?: (index: number) => void;
  /** 第19章：Overlay 接受 / 拒绝。 */
  onOcrDecide?: (index: number, accept: boolean) => void;
  /** 第19章：Overlay 应用。 */
  onOcrApply?: (index: number) => void;
}

/** 单页 Overlay：按注册层顺序渲染，层内按贡献 order 排序。 */
export function PageOverlays({
  pageWidth,
  pageHeight,
  rotation,
  objects,
  hoverObjectId,
  selectedObjectIds,
  editingObjectId,
  editingText = '',
  onCommitText,
  onCancelEdit,
  hoverKind = '',
  hoverPriority = '',
  primaryObjectId = null,
  ocrResults = [],
  toolMode = '',
  onCursorOverride,
  notify,
  ocrReviews = [],
  ocrActiveIndex = null,
  ocrCanApply = [],
  onOcrActivate,
  onOcrDecide,
  onOcrApply,
}: PageOverlaysProps) {
  const layers = desktopFramework.registries.overlayLayers.sorted();
  const contributions = desktopFramework.registries.overlays.sorted();
  const renderProps = {
    pageWidth,
    pageHeight,
    rotation,
    objects,
    hoverObjectId,
    selectedObjectIds,
    editingObjectId,
    editingText,
    onCommitText,
    onCancelEdit,
    hoverPriority,
    hoverKind,
    primaryObjectId,
    ocrResults,
    toolMode,
    onCursorOverride,
    notify,
    ocrReviews,
    ocrActiveIndex,
    ocrCanApply,
    onOcrActivate,
    onOcrDecide,
    onOcrApply,
  };

  return (
    <div className="dw-overlays" aria-hidden="true">
      {layers.map((layer) => {
        const extra = layer.className?.(renderProps) ?? '';
        const classes = ['dw-layer', `dw-layer-${layer.id}`, extra].filter(Boolean).join(' ');
        return (
          <div key={layer.id} className={classes} data-overlay-layer={layer.id}>
            {contributions
              .filter((item) => item.layer === layer.id)
              .map((item) => (
                <Fragment key={item.id}>{item.render(renderProps)}</Fragment>
              ))}
          </div>
        );
      })}
    </div>
  );
}
