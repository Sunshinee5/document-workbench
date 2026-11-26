// Workspace 出口（规范 16 §三 / §五）。
export { DocumentTabs, type DocumentTabsProps } from './DocumentTabs';
export { WorkspaceNav, type WorkspaceNavProps } from './WorkspaceNav';
export { WORKSPACES, workspaceDef, type WorkspaceDef, type WorkspaceKind } from './workspaces';
export { PagesPanel, type PagesPanelProps } from './panels/PagesPanel';
export { ObjectsPanel, type ObjectsPanelProps } from './panels/ObjectsPanel';
export {
  AiPanel,
  AnnotationPanel,
  AttachmentsPanel,
  BookmarksPanel,
  OutlinePanel,
} from './panels/PlaceholderPanels';
export { OcrWorkspace, type OcrWorkspaceProps } from './panels/OcrWorkspace';
