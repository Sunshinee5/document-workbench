// DocumentTabs：Workspace 顶部的文档上下文条（规范 16 C-4）。
//
// 多文档页签归属 Workspace 区域，不是第二套 UI：
// 它只切换「当前活动文档」，不承载任何业务命令。
import { Icon } from '../ui';
import type { OpenTab } from '../../viewmodels';

export interface DocumentTabsProps {
  /** 已打开文档（真值来自 Workspace.tabs）。 */
  tabs: OpenTab[];
  /** 当前活动文档。 */
  activeDocumentId: number | null;
  /** 切换文档。 */
  onSelect: (documentId: number) => void;
  /** 关闭文档。 */
  onClose: (documentId: number) => void;
}

/** 文档页签条。 */
export function DocumentTabs({ tabs, activeDocumentId, onSelect, onClose }: DocumentTabsProps) {
  return (
    <div className="dw-tabs" role="tablist" aria-label="已打开文档">
      {tabs.length === 0 ? (
        <span className="dw-tabs-empty">暂无打开的文档</span>
      ) : (
        tabs.map((tab) => (
          <div
            key={tab.documentId}
            role="tab"
            aria-selected={tab.documentId === activeDocumentId}
            className={tab.documentId === activeDocumentId ? 'dw-tab is-active' : 'dw-tab'}
            onClick={() => onSelect(tab.documentId)}
            title={`${tab.title} · ${tab.pageCount} 页`}
          >
            <span className="dw-tab-title">{tab.title}</span>
            <span className="dw-tab-pages">{tab.pageCount} 页</span>
            <button
              type="button"
              className="dw-tab-close"
              aria-label={`关闭 ${tab.title}`}
              onClick={(event) => {
                event.stopPropagation();
                onClose(tab.documentId);
              }}
            >
              <Icon name="close" />
            </button>
          </div>
        ))
      )}
    </div>
  );
}
