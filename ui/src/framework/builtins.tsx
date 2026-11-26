// 18B builtin feature（规范 18 §三 / §四）：把 18A 已有能力迁入注册表。
//
// 行为必须与 18A 完全一致：浏览 / 对象选择 / 文字编辑 / 保存链路不变。
// 这里只做「声明化改造」，不新增业务：每条命令 / 分区 / 层都是注册项。
import { useEffect, useState } from 'react';
import { Button, PropertyGrid, PropertySection, type PropertyRow } from '../components/ui';
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
          <span className="dw-hover-tag">
            {props.hoverKind}{props.hoverPriority ? ` · ${props.hoverPriority}` : ''}
          </span>
        ) : null}
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
  });
  return boxes;
}

/** Inline Text Editor（18C AD-26）：编辑态浮在对象 BBox 上，唯一页面内输入入口。 */
function InlineTextEditor({ props }: { props: OverlayRenderProps }) {
  const [draft, setDraft] = useState(props.editingText ?? '');
  useEffect(() => setDraft(props.editingText ?? ''), [props.editingObjectId, props.editingText]);
  const object = props.objects.find((item) => item.objectId === props.editingObjectId);
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
        }}
      />
      <div className="dw-inline-editor-actions">
        <button type="button" onClick={() => props.onCommitText?.(draft)}>提交</button>
        <button type="button" onClick={() => props.onCancelEdit?.()}>取消</button>
      </div>
    </div>
  );
}

/** editing 层注册项：Inline Editor（草稿是事务输入，不进注册表）。 */
function renderEditingOverlay(props: OverlayRenderProps) {
  return props.editingObjectId !== null ? <InlineTextEditor props={props} /> : null;
}

/** Inspector 文本分区（18A 行为：字段只读，Unicode 可编辑提交）。 */
function TextInspectorSection({ inspector, focusedPageRotation, ctx, actions }: InspectorRenderProps) {
  const text = inspector?.text ?? null;
  const [draft, setDraft] = useState('');
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
          value: `第 ${inspector?.page ?? '—'} 页 · z=${inspector?.z_index ?? '—'}`,
          mono: true,
        },
        { label: '旋转', value: focusedPageRotation === null ? '—' : `${focusedPageRotation}°`, mono: true },
        { label: '版本', value: inspector ? `v${inspector.version}（${inspector.lifecycle}）` : '—', mono: true },
      ]
    : [{ label: '对象', value: '未选中文本对象', muted: true }];

  return (
    <PropertySection title="文本">
      <PropertyGrid rows={rows} />
      {ctx.editing.active ? (
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
                actions.replaceText(draft);
                actions.commit();
              }
              if (event.key === 'Escape') {
                event.preventDefault();
                actions.cancel();
              }
            }}
          />
          <div className="dw-prop-actions">
            <Button
              size="sm"
              variant="primary"
              disabled={draft.trim().length === 0}
              onClick={() => {
                actions.replaceText(draft);
                actions.commit();
              }}
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
          if (event.key === 'Enter') commit();
        }}
      />
      / {ctx.pageCount || '—'} 页
    </span>
  );
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
    { id: 'ocr', label: 'OCR', icon: 'ocr', order: 40, cursor: 'ocr', enabled: () => false, reason: 'OCR 由 19 章接入' },
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
  ],

  // ---- Inspector Framework：分区由注册表渲染 ----
  inspectors: [
    { id: 'document', title: '文档', order: 10, matches: () => true, render: DocumentInspectorSection },
    {
      id: 'object', title: '对象', order: 15, matches: () => true, render: ({ inspector }) => (
        <PropertySection title="对象">
          <PropertyGrid
            rows={inspector
              ? [
                  { label: 'ObjectId', value: `#${inspector.object_id}`, mono: true },
                  { label: '类型', value: `${inspector.kind_label}（${inspector.kind}）`, mono: true },
                  { label: '页码', value: `第 ${inspector.page} 页`, mono: true },
                  {
                    label: 'Bounding Box',
                    value: inspector.bbox.map((value) => value.toFixed(1)).join(', '),
                    mono: true,
                  },
                  { label: '层级 z', value: String(inspector.z_index), mono: true },
                  { label: '版本', value: `v${inspector.version}`, mono: true },
                  { label: '生命周期', value: inspector.lifecycle, mono: true },
                ]
              : [{ label: '对象', value: '未选中对象', muted: true }]}
          />
        </PropertySection>
      ),
    },
    {
      id: 'text', title: '文本', order: 20, matches: () => true, render: (props) => <TextInspectorSection {...props} />,
    },
    {
      id: 'image', title: '图片', order: 30, matches: () => true, render: ({ inspector }) => (
        <PropertySection title="图片">
          <PropertyGrid rows={imageRows(inspector)} />
          <div className="dw-prop-actions">
            <Button size="sm" disabled title="替换图片属 18-07 图片编辑，IPC 未接入">替换</Button>
            <Button size="sm" disabled title="裁剪属页面编辑扩展，尚未接入">裁剪</Button>
          </div>
        </PropertySection>
      ),
    },
    {
      id: 'path', title: '路径', order: 40, matches: () => true, render: ({ inspector }) => (
        <PropertySection title="路径">
          <PropertyGrid
            rows={inspector?.path
              ? [
                  { label: '指令数', value: String(inspector.path.command_count), mono: true },
                  { label: '描边', value: inspector.path.stroke ?? '—', mono: true },
                  { label: '填充', value: inspector.path.fill ?? '—', mono: true },
                  { label: '线宽', value: '未采集', muted: true },
                ]
              : [{ label: '对象', value: '未选中路径对象', muted: true }]}
          />
        </PropertySection>
      ),
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
    { id: 'ocr.main', tab: 'ocr', label: 'OCR（19 章）', order: 10 },
    { id: 'annotation.main', tab: 'annotation', label: '批注（20 章）', order: 10 },
    { id: 'ai.main', tab: 'ai', label: 'AI（21 章）', order: 10 },
    { id: 'view.zoom', tab: 'view', label: '缩放', order: 10 },
    { id: 'view.window', tab: 'view', label: '窗口', order: 20 },
    { id: 'view.theme', tab: 'view', label: '主题', order: 30 },
  ],
  ribbonCommands: [
    { id: 'open', tab: 'home', group: 'home.document', label: '打开', icon: 'open', order: 10, needs: ['always'], run: (a) => a.open() },
    { id: 'save', tab: 'home', group: 'home.document', label: '保存', icon: 'save', order: 20, variant: 'primary', needs: ['document'], run: (a) => a.save() },
    { id: 'saveAs', tab: 'home', group: 'home.document', label: '另存为', icon: 'saveAs', order: 30, needs: ['document'], run: (a) => a.saveAs() },
    { id: 'undo', tab: 'home', group: 'home.history', label: '撤销', icon: 'undo', order: 10, needs: ['undo'], run: (a) => a.undo() },
    { id: 'redo', tab: 'home', group: 'home.history', label: '重做', icon: 'redo', order: 20, needs: ['redo'], run: (a) => a.redo() },
    { id: 'print', tab: 'home', group: 'home.output', label: '打印', icon: 'print', order: 10, needs: [], reason: '打印能力由后续章节接入', run: () => undefined },
    { id: 'export', tab: 'home', group: 'home.output', label: '导出', icon: 'export', order: 20, needs: [], reason: '导出能力由后续章节接入', run: () => undefined },

    { id: 'mode.browse', tab: 'edit', group: 'edit.mode', label: '浏览', icon: 'browse', order: 10, needs: ['always'], active: (ctx) => ctx.mode === 'browse', run: (a) => a.setMode('browse') },
    { id: 'mode.select', tab: 'edit', group: 'edit.mode', label: '选择', icon: 'select', order: 20, needs: ['document'], active: (ctx) => ctx.mode === 'select', run: (a) => a.setMode('select') },
    {
      id: 'mode.edit', tab: 'edit', group: 'edit.mode', label: '编辑文本', icon: 'editText', order: 30,
      needs: ['document', 'text'], active: (ctx) => ctx.mode === 'edit',
      reason: '先选中一个文本对象', run: (a) => a.setMode('edit'),
    },
    { id: 'image.replace', tab: 'edit', group: 'edit.object', label: '替换图片', icon: 'image', order: 10, needs: ['image'], reason: '替换图片属 18-07 图片编辑，IPC 未接入', run: () => undefined },
    { id: 'crop', tab: 'edit', group: 'edit.object', label: '裁剪', icon: 'crop', order: 20, needs: [], reason: '裁剪属页面编辑扩展，尚未接入', run: () => undefined },
    { id: 'path.edit', tab: 'edit', group: 'edit.object', label: '路径', icon: 'path', order: 30, needs: ['path'], reason: '先选中一个路径对象', run: () => undefined },
    { id: 'rotateLeft', tab: 'edit', group: 'edit.page', label: '左转', icon: 'rotateLeft', order: 10, needs: ['document'], run: (a) => a.rotate(-90) },
    { id: 'rotateRight', tab: 'edit', group: 'edit.page', label: '右转', icon: 'rotateRight', order: 20, needs: ['document'], run: (a) => a.rotate(90) },
    { id: 'deletePages', tab: 'edit', group: 'edit.page', label: '删除页', icon: 'delete', order: 30, variant: 'danger', needs: ['multiPage'], reason: '只有一页，无法删除', run: (a) => a.deletePages() },
    { id: 'edit.ocr', tab: 'edit', group: 'edit.recognize', label: 'OCR 识别', icon: 'ocr', order: 10, needs: [], reason: 'OCR 由 19 章接入', run: () => undefined },

    { id: 'ocr.recognize', tab: 'ocr', group: 'ocr.main', label: '识别', icon: 'ocr', order: 10, needs: [], reason: 'OCR 由 19 章接入', run: () => undefined },
    { id: 'ocr.merge', tab: 'ocr', group: 'ocr.main', label: '合并文本层', icon: 'layers', order: 20, needs: [], reason: 'OCR 由 19 章接入', run: () => undefined },
    { id: 'ocr.correct', tab: 'ocr', group: 'ocr.main', label: '校正', icon: 'editText', order: 30, needs: [], reason: 'OCR 由 19 章接入', run: () => undefined },
    { id: 'ocr.export', tab: 'ocr', group: 'ocr.main', label: '导出文本', icon: 'export', order: 40, needs: [], reason: 'OCR 由 19 章接入', run: () => undefined },

    { id: 'annotation.highlight', tab: 'annotation', group: 'annotation.main', label: '高亮', icon: 'annotation', order: 10, needs: [], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.underline', tab: 'annotation', group: 'annotation.main', label: '下划线', icon: 'annotation', order: 20, needs: [], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.comment', tab: 'annotation', group: 'annotation.main', label: '批注', icon: 'annotation', order: 30, needs: [], reason: '批注由 20 章接入', run: () => undefined },
    { id: 'annotation.signature', tab: 'annotation', group: 'annotation.main', label: '签名', icon: 'image', order: 40, needs: [], reason: '批注由 20 章接入', run: () => undefined },

    { id: 'ai.summarize', tab: 'ai', group: 'ai.main', label: '摘要', icon: 'ai', order: 10, needs: [], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.translate', tab: 'ai', group: 'ai.main', label: '翻译', icon: 'ai', order: 20, needs: [], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.rewrite', tab: 'ai', group: 'ai.main', label: '改写', icon: 'ai', order: 30, needs: [], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.extract', tab: 'ai', group: 'ai.main', label: '提取', icon: 'ai', order: 40, needs: [], reason: 'AI 由 21 章接入', run: () => undefined },
    { id: 'ai.chat', tab: 'ai', group: 'ai.main', label: '对话', icon: 'ai', order: 50, needs: [], reason: 'AI 由 21 章接入', run: () => undefined },

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
        { id: 'undo', label: '撤销', icon: 'undo', separatorBefore: true, disabled: !ctx.canUndo, run: () => actions.undo() },
        { id: 'redo', label: '重做', icon: 'redo', disabled: !ctx.canRedo, run: () => actions.redo() },
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
  ],

  // ---- Theme Framework：token 名册（值在 tokens.css） ----
  themeTokens: [
    { id: 'accent', cssVar: '--dw-accent', usage: '主强调色：选中框、主按钮、光标提示' },
    { id: 'bg', cssVar: '--dw-bg', usage: '工作台背景' },
    { id: 'paper', cssVar: '--dw-paper', usage: '画布纸张背景' },
    { id: 'text', cssVar: '--dw-text', usage: '正文文字' },
    { id: 'danger', cssVar: '--dw-danger', usage: '危险操作 / 错误提示' },
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

  // ---- Status Framework：统一状态栏项 ----
  statusItems: [
    { id: 'zoom', label: '缩放', order: 10, value: (ctx) => `${ctx.zoomPercent}%` },
    { id: 'page', label: '页码', order: 20, render: (ctx, actions) => <PageStatusInput ctx={ctx} actions={actions} /> },
    {
      id: 'selection', label: '选择', order: 30,
      value: (ctx) => {
        const count = ctx.selection.selected.length;
        if (count === 0) return `Selection(0) · ${ctx.selectionValidation}`;
        return `Selection(${count}, primary #${ctx.selection.primary ?? '—'}) · ${ctx.selectionValidation}`;
      },
      tone: (ctx) => (ctx.selectionValidation === '失效' ? 'warn' : undefined),
    },
    { id: 'tool', label: '当前工具', order: 35, value: (ctx) => `工具(${ctx.toolLabel})` },
    { id: 'mode', label: '光标模式', order: 40, value: (ctx) => ctx.mode },
    {
      id: 'editing', label: '编辑', order: 50,
      value: (ctx) => (ctx.editing.active ? `编辑中 #${ctx.editing.objectId ?? '—'}` : '—'),
      tone: (ctx) => (ctx.editing.active ? 'ok' : undefined),
    },
    { id: 'dirty', label: '未保存', order: 60, value: (ctx) => (ctx.dirty ? '未保存' : '已保存'), tone: (ctx) => (ctx.dirty ? 'warn' : 'ok') },
    { id: 'undo', label: '撤销', order: 70, value: (ctx) => (ctx.canUndo ? '可撤销' : '—') },
    { id: 'redo', label: '重做', order: 80, value: (ctx) => (ctx.canRedo ? '可重做' : '—') },
    {
      id: 'save', label: '保存', order: 90,
      value: (ctx) => {
        if (ctx.saveFeedback === 'saving') return '保存中…';
        if (ctx.saveFeedback === 'failed') return ctx.saveMessage || '保存失败';
        if (ctx.saveFeedback === 'success') return `${ctx.saveMessage || '保存成功'}${ctx.saveMessage ? '' : ''}`;
        return ctx.saveText;
      },
      tone: (ctx) => (ctx.saveFeedback === 'failed' ? 'danger' : ctx.saveFeedback === 'saving' ? 'warn' : ctx.dirty ? 'warn' : 'ok'),
    },
    { id: 'ocr', label: 'OCR', order: 100, value: () => 'OCR 未接入' },
    { id: 'ai', label: 'AI', order: 110, value: () => 'AI 未接入' },
    { id: 'memory', label: '内存', order: 120, value: () => '—' },
    { id: 'parser', label: '解析器', order: 130, value: (ctx) => ctx.parserText },
    { id: 'event', label: '最近事件', order: 140, value: (ctx) => ctx.lastEvent || '—' },
  ],
};
