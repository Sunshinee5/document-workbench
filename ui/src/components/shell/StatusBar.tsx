// StatusBar：固定布局第六区（规范 16 §九）。
//
// 固定十项：Zoom / Page / Selection / Cursor / Dirty / OCR / AI / Memory / Parser / Save。
// 顺序不可变，新增状态只能进这十项，不新增状态栏。
import { useEffect, useState } from 'react';

export interface StatusBarProps {
  /** 缩放百分比。 */
  zoomPercent: number;
  /** 当前页。 */
  page: number;
  /** 总页数。 */
  pageCount: number;
  /** 选择摘要，如「Selection(Page 2)」。 */
  selectionText: string;
  /** 当前鼠标模式中文。 */
  modeText: string;
  /** 是否有未保存修改。 */
  dirty: boolean;
  /** OCR 状态文案。 */
  ocrText: string;
  /** AI 状态文案。 */
  aiText: string;
  /** 内存占用文案。 */
  memoryText: string;
  /** 解析器状态文案。 */
  parserText: string;
  /** 保存状态文案。 */
  saveText: string;
  /** 最近一次任务事件（保存耗时 / 增量信息）。 */
  taskEvent: string;
  /** 错误文案，非空时以危险色显示在右侧。 */
  error: string;
  /** 跳页：页码项可直接输入，回车生效。 */
  onGoToPage: (page: number) => void;
}

/** 单项：标签 + 值。 */
function Item({ label, value, tone }: { label: string; value: string; tone?: 'danger' | 'warn' | 'ok' }) {
  const classes = ['dw-status-item'];
  if (tone) {
    classes.push(`is-${tone}`);
  }
  return (
    <span className={classes.join(' ')} title={label}>
      {value}
    </span>
  );
}

/** 状态栏。 */
export function StatusBar(props: StatusBarProps) {
  const {
    zoomPercent,
    page,
    pageCount,
    selectionText,
    modeText,
    dirty,
    ocrText,
    aiText,
    memoryText,
    parserText,
    saveText,
    taskEvent,
    error,
    onGoToPage,
  } = props;

  // 页码输入草稿：只服务输入过程，提交后仍以 page 为准（唯一真值在 view_state）。
  const [draft, setDraft] = useState(String(page));
  useEffect(() => {
    setDraft(String(page));
  }, [page]);

  const commit = () => {
    const parsed = Number.parseInt(draft, 10);
    if (Number.isFinite(parsed)) {
      onGoToPage(parsed);
    } else {
      setDraft(String(page));
    }
  };

  return (
    <footer className="dw-statusbar">
      <Item label="缩放" value={`${zoomPercent}%`} />
      <span className="dw-status-sep" />
      <span className="dw-status-item dw-status-page" title="页码">
        第
        <input
          className="dw-status-page-input"
          value={draft}
          disabled={pageCount === 0}
          aria-label="跳转到页码"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              commit();
            }
          }}
        />
        / {pageCount || '—'} 页
      </span>
      <span className="dw-status-sep" />
      <Item label="选择" value={selectionText} />
      <span className="dw-status-sep" />
      <Item label="光标模式" value={modeText} />
      <span className="dw-status-sep" />
      <Item label="未保存" value={dirty ? '未保存' : '已保存'} tone={dirty ? 'warn' : 'ok'} />
      <span className="dw-status-sep" />
      <Item label="OCR" value={ocrText} />
      <span className="dw-status-sep" />
      <Item label="AI" value={aiText} />
      <span className="dw-status-sep" />
      <Item label="内存" value={memoryText} />
      <span className="dw-status-sep" />
      <Item label="解析器" value={parserText} />
      <span className="dw-status-sep" />
      <Item label="保存" value={saveText} />
      {taskEvent ? <span className="dw-status-event">{taskEvent}</span> : null}
      <span className="dw-status-spacer" />
      {error ? <span className="dw-status-error">{error}</span> : null}
    </footer>
  );
}
