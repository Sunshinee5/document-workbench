// 18B builtin feature（规范 18 §三 / §四）：把 18A 已有能力迁入注册表。
//
// 行为必须与 18A 完全一致：浏览 / 对象选择 / 文字编辑 / 保存链路不变。
// 这里只做「声明化改造」，不新增业务：每条命令 / 分区 / 层都是注册项。
import { useEffect, useRef, useState } from 'react';
import { Button, PanelEmpty, PropertyGrid, PropertySection, type PropertyRow } from '../components/ui';
import type { InspectorInfo } from '../ipc';
import type {
  DesktopFeature,
  InspectorRenderProps,
  OverlayRenderProps,
} from './types';

/** bbox → 屏幕百分比矩形（从 PageOverlays 迁移，保持算法一致）。 */
function projectBox(
  bbox: [number, number, number, number],
  pageWidth: number,
  pageHeight: number,
  rotation: number,
) {
  const w = pageWidth > 0 ? pageWidth : 1;
  const h = pageHeight > 0 ? pageHeight : 1;
  const [x, y, bw, bh] = bbox;
  const u0 = x / w;
  const u1 = (x + bw) / w;
  const v0 = 1 - (y + bh) / h;
  const v1 = 1 - y / h;
  const angle = ((rotation % 360) + 360) % 360;
  const corners: Array<[number, number]> = [
    [u0, v0],
    [u1, v0],
    [u0, v1],
    [u1, v1],
  ];
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

/** Selection/Hover/Caret/BBox 覆盖层渲染（AD-10 / 18C AD-27 Primary 高亮 + AD-28 优先级提示）。 */
function renderSelectionOverlay(props: OverlayRenderProps) {
  const { pageWidth, pageHeight, rotation, objects, hoverObjectId, selectedObjectIds, editingObjectId } = props;
  const boxes = objects.map((object) => {
    const box = projectBox(object.bbox, pageWidth, pageHeight, rotation);
    const selected = selectedObjectIds.includes(object.objectId);
    const hovered = hoverObjectId === object.objectId;
    const editing = editingObjectId === object.objectId;
    const primary = selected && props.primaryObjectId === object.objectId;
    if (!selected && !hovered && !editing) return null;
    const classes = ['dw-object-box'];
    if (selected) classes.push(primary ? 'is-primary' : 'is-selected');
    if (hovered) classes.push('is-hover');
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
        {primary ? <span className="dw-primary-tag">主选</span> : null}
        {hovered && props.hoverKind ? (
          <span className="dw-hover-tag">{props.hoverKind}</span>
        ) : null}
        {editing ? <span className="dw-caret" /> : null}
        {selected ? (
          <>
            {(['nw', 'ne', 'sw', 'se'] as const).map((corner) => (
              <span
                key={corner}
                className={`dw-handle dw-handle-${corner}`}
                title="缩放由页面命令完成，拖拽缩放未实现"
                onPointerDown={(event) => event.stopPropagation()}
                onClick={(event) => event.stopPropagation()}
                onMouseEnter={() => props.onCursorOverride?.('resize')}
                onMouseLeave={() => props.onCursorOverride?.(null)}
              />
            ))}
            {/* Rotate Handle：规范只要求显示，不实现拖拽旋转。 */}
            <span
              className="dw-handle dw-handle-rotate"
              title="旋转由页面命令完成，拖拽旋转未实现"
              onPointerDown={(event) => event.stopPropagation()}
              onClick={(event) => event.stopPropagation()}
              onMouseEnter={() => props.onCursorOverride?.('rotate')}
              onMouseLeave={() => props.onCursorOverride?.(null)}
            />
          </>
        ) : null}
      </div>
    );
  });
  return boxes;
}

/** Inline Text Editor（18C AD-26）：编辑态浮在对象 BBox 上，唯一页面内输入入口。 */
function InlineTextEditor({ props }: { props: OverlayRenderProps }) {
  const editingObjectId = props.editingObjectId;
  const initialText = props.editingText ?? '';
  const [draft, setDraft] = useState(initialText);
  // 草稿是编辑事务的输入：只有 editingObjectId 真正切换到另一个对象时才重置。
  // inspector / revision 变化会让 editingText 变空，绝不能因此清空用户正在输入的内容（白框根因）。
  const lastObjectIdRef = useRef(editingObjectId);
  useEffect(() => {
    if (lastObjectIdRef.current !== editingObjectId) {
      lastObjectIdRef.current = editingObjectId;
      setDraft(initialText);
      return;
    }
    // 初值只在草稿为空时补入；非空草稿一律保留，不被外部投影覆盖。
    setDraft((current) => (current.length === 0 && initialText.length > 0 ? initialText : current));
  }, [editingObjectId, initialText]);
  const object = props.objects.find((item) => item.objectId === editingObjectId);
  if (!object) return null;
  const box = projectBox(object.bbox, props.pageWidth, props.pageHeight, props.rotation);
  return (
    <div
      className="dw-inline-editor-wrap"
      style={{
        left: `${box.left}%`,
        top: `${box.top}%`,
        width: `${Math.max(box.width, 18)}%`,
        height: `${Math.max(box.height, 6)}%`,
      }}
    >
      <textarea
        className="dw-inline-editor"
        aria-label="页面内文本编辑"
        value={draft}
        autoFocus
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault();
            props.onCommitText?.(draft);
          } else if (event.key === 'Escape') {
            event.preventDefault();
            props.onCancelEdit?.();
          }
          event.stopPropagation();
        }}
        onBlur={() => {
          // 失焦不自动提交：避免点击 Inspector 时产生双提交；提交仍走 Enter / Ribbon。
          // 有未提交改动时用唯一 ToastStack 轻提示，不弹 alert / dialog。
          if (draft !== initialText) {
            props.notify?.('文本修改未提交，按 Enter 保存或 Esc 取消');
          }
        }}
      />
      <div className="dw-inline-editor-actions">
        {/* 阻止 mousedown 抢焦点：否则提交 / 取消按钮会先触发失焦提示。 */}
        <button
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => props.onCommitText?.(draft)}
        >
          提交
        </button>
        <button
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => props.onCancelEdit?.()}
        >
          取消
        </button>
      </div>
    </div>
  );
}

/** editing 层注册项：Inline Editor（草稿是事务输入，不进注册表）。 */
function renderEditingOverlay(props: OverlayRenderProps) {
  if (props.editingObjectId === null) return null;
  // 非文本对象（Path / Image）不得渲染页面内编辑框：避免编辑对象与 Inspector 主选不一致时误编辑。
  const object = props.objects.find((item) => item.objectId === props.editingObjectId);
  if (!object || object.kind !== 'Text') return null;
  return <InlineTextEditor props={props} />;
}

/** OCR Overlay（第19章 / 规范 26 第三部分）：BBox + Hover 轻操作条，不直接写文档。 */
function renderOcrOverlay(props: OverlayRenderProps) {
  // App 按页投影，只传当前页结果；这里直接画，不按页再过滤。
  const visible = props.ocrResults ?? [];
  const reviews = props.ocrReviews ?? [];
  const canApply = props.ocrCanApply ?? [];
  const activeIndex = props.ocrActiveIndex ?? null;
  return visible.map((result) => {
    // 投影里的 resultIndex 是全局序号：reviews / canApply / 回调都按它取值。
    const globalIndex = result.resultIndex;
    const box = projectBox(result.bbox, props.pageWidth, props.pageHeight, props.rotation);
    const review = reviews.find((item) => item.result_index === globalIndex);
    const accepted = review?.decision === '已接受';
    const rejected = review?.decision === '已拒绝';
    const classes = ['dw-ocr-box'];
    if (activeIndex === globalIndex) classes.push('is-active');
    if (rejected) classes.push('is-rejected');
    const applyDisabled = !canApply[globalIndex];
    return (
      <div
        key={`ocr-${result.page}-${globalIndex}`}
        className={classes.join(' ')}
        title={`${result.text} · ${Math.round(result.confidence * 100)}% · ${review?.decision ?? '待审核'}`}
        style={{
          left: `${box.left}%`,
          top: `${box.top}%`,
          width: `${box.width}%`,
          height: `${box.height}%`,
          borderColor: `var(--dw-ocr-${result.confidence >= 0.8 ? 'high' : result.confidence >= 0.5 ? 'mid' : 'low'})`,
        }}
        // Overlay 自身可交互，但指针事件不传给画布：框选 / 页面选择互不干扰。
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          props.onOcrActivate?.(globalIndex);
        }}
      >
        <span className="dw-ocr-box-tag">{Math.round(result.confidence * 100)}%</span>
        <div
          className="dw-ocr-box-actions"
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
        >
          <Button
            size="sm"
            className="dw-ocr-box-btn"
            disabled={accepted}
            title={accepted ? '该结果已接受' : '接受该 OCR 结果'}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation();
              props.onOcrDecide?.(globalIndex, true);
            }}
          >
            接受
          </Button>
          <Button
            size="sm"
            className="dw-ocr-box-btn"
            disabled={rejected}
            title={rejected ? '该结果已拒绝' : '拒绝该 OCR 结果（不可再应用）'}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation();
              props.onOcrDecide?.(globalIndex, false);
            }}
          >
            拒绝
          </Button>
          <Button
            size="sm"
            variant="primary"
            className="dw-ocr-box-btn"
            disabled={applyDisabled}
            title={rejected
              ? '已拒绝的结果不可应用'
              : applyDisabled
                ? '需先接受结果，且结果必须来自文本对象'
                : '经 Editing Engine 写回，可撤销'}
            onMouseDown={(event) => event.preventDefault()}
            onClick={(event) => {
              event.stopPropagation();
              if (!applyDisabled) props.onOcrApply?.(globalIndex);
            }}
          >
            应用
          </Button>
        </div>
      </div>
    );
  });
}

/** Inspector 文本分区（18A 行为：字段只读，Unicode 可编辑提交）。 */
function TextInspectorSection({ inspector, focusedPageRotation, ctx, actions }: InspectorRenderProps) {
  const text = inspector?.text ?? null;
  const [draft, setDraft] = useState('');
  const editingActive = ctx.editing.active;
  const editingObjectId = ctx.editing.objectId;
  const previousEditRef = useRef({ active: false, objectId: null as string | null });
  // 只在「刚进入编辑态 / 编辑对象切换」时灌入原文：inspector 随 primary 刷新不得清空用户草稿。
  useEffect(() => {
    const previous = previousEditRef.current;
    const activated = !previous.active && editingActive;
    const switched = previous.objectId !== editingObjectId;
    previousEditRef.current = { active: editingActive, objectId: editingObjectId };
    if (!editingActive || (!activated && !switched)) return;
    // Inspector 跟随 primary 刷新；只有它与编辑对象是同一个对象时，原文才可信。
    if (inspector?.object_id !== editingObjectId) return;
    setDraft(text?.unicode ?? '');
  }, [editingActive, editingObjectId, inspector?.object_id, text?.unicode]);
  const rows: PropertyRow[] = text
    ? [
        { label: 'ObjectId', value: `#${inspector?.object_id ?? ''}`, mono: true },
        { label: '字体', value: text.font_id || '—', mono: true },
        { label: '字号', value: `${text.font_size.toFixed(2)} pt`, mono: true },
        { label: 'Unicode', value: text.unicode ?? '—' },
        { label: '字形数', value: String(text.glyph_count), mono: true },
        { label: '颜色', value: '未采集', muted: true },
        {
          label: '页 / 层级',
          // 层级只是诊断信息，用中文而不是 z= 暴露内部字段名。
          value: `第 ${inspector?.page ?? '—'} 页 · 层级 ${inspector?.z_index ?? '—'}`,
          mono: true,
        },
        { label: '旋转', value: focusedPageRotation === null ? '—' : `${focusedPageRotation}°`, mono: true },
        { label: '版本', value: inspector ? `v${inspector.version}（${inspector.lifecycle}）` : '—', mono: true },
      ]
    : [{ label: '对象', value: '未选中文本对象', muted: true }];

  return (
    <PropertySection title="文本">
      <PropertyGrid rows={rows} />
      {editingActive ? (
        <div className="dw-prop-edit">
          <textarea
            className="dw-prop-input"
            rows={2}
            value={draft}
            placeholder={text?.unicode ?? '输入新的文本'}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                // 空文本不允许提交：回车会把对象文本清空。
                if (draft.trim().length > 0) actions.replaceText(draft);
              } else if (event.key === 'Escape') {
                event.preventDefault();
                actions.cancel();
              }
              // 阻止冒泡到 App 的全局快捷键（Enter 会再 commit 一次）。
              event.stopPropagation();
            }}
          />
          <div className="dw-prop-actions">
            <Button
              size="sm"
              variant="primary"
              disabled={draft.trim().length === 0}
              onClick={() => actions.replaceText(draft)}
            >
              提交
            </Button>
            <Button size="sm" onClick={() => actions.cancel()}>
              取消
            </Button>
          </div>
        </div>
      ) : (
        <div className="dw-prop-actions">
          <Button
            size="sm"
            disabled={!text}
            title={text ? '进入文字编辑（F2）' : '先选中文本对象'}
            onClick={() => {
              setDraft(text?.unicode ?? '');
              actions.beginEdit();
            }}
          >
            编辑文字
          </Button>
        </div>
      )}
    </PropertySection>
  );
}

/** Inspector 图片分区。 */
function imageRows(inspector: InspectorInfo | null): PropertyRow[] {
  const image = inspector?.image ?? null;
  return image
    ? [
        { label: 'ObjectId', value: `#${inspector?.object_id ?? ''}`, mono: true },
        { label: '像素', value: `${image.pixel_width} × ${image.pixel_height}`, mono: true },
        {
          label: '显示尺寸',
          value: inspector ? `${inspector.bbox[2].toFixed(1)} × ${inspector.bbox[3].toFixed(1)} pt` : '—',
          mono: true,
        },
        { label: '色彩空间', value: image.colorspace || '—', mono: true },
        { label: '分辨率', value: image.resolution_dpi ? `${image.resolution_dpi.toFixed(0)} DPI` : '—', mono: true },
        { label: '压缩', value: image.compression ?? '未采集', muted: !image.compression },
        { label: '流对象', value: `xref ${image.xref}`, mono: true },
      ]
    : [{ label: '对象', value: '未选中图片对象', muted: true }];
}

/** 页码输入项：保留 18A「输入框跳转」行为，draft 是纯 UI 态不进注册表。 */
function PageStatusInput({ ctx, actions }: { ctx: import('./types').FrameworkContext; actions: import('./types').FrameworkActions }) {
  const [draft, setDraft] = useState(String(ctx.page));
  useEffect(() => setDraft(String(ctx.page)), [ctx.page]);
  const commit = () => {
    const parsed = Number.parseInt(draft, 10);
    if (Number.isFinite(parsed)) actions.goToPage(parsed);
    else setDraft(String(ctx.page));
  };
  return (
    <span className="dw-status-item dw-status-page" title="页码">
      第
      <input
        className="dw-status-page-input"
        value={draft}
        disabled={ctx.pageCount === 0}
        aria-label="跳转到页码"
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            // 阻止冒泡：编辑态下 Enter 会被全局快捷键当成“提交”，误伤正在编辑的对象。
            event.stopPropagation();
            commit();
          }
        }}
      />
      / {ctx.pageCount || '—'} 页
    </span>
  );
}

/** HiDPI 比例投影：只读 CSS Token，不持有第二份 DPR 真值。 */
function scaleLabel(): string {
  const raw = window.getComputedStyle(document.documentElement).getPropertyValue('--dw-ui-scale').trim();
  const scale = Number.parseFloat(raw);
  return Number.isFinite(scale) && scale > 0 ? `${Math.round(scale * 100)}%` : '100%';
}

/** Inspector 文档分区。 */
function DocumentInspectorSection({ ctx, document, formatSize }: InspectorRenderProps) {
  const rows: PropertyRow[] = ctx.hasDocument
    ? [
        { label: '标题', value: document.title || '—' },
        { label: '作者', value: document.author || '—' },
        { label: '页数', value: String(document.pageCount) },
        { label: '大小', value: formatSize(document.fileSize) },
        { label: '保存状态', value: document.dirty ? '未保存' : '已保存' },
        { label: '上次保存', value: document.lastSavedAt || '—' },
        { label: '解析器', value: document.parserText },
        { label: '路径', value: document.path || '—', mono: true },
      ]
    : [{ label: '文档', value: '尚未打开', muted: true }];
  return (
    <PropertySection title="文档">
      <PropertyGrid rows={rows} />
    </PropertySection>
  );
}

/** 无数据分区：统一文案 + 统一空态组件，禁止各分区自造 Placeholder。 */
function PlaceholderInspection({ title, hint }: { title: string; hint: string }) {
  return (
    <PropertySection title={title}>
      <PanelEmpty text="暂无数据" hint={hint} />
    </PropertySection>
  );
}

/** 页面分区：只读投影；未打开文档时统一空态。 */
function PageInspectorSection({ ctx, focusedPageRotation }: InspectorRenderProps) {
  if (!ctx.hasDocument) {
    return <PlaceholderInspection title="页面" hint="打开 PDF 后显示页码、页数与旋转信息。" />;
  }
  const rows: PropertyRow[] = [
    { label: '当前页', value: String(ctx.page), mono: true },
    { label: '总页数', value: String(ctx.pageCount), mono: true },
    { label: '旋转', value: focusedPageRotation === null ? '—' : `${focusedPageRotation}°`, mono: true },
    { label: '缩放', value: `${ctx.zoomPercent}%`, mono: true },
  ];
  return (
    <PropertySection title="页面">
      <PropertyGrid rows={rows} />
    </PropertySection>
  );
}

/** 选择分区：只读投影，真值仍在 SelectionState。 */
function SelectionInspectorSection({ ctx }: InspectorRenderProps) {
  if (ctx.selection.selected.length === 0) {
    return <PlaceholderInspection title="选择" hint="在页面或对象树中选择对象后显示数量与主选。" />;
  }
  const rows: PropertyRow[] = [
    { label: '数量', value: `${ctx.selection.selected.length}`, mono: true },
    { label: '主选', value: ctx.selection.primary === null ? '—' : `#${ctx.selection.primary}`, mono: true },
    { label: '校验', value: ctx.selectionValidation },
    { label: '页码', value: ctx.selection.page === null ? '—' : `第 ${ctx.selection.page} 页`, mono: true },
  ];
  return (
    <PropertySection title="选择">
      <PropertyGrid rows={rows} />
    </PropertySection>
  );
}

/** OCR 分区：Provider / Confidence 缺失时统一显示「—」。 */
function OcrInspectorSection({ ctx }: InspectorRenderProps) {
  const confidence = ctx.ocrConfidence;
  const hasConfidence = typeof confidence === 'number' && Number.isFinite(confidence);
  const rows: PropertyRow[] = [
    { label: '会话状态', value: ctx.ocrStateLabel },
    { label: '进度', value: `${ctx.ocrProgress}%`, mono: true },
    { label: '结果数', value: `${ctx.ocrResultCount} 条`, mono: true },
    { label: 'Provider', value: ctx.ocrProvider || '—', muted: !ctx.ocrProvider },
    {
      label: 'Confidence',
      value: hasConfidence ? `${Math.round((confidence as number) * 100)}%` : '—',
      mono: hasConfidence,
      muted: !hasConfidence,
    },
    { label: '文本', value: ctx.ocrActiveText || '—', muted: !ctx.ocrActiveText },
    { label: '引擎', value: ctx.ocrAvailable ? '可用' : '未安装', muted: !ctx.ocrAvailable },
    { label: '写回方式', value: 'Editing Engine', muted: true },
  ];
  return (
    <PropertySection title="OCR">
      <PropertyGrid rows={rows} />
    </PropertySection>
  );
}

/** 调试分区：只投影只读计数器，不暴露任何真值对象。 */
function DebugInspectorSection({ ctx }: InspectorRenderProps) {
  if (!ctx.hasDocument) {
    return <PlaceholderInspection title="调试" hint="打开文档后显示版本号、待提交补丁与最近事件。" />;
  }
  const rows: PropertyRow[] = [
    { label: '版本', value: `v${ctx.revision}`, mono: true },
    { label: '待提交补丁', value: String(ctx.pendingPatches), mono: true },
    // 命中优先级（z=）是调试信息，用户不需要看到；ctx 投影保留，仅移除渲染。
    { label: '最近事件', value: ctx.lastEvent || '—' },
  ];
  return (
    <PropertySection title="调试">
      <PropertyGrid rows={rows} />
    </PropertySection>
  );
}

/** builtin feature：18A 全部能力的声明化注册。 */
export const builtinFeature: DesktopFeature = {
  id: 'builtin',

  // ---- Tool Framework：模式即工具 id，含未接入能力的禁用入口 ----
  tools: [
    { id: 'browse', label: '浏览', icon: 'browse', order: 10, cursor: 'arrow', enabled: () => true },
    { id: 'select', label: '选择', icon: 'select', order: 20, cursor: 'cross', enabled: (ctx) => ctx.hasDocument },
    {
      id: 'edit', label: '编辑', icon: 'editText', order: 30, cursor: 'ibeam',
      enabled: (ctx) => ctx.capability.can_edit_text, reason: '先选中一个文本对象',
    },
    {
      id: 'ocr', label: 'OCR', icon: 'ocr', order: 40, cursor: 'ocr',
      enabled: (ctx) => ctx.hasDocument && ctx.ocrAvailable,
      reason: '先打开文档并确认 OCR 引擎可用',
    },
    { id: 'annotation', label: '批注', icon: 'annotation', order: 50, cursor: 'annotation', enabled: () => false, reason: '批注由 20 章接入' },
    { id: 'ai', label: 'AI', icon: 'ai', order: 60, cursor: 'ai', enabled: () => false, reason: 'AI 由 21 章接入' },
  ],

  // ---- Cursor Framework：工具 → 光标（唯一映射） ----
  cursors: [
    { id: 'browse', cursor: 'arrow' },
    { id: 'select', cursor: 'cross' },
    { id: 'edit', cursor: 'ibeam' },
    { id: 'ocr', cursor: 'ocr' },
    { id: 'annotation', cursor: 'annotation' },
    { id: 'ai', cursor: 'ai' },
  ],

  // ---- Overlay Framework：层顺序冻结；内容由注册项提供 ----
  overlayLayers: [
    { id: 'text', order: 10 },
    { id: 'selection', order: 20 },
    { id: 'editing', order: 30, className: (props) => (props.editingObjectId !== null ? 'is-active' : '') },
    { id: 'annotation', order: 40 },
    { id: 'ocr', order: 50 },
    { id: 'ai', order: 60 },
    { id: 'debug', order: 70 },
  ],
  overlays: [
    { id: 'selection.boxes', layer: 'selection', order: 10, render: renderSelectionOverlay },
    { id: 'editing.inline', layer: 'editing', order: 10, render: renderEditingOverlay },
    { id: 'ocr.results', layer: 'ocr', order: 10, render: renderOcrOverlay },
  ],

  // ---- Inspector Framework：26 §5 固定十分区，无数据统一显示 Placeholder ----
  inspectors: [
    { id: 'document', title: '文档', order: 10, matches: () => true, render: DocumentInspectorSection },
    {
      id: 'page', title: '页面', order: 20, matches: () => true, render: PageInspectorSection,
    },
    {
      id: 'selection', title: '选择', order: 30, matches: () => true, render: SelectionInspectorSection,
    },
    {
      id: 'object', title: '对象', order: 40, matches: () => true, render: (props) => (
        <>
          <PropertySection title="对象">
            <PropertyGrid
              rows={props.inspector
                ? [
                    { label: 'ObjectId', value: `#${props.inspector.object_id}`, mono: true },
                    { label: '类型', value: `${props.inspector.kind_label}（${props.inspector.kind}）`, mono: true },
                    { label: '页码', value: `第 ${props.inspector.page} 页`, mono: true },
                    {
                      label: 'Bounding Box',
                      value: props.inspector.bbox.map((value) => value.toFixed(1)).join(', '),
                      mono: true,
                    },
                    { label: '层级 z', value: String(props.inspector.z_index), mono: true },
                    { label: '版本', value: `v${props.inspector.version}`, mono: true },
                    { label: '生命周期', value: props.inspector.lifecycle, mono: true },
                  ]
                : [{ label: '对象', value: '未选中对象', muted: true }]}
            />
          </PropertySection>
          {/* Text / Image / Path 归入 Object 分区内部，保留全部既有编辑能力。 */}
          <TextInspectorSection {...props} />
          <PropertySection title="图片">
            <PropertyGrid rows={imageRows(props.inspector)} />
            <div className="dw-prop-actions">
              <Button size="sm" disabled title="替换图片属 18-07 图片编辑，IPC 未接入">替换</Button>
              <Button size="sm" disabled title="裁剪属页面编辑扩展，尚未接入">裁剪</Button>
            </div>
          </PropertySection>
          <PropertySection title="路径">
            <PropertyGrid
              rows={props.inspector?.path
                ? [
                    { label: '指令数', value: String(props.inspector.path.command_count), mono: true },
                    { label: '描边', value: props.inspector.path.stroke ?? '—', mono: true },
                    { label: '填充', value: props.inspector.path.fill ?? '—', mono: true },
                    { label: '线宽', value: '未采集', muted: true },
                  ]
                : [{ label: '对象', value: '未选中路径对象', muted: true }]}
            />
          </PropertySection>
        </>
      ),
    },
    {
      id: 'ocr', title: 'OCR', order: 50, matches: () => true, render: OcrInspectorSection,
    },
    {
      id: 'annotation', title: '批注', order: 60, matches: () => true,
      render: () => <PlaceholderInspection title="批注" hint="批注由 20 章接入，接入后显示作者、内容与回复。" />,
    },
    {
      id: 'search', title: '搜索', order: 70, matches: () => true,
      render: () => <PlaceholderInspection title="搜索" hint="搜索由 21 章接入，接入后显示查询、结果与来源。" />,
    },
    {
      id: 'ai', title: 'AI', order: 80, matches: () => true,
      render: () => <PlaceholderInspection title="AI" hint="AI 由 22 章接入，接入后显示 Provider、Model 与 Token。" />,
    },
    {
      id: 'plugin', title: '插件', order: 90, matches: () => true,
      render: () => <PlaceholderInspection title="插件" hint="插件由 23 章接入，接入后显示权限、版本与能力。" />,
    },
    {
      id: 'debug', title: '调试', order: 100, matches: () => true,
      render: DebugInspectorSection,
    },
  ],

  // ---- Ribbon Framework：页签 / 分组 / 命令全部注册；needs 驱动启停 ----
  ribbonTabs: [
    { id: 'home', label: '开始', order: 10 },
    { id: 'edit', label: '编辑', order: 20 },
    { id: 'ocr', label: 'OCR', order: 30 },
    { id: 'annotation', label: '批注', order: 40 },
    { id: 'ai', label: 'AI', order: 50 },
    { id: 'view', label: '视图', order: 60 },
  ],
  ribbonGroups: [
    { id: 'home.document', tab: 'home', label: '文档', order: 10 },
    { id: 'home.history', tab: 'home', label: '历史', order: 20 },
    { id: 'home.output', tab: 'home', label: '输出', order: 30 },
    { id: 'edit.mode', tab: 'edit', label: '鼠标模式', order: 10 },
    { id: 'edit.object', tab: 'edit', label: '对象', order: 20 },
    { id: 'edit.page', tab: 'edit', label: '页面', order: 30 },
    { id: 'edit.recognize', tab: 'edit', label: '识别', order: 40 },
    { id: 'ocr.main', tab: 'ocr', label: 'OCR', order: 10 },
    { id: 'annotation.main', tab: 'annotation', label: '批注（20 章）', order: 10 },
    { id: 'ai.main', tab: 'ai', label: 'AI（21 章）', order: 10 },
    { id: 'view.zoom', tab: 'view', label: '缩放', order: 10 },
    { id: 'view.window', tab: 'view', label: '窗口', order: 20 },
    { id: 'view.theme', tab: 'view', label: '主题', order: 30 },
  ],
  ribbonCommands: [
    { id: 'open', tab: 'home', group: 'home.document', label: '打开', icon: 'open', order: 10, needs: ['always'], run: (a) => a.open() },
    { id: 'save', tab: 'home', group: 'home.document', label: '保存', icon: 'save', order: 20, variant: 'primary', needs: ['document'], reason: '请先打开文档', run: (a) => a.save() },
    { id: 'saveAs', tab: 'home', group: 'home.document', label: '另存为', icon: 'saveAs', order: 30, needs: ['document'], run: (a) => a.saveAs() },
    { id: 'undo', tab: 'home', group: 'home.history', label: '撤销', icon: 'undo', order: 10, needs: ['undo'], reason: '当前没有可撤销的操作', run: (a) => a.undo() },
    { id: 'redo', tab: 'home', group: 'home.history', label: '重做', icon: 'redo', order: 20, needs: ['redo'], reason: '当前没有可重做的操作', run: (a) => a.redo() },
    { id: 'print', tab: 'home', group: 'home.output', label: '打印', icon: 'print', order: 10, needs: ['never'], reason: '打印能力由后续章节接入', run: () => undefined },
    { id: 'export', tab: 'home', group: 'home.output', label: '导出', icon: 'export', order: 20, needs: ['never'], reason: '导出能力由后续章节接入', run: () => undefined },

    { id: 'mode.browse', tab: 'edit', group: 'edit.mode', label: '浏览', icon: 'browse', order: 10, needs: ['always'], active: (ctx) => ctx.mode === 'browse', run: (a) => a.setMode('browse') },
    { id: 'mode.select', tab: 'edit', group: 'edit.mode', label: '选择', icon: 'select', order: 20, needs: ['document'], active: (ctx) => ctx.mode === 'select', run: (a) => a.setMode('select') },
    {
      id: 'mode.edit', tab: 'edit', group: 'edit.mode', label: '编辑文本', icon: 'editText', order: 30,
      needs: ['document', 'text'], active: (ctx) => ctx.mode === 'edit',
      reason: '先选中一个文本对象', run: (a) => a.setMode('edit'),
    },
    {
      id: 'mode.ocr', tab: 'edit', group: 'edit.mode', label: 'OCR', icon: 'ocr', order: 40,
      needs: ['document', 'ocrAvailable'], active: (ctx) => ctx.mode === 'ocr',
      reason: '先打开文档并确认 OCR 引擎可用', run: (a) => a.setMode('ocr'),
    },
    { id: 'image.replace', tab: 'edit', group: 'edit.object', label: '替换图片', icon: 'image', order: 10, needs: ['never'], reason: '替换图片属 18-07 图片编辑，IPC 未接入', run: () => undefined },
    { id: 'crop', tab: 'edit', group: 'edit.object', label: '裁剪', icon: 'crop', order: 20, needs: ['never'], reason: '裁剪属页面编辑扩展，尚未接入', run: () => undefined },
    { id: 'path.edit', tab: 'edit', group: 'edit.object', label: '路径', icon: 'path', order: 30, needs: ['never'], reason: '路径编辑由后续章节接入，IPC 未接入', run: () => undefined },
    {
      id: 'deleteObject', tab: 'edit', group: 'edit.object', label: '删除对象', icon: 'delete', order: 40,
      needs: ['never'], reason: '对象删除由 18A C-9 排除，本章不实现', run: () => undefined,
    },
    { id: 'rotateLeft', tab: 'edit', group: 'edit.page', label: '左转', icon: 'rotateLeft', order: 10, needs: ['document'], run: (a) => a.rotate(-90) },
    { id: 'rotateRight', tab: 'edit', group: 'edit.page', label: '右转', icon: 'rotateRight', order: 20, needs: ['document'], run: (a) => a.rotate(90) },
    { id: 'deletePages', tab: 'edit', group: 'edit.page', label: '删除页', icon: 'delete', order: 30, variant: 'danger', needs: ['multiPage'], reason: '只有一页，无法删除', run: (a) => a.deletePages() },
    {
      id: 'edit.ocr', tab: 'edit', group: 'edit.recognize', label: 'OCR 识别', icon: 'ocr', order: 10,
      needs: ['document', 'ocrAvailable'], reason: '先打开文档并确认 OCR 引擎可用',
      run: (a) => a.ocrRecognize('full'),
    },

    { id: 'ocr.recognize', tab: 'ocr', group: 'ocr.main', label: '全文识别', icon: 'ocr', order: 10, needs: ['document', 'ocrAvailable'], reason: '先打开文档并确认 OCR 引擎可用', run: (a) => a.ocrRecognize('full') },
    { id: 'ocr.pause', tab: 'ocr', group: 'ocr.main', label: '暂停', icon: 'select', order: 15, needs: ['ocrRunning'], reason: '当前没有运行中的 OCR 任务', run: (a) => a.ocrPause() },
    { id: 'ocr.resume', tab: 'ocr', group: 'ocr.main', label: '继续', icon: 'select', order: 16, needs: ['ocrPaused'], reason: '当前没有已暂停的 OCR 任务', run: (a) => a.ocrResume() },
    { id: 'ocr.cancel', tab: 'ocr', group: 'ocr.main', label: '取消', icon: 'close', order: 17, needs: ['ocrRunning', 'ocrPaused'], reason: '当前没有运行中的 OCR 任务', run: (a) => a.ocrCancel() },
    {
      id: 'ocr.apply', tab: 'ocr', group: 'ocr.main', label: '应用当前结果', icon: 'save', order: 20,
      needs: ['document', 'ocrApplyable'], reason: '先接受当前 OCR 结果，且结果必须来自文本对象',
      // ocrApplyActive 返回 void，不能用 ?? 兜底（会再应用第 0 条）。
      run: (a) => { if (a.ocrApplyActive) a.ocrApplyActive(); else a.ocrApply(0); },
    },
    { id: 'ocr.search', tab: 'ocr', group: 'ocr.main', label: '结果内查找', icon: 'outline', order: 30, needs: ['ocrResults'], reason: '先完成 OCR 识别', run: (a) => a.searchInOcr() },
    { id: 'ocr.merge', tab: 'ocr', group: 'ocr.main', label: '合并文本层', icon: 'layers', order: 40, needs: ['never'], reason: '合并策略由后续章节接入', run: () => undefined },

    { id: 'annotation.highlight', tab: 'annotation', group: 'annotation.main', label: '高亮', icon: 'annotation', order: 10, needs: ['never'], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.underline', tab: 'annotation', group: 'annotation.main', label: '下划线', icon: 'annotation', order: 20, needs: ['never'], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.comment', tab: 'annotation', group: 'annotation.main', label: '批注', icon: 'annotation', order: 30, needs: ['never'], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.signature', tab: 'annotation', group: 'annotation.main', label: '签名', icon: 'image', order: 40, needs: ['never'], reason: '批注由 20 章接入', run: () => undefined },

    { id: 'ai.summarize', tab: 'ai', group: 'ai.main', label: '摘要', icon: 'ai', order: 10, needs: ['never'], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.translate', tab: 'ai', group: 'ai.main', label: '翻译', icon: 'ai', order: 20, needs: ['never'], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.rewrite', tab: 'ai', group: 'ai.main', label: '改写', icon: 'ai', order: 30, needs: ['never'], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.extract', tab: 'ai', group: 'ai.main', label: '提取', icon: 'ai', order: 40, needs: ['never'], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.chat', tab: 'ai', group: 'ai.main', label: '对话', icon: 'ai', order: 50, needs: ['never'], reason: 'AI 由 21 章接入', run: () => undefined },

    { id: 'zoomOut', tab: 'view', group: 'view.zoom', label: '缩小', icon: 'zoomOut', order: 10, needs: ['document'], run: (a) => a.zoomOut() },
    { id: 'zoomIn', tab: 'view', group: 'view.zoom', label: '放大', icon: 'zoomIn', order: 20, needs: ['document'], run: (a) => a.zoomIn() },
    { id: 'fitWidth', tab: 'view', group: 'view.zoom', label: '适应宽度', icon: 'fitWidth', order: 30, needs: ['document'], run: (a) => a.fitWidth() },
    { id: 'actualSize', tab: 'view', group: 'view.zoom', label: '100%', icon: 'actualSize', order: 40, needs: ['document'], run: (a) => a.actualSize() },
    { id: 'fullscreen', tab: 'view', group: 'view.window', label: '全屏', icon: 'fullscreen', order: 10, needs: ['always'], run: (a) => a.fullscreen() },
    { id: 'theme.light', tab: 'view', group: 'view.theme', label: '浅色', icon: 'themeLight', order: 10, needs: ['always'], active: (ctx) => ctx.theme === 'light', run: (a) => a.setTheme('light') },
    { id: 'theme.dark', tab: 'view', group: 'view.theme', label: '深色', icon: 'themeDark', order: 20, needs: ['always'], active: (ctx) => ctx.theme === 'dark', run: (a) => a.setTheme('dark') },
    { id: 'theme.system', tab: 'view', group: 'view.theme', label: '跟随系统', icon: 'themeSystem', order: 30, needs: ['always'], active: (ctx) => ctx.theme === 'system', run: (a) => a.setTheme('system') },
  ],

  // ---- Context Menu Framework：Provider 统一供给，无 switch ----
  contextMenu: [
    {
      id: 'builtin.object',
      order: 10,
      items: (ctx, actions) => [
        { id: 'edit', label: '编辑文字', icon: 'editText', disabled: !ctx.capability.can_edit_text, title: '先选中文本对象（F2）', run: () => actions.beginEdit() },
        { id: 'commit', label: '提交编辑', icon: 'save', disabled: !ctx.editing.active, run: () => actions.commit() },
        { id: 'cancel', label: '取消编辑', icon: 'close', disabled: !ctx.editing.active, run: () => actions.cancel() },
        { id: 'selectAll', label: '全选对象', icon: 'select', separatorBefore: true, disabled: !ctx.hasDocument, run: () => actions.selectAll() },
        { id: 'clear', label: '清空选择', disabled: !ctx.hasDocument, run: () => actions.clearSelection() },
        {
          id: 'deleteObject', label: '删除对象', icon: 'delete', disabled: true,
          title: '对象删除由 18A C-9 排除，本章不实现', run: () => undefined,
        },
        { id: 'undo', label: '撤销', icon: 'undo', separatorBefore: true, disabled: !ctx.canUndo, run: () => actions.undo() },
        { id: 'redo', label: '重做', icon: 'redo', disabled: !ctx.canRedo, run: () => actions.redo() },
      ],
    },
    {
      id: 'builtin.ocr',
      order: 20,
      items: (ctx, actions) => [
        {
          id: 'ocr.recognize', label: 'OCR 识别', icon: 'ocr', separatorBefore: true,
          disabled: !ctx.hasDocument || !ctx.ocrAvailable,
          title: ctx.ocrAvailable ? '对当前文档执行全文 OCR' : '没有可用的 OCR 引擎',
          run: () => actions.ocrRecognize('full'),
        },
        {
          id: 'ocr.copy', label: '复制 OCR 文本', icon: 'text',
          disabled: ctx.ocrResultCount === 0, title: '先完成 OCR 识别',
          run: () => actions.ocrCopyResult(),
        },
        {
          id: 'ocr.review', label: '审核 OCR 结果', icon: 'outline',
          disabled: ctx.ocrResultCount === 0, title: '切换到 OCR 工作区查看 Review',
          run: () => actions.searchInOcr(),
        },
      ],
    },
  ],

  // ---- Shortcut Framework：快捷键注册，App 不再 switch ----
  shortcuts: [
    { id: 'open', combo: 'ctrl+o', order: 10, run: (a) => a.open() },
    { id: 'save', combo: 'ctrl+s', order: 20, run: (a) => a.save() },
    { id: 'saveAs', combo: 'ctrl+shift+s', order: 30, run: (a) => a.saveAs() },
    { id: 'undo', combo: 'ctrl+z', order: 40, run: (a) => a.undo() },
    { id: 'redo', combo: 'ctrl+y', order: 50, run: (a) => a.redo() },
    { id: 'redoAlt', combo: 'ctrl+shift+z', order: 60, run: (a) => a.redo() },
    { id: 'zoomIn', combo: 'ctrl+=', order: 70, run: (a) => a.zoomIn() },
    { id: 'zoomInAlt', combo: 'ctrl++', order: 80, run: (a) => a.zoomIn() },
    { id: 'zoomInShift', combo: 'ctrl+shift+=', order: 85, run: (a) => a.zoomIn() },
    { id: 'zoomOut', combo: 'ctrl+-', order: 90, run: (a) => a.zoomOut() },
    { id: 'zoomReset', combo: 'ctrl+0', order: 100, run: (a) => a.actualSize() },
    { id: 'fullscreen', combo: 'f11', order: 110, run: (a) => a.fullscreen() },
    { id: 'cancel', combo: 'escape', order: 120, enabled: (ctx) => ctx.editing.active, run: (a) => a.cancel() },
    { id: 'beginEdit', combo: 'f2', order: 130, enabled: (ctx) => ctx.capability.can_edit_text, run: (a) => a.beginEdit() },
    { id: 'commit', combo: 'enter', order: 140, enabled: (ctx) => ctx.editing.active, run: (a) => a.commit() },
    { id: 'selectAll', combo: 'ctrl+a', order: 150, enabled: (ctx) => ctx.hasDocument, run: (a) => a.selectAll() },
    { id: 'nextPage', combo: 'pagedown', order: 160, run: (a) => a.nextPage() },
    { id: 'prevPage', combo: 'pageup', order: 170, run: (a) => a.prevPage() },
    {
      // 21 章 C-9：Ctrl+F 归 Global Search；OCR 内检索迁到 Ctrl+Shift+F。
      id: 'searchGlobal', combo: 'ctrl+f', order: 180,
      enabled: () => true, run: (a) => a.openGlobalSearch?.(),
    },
    {
      id: 'searchOcr', combo: 'ctrl+shift+f', order: 185,
      enabled: (ctx) => ctx.ocrResultCount > 0, run: (a) => a.searchInOcr(),
    },
    {
      id: 'deleteObject', combo: 'delete', order: 190,
      enabled: () => false, run: () => undefined,
    },
  ],

  // ---- Theme Framework：token 名册（值在 tokens.css；26 §7 要求与 CSS 实际名册一致） ----
  themeTokens: [
    { id: 'bg', cssVar: '--dw-bg', usage: '工作台背景' },
    { id: 'surface', cssVar: '--dw-surface', usage: '面板 / 按钮主表面' },
    { id: 'surface2', cssVar: '--dw-surface-2', usage: '次级表面：Ribbon 主体 / 输入框' },
    { id: 'surface3', cssVar: '--dw-surface-3', usage: '画布底色 / 三级表面' },
    { id: 'border', cssVar: '--dw-border', usage: '常规分隔线' },
    { id: 'borderStrong', cssVar: '--dw-border-strong', usage: '强调分隔线与滚动条' },
    { id: 'text', cssVar: '--dw-text', usage: '正文文字' },
    { id: 'textMuted', cssVar: '--dw-text-muted', usage: '辅助说明文字' },
    { id: 'accent', cssVar: '--dw-accent', usage: '主强调色：选中框、主按钮、光标提示' },
    { id: 'accentSoft', cssVar: '--dw-accent-soft', usage: '强调色浅底：Hover / 激活态' },
    { id: 'accentText', cssVar: '--dw-accent-text', usage: '强调底上的文字' },
    { id: 'danger', cssVar: '--dw-danger', usage: '危险操作 / 错误提示' },
    { id: 'dangerSoft', cssVar: '--dw-danger-soft', usage: '错误浅底' },
    { id: 'success', cssVar: '--dw-success', usage: '成功态文本' },
    { id: 'warning', cssVar: '--dw-warning', usage: '警告 / 未保存态文本' },
    { id: 'selection', cssVar: '--dw-selection', usage: '对象选中填充' },
    { id: 'hover', cssVar: '--dw-hover', usage: '统一 Hover 背景' },
    { id: 'caret', cssVar: '--dw-caret', usage: 'Inline Editor 光标' },
    { id: 'paper', cssVar: '--dw-paper', usage: '画布纸张背景' },
    { id: 'motionFast', cssVar: '--dw-motion-fast', usage: 'Overlay / Tooltip 快速过渡' },
    { id: 'motionNormal', cssVar: '--dw-motion-normal', usage: 'Progress / Panel 常规过渡' },
    { id: 'motionEase', cssVar: '--dw-motion-ease', usage: '统一缓动曲线' },
    { id: 'ocrHigh', cssVar: '--dw-ocr-high', usage: 'OCR 高置信度 BBox' },
    { id: 'ocrMid', cssVar: '--dw-ocr-mid', usage: 'OCR 中置信度 BBox' },
    { id: 'ocrLow', cssVar: '--dw-ocr-low', usage: 'OCR 低置信度 BBox' },
    { id: 'uiScale', cssVar: '--dw-ui-scale', usage: 'HiDPI 投影缩放（WebView DPR）' },
    { id: 'font', cssVar: '--dw-font', usage: '全局字体' },
    { id: 'fontMono', cssVar: '--dw-font-mono', usage: 'ID / 坐标等宽字体' },
    { id: 'radius', cssVar: '--dw-radius', usage: '控件统一圆角' },
    { id: 'radiusLg', cssVar: '--dw-radius-lg', usage: '卡片 / 面板统一圆角' },
    { id: 'shadow', cssVar: '--dw-shadow', usage: 'Toast / Dashboard 卡片阴影' },
    { id: 'shadowMenu', cssVar: '--dw-shadow-menu', usage: '右键菜单浮层阴影' },
    { id: 'shadowPage', cssVar: '--dw-shadow-page', usage: 'PDF 纸张阴影' },
    { id: 'titlebarH', cssVar: '--dw-titlebar-h', usage: '第一区高度' },
    { id: 'ribbonH', cssVar: '--dw-ribbon-h', usage: '第二区高度' },
    { id: 'statusbarH', cssVar: '--dw-statusbar-h', usage: '第六区高度' },
    { id: 'sidebarW', cssVar: '--dw-sidebar-w', usage: '左侧导航默认宽度' },
    { id: 'inspectorW', cssVar: '--dw-inspector-w', usage: '右侧属性面板宽度' },
    { id: 'tabbarH', cssVar: '--dw-tabbar-h', usage: '文档页签条高度' },
    { id: 'iconStacked', cssVar: '--dw-icon-stacked', usage: 'Ribbon 大按钮图标尺寸' },
    { id: 'iconQuick', cssVar: '--dw-icon-quick', usage: 'Quick Access 图标尺寸' },
    { id: 'fontRibbon', cssVar: '--dw-font-ribbon', usage: 'Ribbon 统一文字尺寸' },
    { id: 'btnStackedW', cssVar: '--dw-btn-stacked-w', usage: 'Ribbon 大按钮统一宽度' },
    { id: 'btnStackedH', cssVar: '--dw-btn-stacked-h', usage: 'Ribbon 大按钮统一高度' },
    { id: 'ribbonTabsH', cssVar: '--dw-ribbon-tabs-h', usage: 'Ribbon 页签行高度' },
    { id: 'ribbonBodyH', cssVar: '--dw-ribbon-body-h', usage: 'Ribbon 主体高度' },
    { id: 'groupBodyH', cssVar: '--dw-group-body-h', usage: 'Ribbon 分组按钮区高度' },
    { id: 'groupGap', cssVar: '--dw-group-gap', usage: 'Ribbon 分组统一间距' },
    { id: 'quickBtnW', cssVar: '--dw-quick-btn-w', usage: 'Quick Access 按钮宽度' },
    { id: 'quickBtnH', cssVar: '--dw-quick-btn-h', usage: 'Quick Access 按钮高度' },
  ],

  // ---- Dock Framework：六槽位结构冻结（不实现浮动 / 保存） ----
  dockPanes: [
    { id: 'titlebar', slot: 'titlebar', order: 10, label: '标题栏' },
    { id: 'ribbon', slot: 'ribbon', order: 10, label: '功能区' },
    { id: 'navigator', slot: 'navigator', order: 10, label: '工作区导航' },
    { id: 'canvas', slot: 'canvas', order: 10, label: '画布' },
    { id: 'inspector', slot: 'inspector', order: 10, label: '属性' },
    { id: 'statusbar', slot: 'statusbar', order: 10, label: '状态栏' },
  ],

  // ---- Status Framework：左 / 中 / 右三组（26 §6）；所有既有项保留 ----
  statusItems: [
    // 左：Page / Zoom / Selection / Mode（既有 tool / editing 诊断项随左组）
    { id: 'page', label: '页码', order: 10, group: 'left', render: (ctx, actions) => <PageStatusInput ctx={ctx} actions={actions} /> },
    { id: 'zoom', label: '缩放', order: 20, group: 'left', value: (ctx) => `${ctx.zoomPercent}%` },
    {
      id: 'selection', label: '选择', order: 30, group: 'left',
      value: (ctx) => {
        const count = ctx.selection.selected.length;
        if (count === 0) return `Selection(0) · ${ctx.selectionValidation}`;
        return `Selection(${count}, primary #${ctx.selection.primary ?? '—'}) · ${ctx.selectionValidation}`;
      },
      tone: (ctx) => (ctx.selectionValidation === '失效' ? 'warn' : undefined),
    },
    { id: 'tool', label: '当前工具', order: 35, group: 'left', value: (ctx) => `工具(${ctx.toolLabel})` },
    { id: 'mode', label: '光标模式', order: 40, group: 'left', value: (ctx) => ctx.mode },
    {
      id: 'editing', label: '编辑', order: 45, group: 'left',
      value: (ctx) => (ctx.editing.active ? `编辑中 #${ctx.editing.objectId ?? '—'}` : '—'),
      tone: (ctx) => (ctx.editing.active ? 'ok' : undefined),
    },
    // 中：Save / OCR / Search / AI（既有 dirty / undo / redo 诊断项随中组）
    {
      id: 'dirty', label: '未保存', order: 50, group: 'center',
      // 保存进行中时不能同时显示“已保存”，否则与右侧“保存中…”自相矛盾。
      value: (ctx) => (ctx.saveFeedback === 'saving' ? '保存中' : ctx.dirty ? '未保存' : '已保存'),
      tone: (ctx) => (ctx.saveFeedback === 'saving' ? 'warn' : ctx.dirty ? 'warn' : 'ok'),
    },
    { id: 'undo', label: '撤销', order: 55, group: 'center', value: (ctx) => (ctx.canUndo ? '可撤销' : '—') },
    { id: 'redo', label: '重做', order: 56, group: 'center', value: (ctx) => (ctx.canRedo ? '可重做' : '—') },
    {
      id: 'save', label: '保存', order: 60, group: 'center',
      value: (ctx) => {
        if (ctx.saveFeedback === 'saving') return '保存中…';
        if (ctx.saveFeedback === 'failed') return ctx.saveMessage || '保存失败';
        if (ctx.saveFeedback === 'success') return ctx.saveMessage || '保存成功';
        return ctx.saveText;
      },
      tone: (ctx) => (ctx.saveFeedback === 'failed' ? 'danger' : ctx.saveFeedback === 'saving' ? 'warn' : ctx.dirty ? 'warn' : 'ok'),
    },
    {
      id: 'ocr', label: 'OCR', order: 70, group: 'center',
      value: (ctx) => `${ctx.ocrStateLabel} · ${ctx.ocrProgress}% · ${ctx.ocrResultCount} 条`,
      tone: (ctx) => (ctx.ocrStateLabel.includes('失败') ? 'danger' : ctx.ocrStateLabel.includes('识别中') ? 'warn' : undefined),
    },
    {
      id: 'search', label: '搜索', order: 80, group: 'center',
      value: (ctx) => ctx.searchStateLabel || '搜索未就绪',
    },
    { id: 'ai', label: 'AI', order: 90, group: 'center', value: () => 'AI 未接入' },
    // 右：Memory / CPU / Scale（parser / event 诊断项随右组）
    { id: 'memory', label: '内存', order: 100, group: 'right', value: () => '—' },
    { id: 'cpu', label: 'CPU · 未采集', order: 110, group: 'right', value: () => '—' },
    { id: 'scale', label: '界面缩放（HiDPI）', order: 120, group: 'right', value: () => scaleLabel() },
    { id: 'parser', label: '解析器', order: 130, group: 'right', value: (ctx) => ctx.parserText },
    { id: 'event', label: '最近事件', order: 140, group: 'right', value: (ctx) => ctx.lastEvent || '—' },
  ],
};
