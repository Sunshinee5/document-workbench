// Inspector：固定布局第五区（规范 16 §八）。
//
// 右侧固定面板，禁止按功能弹窗。分区固定五项：
// Document / Text / Image / Annotation / AI，顺序不可调整。
import { useState } from 'react';
import { Button, PropertyGrid, PropertySection, type PropertyRow } from '../ui';
import type { InspectorInfo } from '../../ipc';

export interface InspectorDocumentInfo {
  /** 文档标题。 */
  title: string;
  /** 作者。 */
  author: string;
  /** 页数。 */
  pageCount: number;
  /** 文件大小（字节）。 */
  fileSize: number;
  /** 源文件路径。 */
  path: string;
  /** 是否有未保存修改。 */
  dirty: boolean;
  /** 最近保存时间（HH:MM）；未保存过为空串。 */
  lastSavedAt: string;
  /** 解析器状态文案。 */
  parserText: string;
}

export interface InspectorProps {
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 文档级信息。 */
  document: InspectorDocumentInfo;
  /** Core 返回的 Inspector 投影；null 表示未选中。 */
  inspector: InspectorInfo | null;
  /** 聚焦对象所在页的旋转角。 */
  focusedPageRotation: number | null;
  /** 是否处于编辑事务中。 */
  editing: boolean;
  /** 开始编辑（ReplaceText 预填当前文本）。 */
  onBeginEdit: () => void;
  /** 提交编辑。 */
  onCommit: () => void;
  /** 取消编辑。 */
  onCancel: () => void;
  /** 提交新的文本内容。 */
  onReplaceText: (text: string) => void;
  /** 文件大小格式化函数（由调用方统一，避免两处实现）。 */
  formatSize: (bytes: number) => string;
}

/** 文本分区：字段来自 Core 投影，颜色未采集（C-5）。 */
function TextSection({
  inspector,
  pageRotation,
  editing,
  onBeginEdit,
  onCommit,
  onCancel,
  onReplaceText,
}: {
  inspector: InspectorInfo | null;
  pageRotation: number | null;
  editing: boolean;
  onBeginEdit: () => void;
  onCommit: () => void;
  onCancel: () => void;
  onReplaceText: (text: string) => void;
}) {
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
        { label: '旋转', value: pageRotation === null ? '—' : `${pageRotation}°`, mono: true },
        {
          label: '版本',
          value: inspector ? `v${inspector.version}（${inspector.lifecycle}）` : '—',
          mono: true,
        },
      ]
    : [{ label: '对象', value: '未选中文本对象', muted: true }];

  return (
    <PropertySection title="文本">
      <PropertyGrid rows={rows} />
      {editing ? (
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
                onReplaceText(draft);
                onCommit();
              }
              if (event.key === 'Escape') {
                event.preventDefault();
                onCancel();
              }
            }}
          />
          <div className="dw-prop-actions">
            <Button
              size="sm"
              variant="primary"
              disabled={draft.trim().length === 0}
              onClick={() => {
                onReplaceText(draft);
                onCommit();
              }}
            >
              提交
            </Button>
            <Button size="sm" onClick={onCancel}>
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
              onBeginEdit();
            }}
          >
            编辑文字
          </Button>
        </div>
      )}
    </PropertySection>
  );
}

/** 图片分区：压缩方式未采集（C-5），分辨率由 bbox 推算。 */
function ImageSection({ inspector }: { inspector: InspectorInfo | null }) {
  const image = inspector?.image ?? null;
  const rows: PropertyRow[] = image
    ? [
        { label: 'ObjectId', value: `#${inspector?.object_id ?? ''}`, mono: true },
        { label: '像素', value: `${image.pixel_width} × ${image.pixel_height}`, mono: true },
        {
          label: '显示尺寸',
          value: inspector
            ? `${inspector.bbox[2].toFixed(1)} × ${inspector.bbox[3].toFixed(1)} pt`
            : '—',
          mono: true,
        },
        { label: '色彩空间', value: image.colorspace || '—', mono: true },
        {
          label: '分辨率',
          value: image.resolution_dpi ? `${image.resolution_dpi.toFixed(0)} DPI` : '—',
          mono: true,
        },
        { label: '压缩', value: image.compression ?? '未采集', muted: !image.compression },
        { label: '流对象', value: `xref ${image.xref}`, mono: true },
      ]
    : [{ label: '对象', value: '未选中图片对象', muted: true }];

  return (
    <PropertySection title="图片">
      <PropertyGrid rows={rows} />
      <div className="dw-prop-actions">
        <Button size="sm" disabled title="替换图片属 18-07 图片编辑，IPC 未接入">
          替换
        </Button>
        <Button size="sm" disabled title="裁剪属页面编辑扩展，尚未接入">
          裁剪
        </Button>
      </div>
    </PropertySection>
  );
}

/** Inspector 主体。 */
export function Inspector({
  hasDocument,
  document,
  inspector,
  focusedPageRotation,
  editing,
  onBeginEdit,
  onCommit,
  onCancel,
  onReplaceText,
  formatSize,
}: InspectorProps) {
  const documentRows: PropertyRow[] = hasDocument
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
    <aside className="dw-inspector" aria-label="属性">
      <header className="dw-inspector-header">
        <span className="dw-inspector-title">属性</span>
        <span className="dw-inspector-hint">
          {inspector ? `#${inspector.object_id} ${inspector.kind_label}` : '未选中对象'}
        </span>
      </header>
      <div className="dw-inspector-body dw-scroll">
        <PropertySection title="文档">
          <PropertyGrid rows={documentRows} />
        </PropertySection>
        <TextSection
          inspector={inspector}
          pageRotation={focusedPageRotation}
          editing={editing}
          onBeginEdit={onBeginEdit}
          onCommit={onCommit}
          onCancel={onCancel}
          onReplaceText={onReplaceText}
        />
        <ImageSection inspector={inspector} />
        <PropertySection title="路径">
          <PropertyGrid
            rows={
              inspector?.path
                ? [
                    { label: '指令数', value: String(inspector.path.command_count), mono: true },
                    { label: '描边', value: inspector.path.stroke ?? '—', mono: true },
                    { label: '填充', value: inspector.path.fill ?? '—', mono: true },
                    { label: '线宽', value: '未采集', muted: true },
                  ]
                : [{ label: '对象', value: '未选中路径对象', muted: true }]
            }
          />
        </PropertySection>
        <PropertySection title="批注">
          <PropertyGrid rows={[{ label: '批注', value: '由 20 章接入', muted: true }]} />
        </PropertySection>
        <PropertySection title="AI">
          <PropertyGrid
            rows={[
              { label: 'Prompt', value: '由 21 章接入', muted: true },
              { label: '上下文', value: '由 21 章接入', muted: true },
              { label: '结果', value: '由 21 章接入', muted: true },
            ]}
          />
        </PropertySection>
      </div>
    </aside>
  );
}
