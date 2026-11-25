// Tree：对象树 / 大纲 / 书签共用的唯一树实现（规范 16 C-5）。
//
// 受控组件：展开集合与选中项由调用方持有，树自己不存状态（避免第二份真值）。
import { Icon, type IconName } from './Icon';

/** 树节点：id 必须全局唯一（含父路径语义由调用方保证）。 */
export interface TreeNode {
  /** 稳定标识。 */
  id: string;
  /** 主文案。 */
  label: string;
  /** 前置图标。 */
  icon?: IconName;
  /** 右侧次要文案（页码、对象数等）。 */
  badge?: string;
  /** 次要样式（未知对象等）。 */
  muted?: boolean;
  /** 不可选中。 */
  disabled?: boolean;
  /** 子节点。 */
  children?: TreeNode[];
}

export interface TreeProps {
  /** 根节点列表。 */
  nodes: TreeNode[];
  /** 已展开的节点 id。 */
  expanded: string[];
  /** 展开 / 折叠。 */
  onToggle: (id: string) => void;
  /** 当前聚焦节点 id。 */
  focusedId?: string | null;
  /** 点击节点。 */
  onSelect?: (node: TreeNode) => void;
  /** 悬停节点；离开传 null。 */
  onHover?: (node: TreeNode | null) => void;
}

/** 递归渲染一层节点。 */
function TreeLevel({
  nodes,
  depth,
  expanded,
  focusedId,
  onToggle,
  onSelect,
  onHover,
}: TreeProps & { depth: number }) {
  return (
    <ul className="dw-tree-level" role={depth === 0 ? 'tree' : 'group'}>
      {nodes.map((node) => {
        const hasChildren = Boolean(node.children && node.children.length > 0);
        const isOpen = expanded.includes(node.id);
        const isFocused = focusedId === node.id;
        const classes = ['dw-tree-row'];
        if (isFocused) {
          classes.push('is-focused');
        }
        if (node.muted) {
          classes.push('is-muted');
        }
        return (
          <li key={node.id} role="treeitem" aria-expanded={hasChildren ? isOpen : undefined}>
            <div
              className={classes.join(' ')}
              style={{ paddingLeft: 6 + depth * 14 }}
              onClick={() => {
                if (node.disabled) {
                  return;
                }
                if (hasChildren) {
                  onToggle(node.id);
                }
                onSelect?.(node);
              }}
              onMouseEnter={() => onHover?.(node)}
              onMouseLeave={() => onHover?.(null)}
            >
              <span className="dw-tree-arrow">
                {hasChildren ? <Icon name={isOpen ? 'chevronDown' : 'chevronRight'} /> : null}
              </span>
              {node.icon ? <Icon name={node.icon} /> : null}
              <span className="dw-tree-label" title={node.label}>
                {node.label}
              </span>
              {node.badge ? <span className="dw-tree-badge">{node.badge}</span> : null}
            </div>
            {hasChildren && isOpen ? (
              <TreeLevel
                nodes={node.children ?? []}
                depth={depth + 1}
                expanded={expanded}
                focusedId={focusedId}
                onToggle={onToggle}
                onSelect={onSelect}
                onHover={onHover}
              />
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

/** 统一树。 */
export function Tree(props: TreeProps) {
  if (props.nodes.length === 0) {
    return null;
  }
  return <TreeLevel {...props} depth={0} />;
}
