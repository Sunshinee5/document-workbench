// PagesPanel：页面 Workspace（缩略图 / 选择 / 拖拽排序）。
//
// 只消费 ViewModel 投影，不自己存页序：真值在 Document.page_layout 与 Session.selection。
import { Button, Panel, PanelEmpty } from '../../ui';

export interface PagesPanelProps {
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 当前页序（原始页码）。 */
  pageOrder: number[];
  /** 原始页码 → 顺时针旋转角。 */
  rotations: Record<number, number>;
  /** 选中的显示位置（1 起）。 */
  selectedPages: number[];
  /** 当前页（1 起）。 */
  currentPage: number;
  /** 页结构版本：变化即重建列表，避免拖拽后 key 复用错位。 */
  layoutRevision: number;
  /** 选择页；mode 由修饰键决定。 */
  onSelectPages: (pages: number[], mode: 'replace' | 'toggle' | 'range') => void;
  /** 拖拽排序：from / to 为显示位置（1 起）。 */
  onMovePage: (from: number, to: number) => void;
  /** 清空选择。 */
  onClearSelection: () => void;
}

/** 页面 Workspace。 */
export function PagesPanel({
  hasDocument,
  pageOrder,
  rotations,
  selectedPages,
  currentPage,
  layoutRevision,
  onSelectPages,
  onMovePage,
  onClearSelection,
}: PagesPanelProps) {
  if (!hasDocument) {
    return (
      <Panel title="页面">
        <PanelEmpty text="尚未打开文档" hint="打开 PDF 后可在这里调整页序与选择页。" />
      </Panel>
    );
  }

  return (
    <Panel
      title={`页面（${pageOrder.length}）`}
      footer={
        <Button size="sm" onClick={onClearSelection} disabled={selectedPages.length === 0}>
          清空选择
        </Button>
      }
    >
      <div className="dw-page-list">
        {pageOrder.map((original, index) => {
          const position = index + 1;
          const selected = selectedPages.includes(position);
          const rotation = rotations[original] ?? 0;
          const classes = ['dw-page-item'];
          if (selected) {
            classes.push('is-selected');
          }
          if (position === currentPage) {
            classes.push('is-current');
          }
          return (
            <div
              key={`${layoutRevision}-${original}`}
              className={classes.join(' ')}
              draggable
              onDragStart={(event) => {
                event.dataTransfer.setData('text/plain', String(position));
              }}
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => {
                event.preventDefault();
                const from = Number(event.dataTransfer.getData('text/plain'));
                if (Number.isFinite(from) && from > 0 && from !== position) {
                  onMovePage(from, position);
                }
              }}
              onClick={(event) => {
                const mode = event.shiftKey
                  ? 'range'
                  : event.ctrlKey || event.metaKey
                    ? 'toggle'
                    : 'replace';
                onSelectPages([position], mode);
              }}
            >
              <span className="dw-page-no">{position}</span>
              <span className="dw-page-meta">原第 {original} 页</span>
              {rotation !== 0 ? <span className="dw-page-rot">{rotation}°</span> : null}
            </div>
          );
        })}
      </div>
    </Panel>
  );
}
