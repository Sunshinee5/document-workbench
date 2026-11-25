// Workspace 分类定义（规范 16 §三 / §五）。
//
// 固定十类，顺序即导航顺序。新增能力 → 新增 Workspace，禁止新增 Dock Window。
// Batch / Layers 是 Future 分类：显示在导航里但禁用，不得隐藏（§四 同理）。
import type { IconName } from '../ui';

/** Workspace 标识。 */
export type WorkspaceKind =
  | 'pages'
  | 'bookmarks'
  | 'outline'
  | 'objects'
  | 'ocr'
  | 'annotation'
  | 'ai'
  | 'attachments'
  | 'batch'
  | 'layers';

/** 一条导航定义。 */
export interface WorkspaceDef {
  /** 标识。 */
  id: WorkspaceKind;
  /** 中文名。 */
  label: string;
  /** 图标。 */
  icon: IconName;
  /** 是否可用；Future 分类为 false。 */
  enabled: boolean;
  /** 禁用原因（enabled 为 false 时展示）。 */
  reason?: string;
}

/** 十类 Workspace：顺序冻结。 */
export const WORKSPACES: WorkspaceDef[] = [
  { id: 'pages', label: '页面', icon: 'pages', enabled: true },
  { id: 'bookmarks', label: '书签', icon: 'bookmark', enabled: true },
  { id: 'outline', label: '大纲', icon: 'outline', enabled: true },
  { id: 'objects', label: '对象', icon: 'objects', enabled: true },
  { id: 'ocr', label: 'OCR', icon: 'ocr', enabled: true },
  { id: 'annotation', label: '批注', icon: 'annotation', enabled: true },
  { id: 'ai', label: 'AI', icon: 'ai', enabled: true },
  { id: 'attachments', label: '附件', icon: 'attachment', enabled: true },
  {
    id: 'batch',
    label: '批处理',
    icon: 'batch',
    enabled: false,
    reason: '批处理由 22 章接入',
  },
  {
    id: 'layers',
    label: '图层',
    icon: 'layers',
    enabled: false,
    reason: '图层属 Future 能力',
  },
];

/** 取分类定义。 */
export function workspaceDef(id: WorkspaceKind): WorkspaceDef {
  return WORKSPACES.find((item) => item.id === id) ?? WORKSPACES[0];
}
