// Inspector：固定布局第五区（规范 16 §八）。
//
// 右侧固定面板，禁止按功能弹窗。分区固定五项：
// Document / Text / Image / Annotation / AI，顺序不可调整。
import { Button, PropertyGrid, PropertySection, type PropertyRow } from '../ui';
import type { ObjectNodeInfo } from '../../ipc';

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
  /** 当前聚焦对象；null 表示未聚焦。 */
  focusedObject: ObjectNodeInfo | null;
  /** 聚焦对象所在页的旋转角。 */
  focusedPageRotation: number | null;
  /** 文件大小格式化函数（由调用方统一，避免两处实现）。 */
  formatSize: (bytes: number) => string;
}

/** 文本对象分区。 */
function TextSection({
  object,
  pageRotation,
}: {
  object: ObjectNodeInfo | null;
  pageRotation: number | null;
}) {
  const isText = object?.kind === 'Text';
  const rows: PropertyRow[] = isText
    ? [
        { label: 'ObjectId', value: `#${object.object_id}`, mono: true },
        { label: '字体', value: object.font_id ?? '—', mono: true },
        {
          label: '字号',
          value: object.font_size ? `${object.font_size.toFixed(2)} pt` : '—',
          mono: true,
        },
        { label: 'Unicode', value: object.unicode ?? '—' },
        { label: '字形数', value: String(object.glyph_count), mono: true },
        { label: '页 / 层级', value: `第 ${object.page} 页 · z=${object.z_index}`, mono: true },
        { label: '旋转', value: pageRotation === null ? '—' : `${pageRotation}°`, mono: true },
        { label: '版本', value: `v${object.version}（${object.lifecycle}）`, mono: true },
      ]
    : [{ label: '对象', value: '未选中文本对象', muted: true }];

  return (
    <PropertySection title="文本">
      <PropertyGrid rows={rows} />
      <div className="dw-prop-actions">
        <Button size="sm" disabled title="文字编辑入口需 18 章编辑 IPC 桥接后启用">
          编辑文字
        </Button>
        <Button size="sm" disabled title="字体重写由 18-07 落地，UI 入口待 IPC 接入">
          重写字形
        </Button>
      </div>
    </PropertySection>
  );
}

/** 图片对象分区。 */
function ImageSection({ object }: { object: ObjectNodeInfo | null }) {
  const isImage = object?.kind === 'Image';
  const rows: PropertyRow[] = isImage
    ? [
        { label: 'ObjectId', value: `#${object.object_id}`, mono: true },
        { label: '宽 × 高', value: `${object.bbox[2].toFixed(1)} × ${object.bbox[3].toFixed(1)} pt`, mono: true },
        { label: '位置', value: `(${object.bbox[0].toFixed(1)}, ${object.bbox[1].toFixed(1)})`, mono: true },
        { label: '压缩', value: '未读取', muted: true },
        { label: '流对象', value: object.stream_ref === null ? '—' : `xref ${object.stream_ref}`, mono: true },
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
  focusedObject,
  focusedPageRotation,
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
          {focusedObject ? `#${focusedObject.object_id} ${focusedObject.kind_label}` : '未聚焦对象'}
        </span>
      </header>
      <div className="dw-inspector-body dw-scroll">
        <PropertySection title="文档">
          <PropertyGrid rows={documentRows} />
        </PropertySection>
        <TextSection object={focusedObject} pageRotation={focusedPageRotation} />
        <ImageSection object={focusedObject} />
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
