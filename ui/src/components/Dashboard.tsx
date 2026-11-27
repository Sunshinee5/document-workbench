// Dashboard（规范 26 第一部分 §4）：未打开 PDF 时的唯一落地页。
//
// 零新增依赖：只复用统一组件层、GLYPHS 图标与 --dw-* Token。
// 最近列表是纯 UI 偏好，存 localStorage；打开 / 新建一律经 FrameworkActions，
// 不在 UI 内自行解析 PDF、不产生 Command / Diff。
import { useCallback, useEffect, useState } from 'react';
import type { DragEvent } from 'react';
import type { FrameworkActions } from '../framework';
import { Button, Icon, Panel, PanelEmpty, type IconName } from './ui';

/** localStorage 键名：与主题偏好一样属于纯 UI 层，不进 Session。 */
export const DASHBOARD_STORAGE_KEY = 'dw.dashboard.v1';

/** 每类最近记录最多保留条数。 */
const MAX_ENTRIES = 6;

/** 最近记录分类。 */
export type DashboardKind = 'files' | 'ocr' | 'ai' | 'search';

/** 一条最近记录：只存展示投影，不存 Document 真值。 */
export interface DashboardEntry {
  /** 去重 id（文件用绝对路径，其余用能力 id）。 */
  id: string;
  /** 展示标题。 */
  title: string;
  /** 本地绝对路径（仅文件记录具备）。 */
  path?: string;
  /** 附加说明（页数 / 结果数 / 来源文档）。 */
  detail?: string;
  /** 展示时间（HH:MM）。 */
  at: string;
}

/** Dashboard 持久化投影。 */
export interface DashboardData {
  files: DashboardEntry[];
  ocr: DashboardEntry[];
  ai: DashboardEntry[];
  search: DashboardEntry[];
}

/** 空投影：读取失败时统一回落，不让坏数据打断界面。 */
function emptyData(): DashboardData {
  return { files: [], ocr: [], ai: [], search: [] };
}

/** 当前时间标签（本地时区 HH:MM）。 */
function timeLabel(): string {
  const now = new Date();
  return `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
}

/** 逐条校验 localStorage 数据，防止旧版本 / 手工写入破坏渲染。 */
function readEntries(value: unknown): DashboardEntry[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const entries: DashboardEntry[] = [];
  for (const item of value) {
    if (!item || typeof item !== 'object') {
      continue;
    }
    const raw = item as Partial<DashboardEntry>;
    if (typeof raw.id !== 'string' || typeof raw.title !== 'string' || !raw.id || !raw.title) {
      continue;
    }
    entries.push({
      id: raw.id,
      title: raw.title,
      path: typeof raw.path === 'string' ? raw.path : undefined,
      detail: typeof raw.detail === 'string' ? raw.detail : undefined,
      at: typeof raw.at === 'string' ? raw.at : '',
    });
  }
  return entries.slice(0, MAX_ENTRIES);
}

/** 读取最近列表；隐私模式 / 损坏数据都静默回落空投影。 */
export function loadDashboard(): DashboardData {
  try {
    const raw = window.localStorage.getItem(DASHBOARD_STORAGE_KEY);
    if (!raw) {
      return emptyData();
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') {
      return emptyData();
    }
    const data = parsed as Partial<DashboardData>;
    return {
      files: readEntries(data.files),
      ocr: readEntries(data.ocr),
      ai: readEntries(data.ai),
      search: readEntries(data.search),
    };
  } catch {
    return emptyData();
  }
}

/** 追加 / 置顶一条最近记录（同 id 去重），只写 UI 偏好。 */
export function recordDashboard(kind: DashboardKind, entry: Omit<DashboardEntry, 'at'> & { at?: string }): void {
  const data = loadDashboard();
  const next: DashboardEntry = { ...entry, at: entry.at ?? timeLabel() };
  data[kind] = [next, ...data[kind].filter((item) => item.id !== next.id)].slice(0, MAX_ENTRIES);
  try {
    window.localStorage.setItem(DASHBOARD_STORAGE_KEY, JSON.stringify(data));
  } catch {
    // 存不下只影响下次启动的展示，不影响本次会话。
  }
}

/** 各分类图标：只取 GLYPHS 已有字形，不新增图标库。 */
const KIND_ICONS: Record<DashboardKind, IconName> = {
  files: 'pages',
  ocr: 'ocr',
  ai: 'ai',
  search: 'ocr',
};

/** 最近记录卡片：文件可重新打开，其余记录只读展示。 */
function EntryCard({
  kind,
  title,
  entries,
  actions,
  hint,
}: {
  kind: DashboardKind;
  title: string;
  entries: DashboardEntry[];
  actions: FrameworkActions;
  hint: string;
}) {
  const openFile = (entry: DashboardEntry) => {
    if (entry.path && actions.openPath) {
      actions.openPath(entry.path);
    } else {
      actions.open();
    }
  };

  return (
    <Panel title={title} className="dw-dashboard-card" scroll={false}>
      <div className="dw-dashboard-entries">
        {entries.length === 0 ? (
          <PanelEmpty text="暂无数据" hint={hint} />
        ) : entries.map((entry) => {
          const meta = [entry.at, entry.detail].filter(Boolean).join(' · ');
          const content = (
            <>
              <Icon name={KIND_ICONS[kind]} className="dw-dashboard-entry-icon" />
              <span className="dw-dashboard-entry-main">
                <span className="dw-dashboard-entry-title">{entry.title}</span>
                {meta ? <span className="dw-dashboard-entry-meta">{meta}</span> : null}
              </span>
            </>
          );
          return kind === 'files' ? (
            <button
              key={entry.id}
              type="button"
              className="dw-dashboard-entry"
              title={entry.path ? `打开 ${entry.path}` : '重新选择文件'}
              onClick={() => openFile(entry)}
            >
              {content}
            </button>
          ) : (
            <div key={entry.id} className="dw-dashboard-entry" title={entry.detail ?? entry.title}>
              {content}
            </div>
          );
        })}
      </div>
    </Panel>
  );
}

export interface DashboardProps {
  /** 框架动作：打开 / 新建 / 拖入都只转发，不写业务。 */
  actions: FrameworkActions;
}

/** 无文档落地页：九项入口齐全（最近文件 / 继续编辑 / OCR / AI / 搜索 / 示例 / 拖入 / 新建 / 打开）。 */
export function Dashboard({ actions }: DashboardProps) {
  const [data, setData] = useState<DashboardData>(loadDashboard);
  const [dragging, setDragging] = useState(false);
  const [dropError, setDropError] = useState('');

  useEffect(() => {
    const refresh = () => setData(loadDashboard());
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, []);

  /** 拖入 PDF：只识别扩展名与本地路径，真正的打开仍走 FrameworkActions。 */
  const onDrop = useCallback((event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(false);
    setDropError('');
    const file = event.dataTransfer.files.item(0);
    if (!file) {
      return;
    }
    if (!file.name.toLowerCase().endsWith('.pdf')) {
      setDropError('请拖入 PDF 文件');
      return;
    }
    const localPath = (file as File & { path?: string }).path;
    if (!localPath || !actions.openPath) {
      setDropError('当前环境未提供文件路径，请使用「打开」选择该文件');
      return;
    }
    actions.openPath(localPath);
  }, [actions]);

  const onDragOver = useCallback((event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(true);
  }, []);

  const onDragLeave = useCallback(() => setDragging(false), []);

  return (
    <div
      className="dw-dashboard dw-scroll"
      data-dashboard="true"
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
    >
      <div className="dw-dashboard-inner">
        <div className="dw-dashboard-hero">
          <div className="dw-dashboard-heading">
            <span className="dw-dashboard-title">文档工作台</span>
            <span className="dw-dashboard-subtitle">
              打开 PDF 开始阅读、编辑与识别；所有操作都会回到统一工作台。
            </span>
          </div>
          <div className="dw-dashboard-actions">
            <Button
              variant="primary"
              icon="open"
              title="选择 PDF 开始新的编辑会话（独立新建能力尚未接入，复用打开动作）"
              onClick={() => actions.open()}
            >
              新建
            </Button>
            <Button icon="open" title="打开本地 PDF（Ctrl+O）" onClick={() => actions.open()}>
              打开
            </Button>
          </div>
        </div>

        <div className={dragging ? 'dw-dashboard-drop is-over' : 'dw-dashboard-drop'}>
          <Icon name="open" className="dw-dashboard-drop-icon" />
          <div>
            <div>拖入 PDF 到这里开始编辑</div>
            <div className="dw-dashboard-drop-hint">也可以点击右上角「打开」，或按 Ctrl+O。</div>
            {dropError ? <div className="dw-dashboard-drop-error">{dropError}</div> : null}
          </div>
        </div>

        <div className="dw-dashboard-grid">
          <EntryCard kind="files" title="最近文件" entries={data.files} actions={actions} hint="打开过的 PDF 会显示在这里。" />
          <EntryCard kind="files" title="继续编辑" entries={data.files.slice(0, 1)} actions={actions} hint="打开一份 PDF 后即可从这里继续。" />
          <EntryCard kind="ocr" title="最近 OCR" entries={data.ocr} actions={actions} hint="完成 OCR 识别后会留下结果摘要。" />
          <EntryCard kind="ai" title="最近 AI" entries={data.ai} actions={actions} hint="AI 会话接入后显示在这里。" />
          <EntryCard kind="search" title="最近搜索" entries={data.search} actions={actions} hint="发起搜索后会留下查询记录。" />
          <Panel
            title="示例文档"
            scroll={false}
            footer={<Button size="sm" disabled title="内置示例库尚未接入">加载示例</Button>}
          >
            <PanelEmpty text="暂无示例文档" hint="示例库接入后，这里提供一键打开的演示 PDF。" />
          </Panel>
        </div>
      </div>
    </div>
  );
}
