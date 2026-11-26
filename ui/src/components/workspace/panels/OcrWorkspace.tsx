// OCR Workspace 面板（第19章 §三 / 规范 20）。
//
// 支持全文 / 指定页 / 指定区域 OCR，Session 状态同步，Review 接受/拒绝，Apply 走 Editing Engine。
// UI 只消费 OcrController 投影，不解析 PDF、不改 Object。
import { useState } from 'react';
import { Button, ErrorPanel, Panel, PanelEmpty, Progress, Skeleton, Tooltip } from '../../ui';
import type { OcrState } from '../../../ocr/ocrController';

export interface OcrWorkspaceProps {
  /** OCR  Controller 投影。 */
  state: OcrState;
  /** 是否有活动文档。 */
  hasDocument: boolean;
  /** 启动识别（pages 空为全文；region 为 PDF 点 bbox）。 */
  onStart: (pages: number[], region: [number, number, number, number] | null) => void;
  /** 选择 Provider。 */
  onProvider: (name: string) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onDecide: (index: number, accept: boolean) => void;
  onApply: (index: number) => void;
  onClearCache: () => void;
}

/** 解析「1,2,3」页码串。 */
function parsePages(text: string): number[] {
  return text
    .split(/[,，\s]+/)
    .map((part) => Number.parseInt(part, 10))
    .filter((value) => Number.isFinite(value) && value > 0);
}

/** 解析「x,y,w,h」区域串。 */
function parseRegion(text: string): [number, number, number, number] | null {
  const parts = text.split(/[,，\s]+/).map((part) => Number.parseFloat(part));
  if (parts.length !== 4 || parts.some((value) => !Number.isFinite(value))) return null;
  return [parts[0], parts[1], parts[2], parts[3]];
}

/** OCR 工作区主体。 */
export function OcrWorkspace({
  state,
  hasDocument,
  onStart,
  onProvider,
  onPause,
  onResume,
  onCancel,
  onDecide,
  onApply,
  onClearCache,
}: OcrWorkspaceProps) {
  const [pagesText, setPagesText] = useState('');
  const [regionText, setRegionText] = useState('');
  const session = state.session;
  const running = session.state === 'Running';
  const paused = session.state === 'Paused';
  const availableProviders = state.providers.filter((provider) => provider.available);

  const startFull = () => onStart([], null);
  const startPages = () => {
    const pages = parsePages(pagesText);
    if (pages.length === 0) return;
    onStart(pages, null);
  };
  const startRegion = () => {
    const region = parseRegion(regionText);
    if (!region) return;
    onStart([], region);
  };

  return (
    <Panel
      title="OCR 识别"
      actions={
        <Tooltip text="清空 OCR 结果缓存，避免陈旧结果">
          <button type="button" className="dw-panel-action" onClick={onClearCache}>清缓存</button>
        </Tooltip>
      }
      footer={
        <div className="dw-ocr-footer">
          <Tooltip
            text={running ? '暂停当前识别任务' : paused ? '继续识别任务' : '当前没有运行中的识别任务'}
            disabled={!running && !paused}
          >
            <Button size="sm" disabled={!running && !paused} onClick={running ? onPause : onResume}>
              {running ? '暂停' : paused ? '继续' : '暂停'}
            </Button>
          </Tooltip>
          <Button size="sm" disabled={!running && !paused} onClick={onCancel}>取消</Button>
        </div>
      }
    >
      <ErrorPanel message={state.error || session.error} />

      <div className="dw-ocr-provider">
        <label htmlFor="dw-ocr-provider-select">识别引擎</label>
        <select
          id="dw-ocr-provider-select"
          value={state.provider}
          onChange={(event) => onProvider(event.target.value)}
        >
          {state.providers.map((provider) => (
            <option key={provider.name} value={provider.name}>
              {provider.name}{provider.available ? '' : '（未安装）'}
            </option>
          ))}
        </select>
        <div className="dw-ocr-hint">
          {availableProviders.length === 0
            ? '没有可用的 OCR 引擎；文本层 Provider 通常可用。'
            : `可用引擎：${availableProviders.map((item) => item.name).join('、')}`}
        </div>
      </div>

      {!hasDocument ? (
        <PanelEmpty text="尚未打开文档" hint="打开 PDF 后即可进行 OCR 识别。" />
      ) : (
        <>
          <div className="dw-ocr-actions">
            <Tooltip text="对整篇文档逐页识别（文本层）">
              <Button size="sm" disabled={running} onClick={startFull}>全文 OCR</Button>
            </Tooltip>
            <Tooltip text="指定页码识别，逗号分隔，如 1,3,5">
              <Button size="sm" disabled={running} onClick={startPages}>指定页</Button>
            </Tooltip>
            <Tooltip text="指定 PDF 坐标区域，格式 x,y,w,h">
              <Button size="sm" disabled={running} onClick={startRegion}>区域 OCR</Button>
            </Tooltip>
          </div>
          <input
            className="dw-ocr-input"
            aria-label="指定页"
            placeholder="页码：1,3,5"
            value={pagesText}
            onChange={(event) => setPagesText(event.target.value)}
          />
          <input
            className="dw-ocr-input"
            aria-label="指定区域"
            placeholder="区域：x,y,w,h（PDF 点）"
            value={regionText}
            onChange={(event) => setRegionText(event.target.value)}
          />

          <div className="dw-ocr-session">
            <div className="dw-ocr-status">{session.state_label}</div>
            <Progress value={session.progress} label="识别进度" />
            {running || paused ? <Skeleton rows={2} /> : null}
          </div>

          {session.results.length === 0 ? (
            <PanelEmpty text="暂无 OCR 结果" hint="选择引擎并启动识别后，结果会显示在这里。" />
          ) : (
            <div className="dw-ocr-results">
              {session.results.map((result, index) => {
                const review = session.reviews[index];
                return (
                  <div key={`${result.page}-${index}`} className="dw-ocr-result">
                    <div className="dw-ocr-result-head">
                      <span>第 {result.page} 页 · {Math.round(result.confidence * 100)}%</span>
                      <span className="dw-ocr-result-kind">{result.language}</span>
                    </div>
                    <div className="dw-ocr-result-text">{result.text}</div>
                    <div className="dw-ocr-result-bbox">
                      [{result.bbox.map((value) => value.toFixed(1)).join(', ')}]
                    </div>
                    {review ? (
                      <div className="dw-ocr-review">
                        <div className="dw-ocr-diff">{review.diff}</div>
                        <div className="dw-ocr-result-actions">
                          <Button
                            size="sm"
                            disabled={review.decision === '已接受'}
                            onClick={() => onDecide(index, true)}
                          >
                            {review.decision === '已接受' ? '已接受' : '接受'}
                          </Button>
                          <Button
                            size="sm"
                            disabled={review.decision === '已拒绝'}
                            onClick={() => onDecide(index, false)}
                          >
                            拒绝
                          </Button>
                          <Tooltip
                            text={result.source_object_id === null
                              ? '图像 OCR 结果不能写回对象（需先映射到文本对象）'
                              : review.decision !== '已接受'
                                ? '请先接受该结果'
                                : '经 Editing Engine 写回，可撤销'}
                            disabled={result.source_object_id === null || review.decision !== '已接受'}
                          >
                            <Button
                              size="sm"
                              variant="primary"
                              disabled={result.source_object_id === null || review.decision !== '已接受' || state.applying}
                              onClick={() => onApply(index)}
                            >
                              应用
                            </Button>
                          </Tooltip>
                        </div>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}
    </Panel>
  );
}
