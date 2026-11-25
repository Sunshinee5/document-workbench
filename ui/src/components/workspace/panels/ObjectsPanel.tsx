// ObjectsPanel：对象 Workspace（规范 16 §十一）。
//
// 展示 `Page → Text / Image / Path / Unknown`，支持点击定位、Hover 联动、聚焦联动。
// 数据来自只读投影 `get_object_tree`：UI 不解析 PDF，也不产生任何编辑。
import { useMemo } from 'react';
import { Panel, PanelEmpty, Tree, type IconName, type TreeNode } from '../../ui';
import type { ObjectNodeInfo, ObjectTreeInfo } from '../../../ipc';

export interface ObjectsPanelProps {
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 对象树投影。 */
  tree: ObjectTreeInfo | null;
  /** 是否正在解析。 */
  loading: boolean;
  /** 解析失败原因。 */
  error: string;
  /** 当前聚焦对象（UI 聚焦，不是 Selection 真值）。 */
  focusedObjectId: number | null;
  /** 展开集合。 */
  expanded: string[];
  /** 展开 / 折叠节点。 */
  onToggle: (id: string) => void;
  /** 聚焦对象；传 null 取消聚焦。 */
  onFocusObject: (object: ObjectNodeInfo | null) => void;
  /** 滚动到指定页。 */
  onLocatePage: (page: number) => void;
}

/** 对象种类 → 图标。 */
function iconOf(kind: string): IconName {
  switch (kind) {
    case 'Text':
      return 'text';
    case 'Image':
      return 'image';
    case 'Path':
      return 'path';
    case 'Unknown':
      return 'unknown';
    default:
      return 'objects';
  }
}

/** 对象摘要文案：文本优先显示内容，其它显示种类。 */
function labelOf(node: ObjectNodeInfo): string {
  const head = `#${node.object_id}`;
  if (node.kind === 'Text' && node.unicode) {
    const text = node.unicode.replace(/\s+/g, ' ').trim();
    return `${head} ${text.length > 24 ? `${text.slice(0, 24)}…` : text}`;
  }
  return `${head} ${node.kind_label}`;
}

/** 对象 Workspace。 */
export function ObjectsPanel({
  hasDocument,
  tree,
  loading,
  error,
  focusedObjectId,
  expanded,
  onToggle,
  onFocusObject,
  onLocatePage,
}: ObjectsPanelProps) {
  const nodes = useMemo<TreeNode[]>(() => {
    if (!tree) {
      return [];
    }
    return tree.pages.map((page) => ({
      id: `page:${page.page}`,
      label: `第 ${page.page} 页`,
      icon: 'pages' as IconName,
      badge: `${page.object_count} 对象`,
      children: page.objects.map((object) => ({
        id: `obj:${page.page}:${object.object_id}`,
        label: labelOf(object),
        icon: iconOf(object.kind),
        badge: object.kind === 'Text' ? `${Math.round(object.font_size ?? 0)}pt` : undefined,
        muted: object.kind === 'Unknown',
      })),
    }));
  }, [tree]);

  /** 节点 id 里带页码与对象号，避免再查一次表。 */
  const handleSelect = (node: TreeNode) => {
    const [type, pageText, objectText] = node.id.split(':');
    const page = Number(pageText);
    if (type === 'page') {
      onFocusObject(null);
      if (Number.isFinite(page)) {
        onLocatePage(page);
      }
      return;
    }
    const objectId = Number(objectText);
    const object =
      tree?.pages
        .flatMap((item) => item.objects)
        .find((item) => item.object_id === objectId) ?? null;
    onFocusObject(object);
    if (Number.isFinite(page)) {
      onLocatePage(page);
    }
  };

  /** Hover 联动：只做瞬时聚焦，不写 Selection 真值。 */
  const handleHover = (node: TreeNode | null) => {
    if (!node || node.id.startsWith('page:')) {
      onFocusObject(null);
      return;
    }
    const objectId = Number(node.id.split(':')[2]);
    const object =
      tree?.pages
        .flatMap((item) => item.objects)
        .find((item) => item.object_id === objectId) ?? null;
    onFocusObject(object);
  };

  const focusedNodeId =
    focusedObjectId === null
      ? null
      : (nodes
          .flatMap((page) => page.children ?? [])
          .find((child) => child.id.endsWith(`:${focusedObjectId}`))?.id ?? null);

  if (!hasDocument) {
    return (
      <Panel title="对象">
        <PanelEmpty text="尚未打开文档" hint="打开 PDF 后可查看页面对象结构。" />
      </Panel>
    );
  }

  return (
    <Panel
      title="对象"
      actions={<span className="dw-panel-count">{tree ? `${tree.object_count} 个` : '—'}</span>}
    >
      {loading ? <PanelEmpty text="正在解析对象树…" /> : null}
      {!loading && error ? <PanelEmpty text="对象树不可用" hint={error} /> : null}
      {!loading && !error && nodes.length === 0 ? (
        <PanelEmpty text="未解析到对象" hint="该文档可能是扫描件或内容流为空。" />
      ) : null}
      {!loading && !error && nodes.length > 0 ? (
        <Tree
          nodes={nodes}
          expanded={expanded}
          onToggle={onToggle}
          focusedId={focusedNodeId}
          onSelect={handleSelect}
          onHover={handleHover}
        />
      ) : null}
      {tree && tree.warnings.length > 0 ? (
        <div className="dw-panel-warnings">
          {tree.warnings.slice(0, 3).map((warning) => (
            <div key={warning} className="dw-panel-warning">
              {warning}
            </div>
          ))}
        </div>
      ) : null}
    </Panel>
  );
}
