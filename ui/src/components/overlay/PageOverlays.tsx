// PageOverlays：单页 Overlay Stack（规范 16 §六 / C-7）。
//
// 顺序固定：PDF → Text → Selection → Editing → Annotation → OCR → AI → Debug。
// 本组件只画 Canvas 之上、页面之内的层；PDF 渲染仍由 PDF.js 负责，绝不改 Render。
// 全部层 `pointer-events: none`：交互仍由页容器处理，避免遮挡页面点击。

/** 一个参与高亮的对象。 */
export interface OverlayObject {
  /** 领域对象标识。 */
  objectId: number;
  /** PDF 坐标 bbox `[x, y, width, height]`，原点左下。 */
  bbox: [number, number, number, number];
}

export interface PageOverlaysProps {
  /** 页宽（PDF 点）。 */
  pageWidth: number;
  /** 页高（PDF 点）。 */
  pageHeight: number;
  /** 本页旋转角（0 / 90 / 180 / 270）。 */
  rotation: number;
  /** 本页对象。 */
  objects: OverlayObject[];
  /** Hover 对象（来自 hit_test，UI 不推算）。 */
  hoverObjectId: number | null;
  /** 选中对象（Selection 真值在 Session，这里只是投影）。 */
  selectedObjectIds: number[];
  /** 正在编辑的对象：显示 Caret。 */
  editingObjectId: number | null;
}

/** 屏幕百分比矩形。 */
interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * 把 PDF bbox 映射成屏幕百分比矩形。
 *
 * PDF 原点在左下、y 向上；屏幕原点在左上、y 向下。
 * 旋转 90/270 时页面宽高互换，因此对四个角统一做旋转变换后取包络，
 * 避免为每个角度单独推导公式而出错。
 */
export function projectBox(
  bbox: [number, number, number, number],
  pageWidth: number,
  pageHeight: number,
  rotation: number,
): Box {
  const safeWidth = pageWidth > 0 ? pageWidth : 1;
  const safeHeight = pageHeight > 0 ? pageHeight : 1;
  const [x, y, width, height] = bbox;

  // 归一化到 0..1，并把 y 轴翻转到屏幕方向。
  const u0 = x / safeWidth;
  const u1 = (x + width) / safeWidth;
  const v0 = 1 - (y + height) / safeHeight;
  const v1 = 1 - y / safeHeight;

  const corners: Array<[number, number]> = [
    [u0, v0],
    [u1, v0],
    [u0, v1],
    [u1, v1],
  ];

  const angle = ((rotation % 360) + 360) % 360;
  const mapped = corners.map(([u, v]) => {
    switch (angle) {
      case 90:
        // 顺时针 90°：(u, v) → (1 - v, u)
        return [1 - v, u] as [number, number];
      case 180:
        return [1 - u, 1 - v] as [number, number];
      case 270:
        return [v, 1 - u] as [number, number];
      default:
        return [u, v] as [number, number];
    }
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

/** 单页 Overlay 层组。 */
export function PageOverlays({
  pageWidth,
  pageHeight,
  rotation,
  objects,
  hoverObjectId,
  selectedObjectIds,
  editingObjectId,
}: PageOverlaysProps) {
  return (
    <div className="dw-overlays" aria-hidden="true">
      {/* Text 层：PDF.js 文本层的占位，只读，不参与交互。 */}
      <div className="dw-layer dw-layer-text" />
      {/* Selection 层：Hover 虚线框 + Selection 实线框 + 编辑 Caret，全部只读。 */}
      <div className="dw-layer dw-layer-selection">
        {objects.map((object) => {
          const box = projectBox(object.bbox, pageWidth, pageHeight, rotation);
          const selected = selectedObjectIds.includes(object.objectId);
          const hovered = hoverObjectId === object.objectId;
          const editing = editingObjectId === object.objectId;
          if (!selected && !hovered && !editing) {
            return null;
          }
          const classes = ['dw-object-box'];
          if (selected) {
            classes.push('is-selected');
          }
          if (hovered) {
            classes.push('is-hover');
          }
          return (
            <div
              key={object.objectId}
              className={classes.join(' ')}
              style={{
                left: `${box.left}%`,
                top: `${box.top}%`,
                width: `${box.width}%`,
                height: `${box.height}%`,
              }}
            >
              {editing ? <span className="dw-caret" /> : null}
              {selected ? (
                <>
                  <span className="dw-handle dw-handle-nw" />
                  <span className="dw-handle dw-handle-ne" />
                  <span className="dw-handle dw-handle-sw" />
                  <span className="dw-handle dw-handle-se" />
                </>
              ) : null}
            </div>
          );
        })}
      </div>
      {/* Editing 层：整页编辑态提示，Caret 由 Selection 层按对象绘制。 */}
      <div className={editingObjectId !== null ? 'dw-layer dw-layer-editing is-active' : 'dw-layer dw-layer-editing'} />
      {/* 以下层按 19 / 20 / 21 章接入，顺序不可调整。 */}
      <div className="dw-layer dw-layer-annotation" />
      <div className="dw-layer dw-layer-ocr" />
      <div className="dw-layer dw-layer-ai" />
      <div className="dw-layer dw-layer-debug" />
    </div>
  );
}
