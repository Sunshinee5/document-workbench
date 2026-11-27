// Workspace 分类定义（规范 16 §三 / §五 / 22 C-2 / 26 第一部分 §3）。
//
// 原固定十类是 R1.0 基线；16 §三同时规定「每新增能力 → 新增 Workspace」，
// 22 C-2 明确允许追加第 11 类 search。此处按 26 再追加 plugin / settings /
// collaboration 三个 UI 占位分类（不新增 Ribbon 页签 / Dock Window，不改六区结构）。
// 既有十类顺序保持原样，新分类统一追加在末尾；Batch / Layers 仍禁用显示。
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
  | 'layers'
  | 'search'
  | 'plugin'
  | 'settings'
  | 'collaboration';

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

/** Workspace 分类：前 10 项冻结，后 4 项按 22 C-2 / 26 §3 追加。 */
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
  // 以下四项为 26 章追加的 UI 入口，Core 未落地前只显示统一空态。
  { id: 'search', label: '搜索', icon: 'search', enabled: true },
  { id: 'plugin', label: '插件', icon: 'plugin', enabled: true },
  { id: 'settings', label: '设置', icon: 'settings', enabled: true },
  { id: 'collaboration', label: '协作', icon: 'layers', enabled: true },
];

/** 取分类定义。 */
export function workspaceDef(id: WorkspaceKind): WorkspaceDef {
  return WORKSPACES.find((item) => item.id === id) ?? WORKSPACES[0];
}
