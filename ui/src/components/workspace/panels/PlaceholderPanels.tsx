// 尚未接入的 Workspace（规范 16 §三 / §四）。
//
// 规则：能力未实现时**显示禁用态**，不隐藏、不弹「敬请期待」浮窗。
// 这些面板都复用同一个空态容器（C-5 单一实现），只换文案与禁用入口。
import type { ReactNode } from 'react';
import { Button, Panel, PanelEmpty } from '../../ui';

/** 通用空态面板：标题 + 空态 + 可选禁用入口。 */
function EmptyWorkspace({
  title,
  text,
  hint,
  children,
}: {
  title: string;
  text: string;
  hint: string;
  children?: ReactNode;
}) {
  return (
    <Panel title={title} footer={children}>
      <PanelEmpty text={text} hint={hint} />
    </Panel>
  );
}

/** 书签：依赖 17 章 Bookmark 数据，尚未接入。 */
export function BookmarksPanel() {
  return (
    <EmptyWorkspace
      title="书签"
      text="暂无书签"
      hint="书签数据将在书签能力接入后出现在这里。"
    >
      <Button size="sm" disabled title="书签能力尚未接入">
        新建书签
      </Button>
    </EmptyWorkspace>
  );
}

/** 大纲：依赖 PDF 书签树（Outlines）解析。 */
export function OutlinePanel() {
  return (
    <EmptyWorkspace
      title="大纲"
      text="暂无大纲"
      hint="该文档没有可用的目录树，或大纲解析尚未接入。"
    />
  );
}

/** 附件：依赖 PDF EmbeddedFiles。 */
export function AttachmentsPanel() {
  return (
    <EmptyWorkspace
      title="附件"
      text="暂无附件"
      hint="PDF 内嵌附件将在附件能力接入后列出。"
    />
  );
}

/** OCR：19 章。入口可见但禁用。 */
export function OcrPanel() {
  return (
    <EmptyWorkspace
      title="OCR"
      text="OCR 尚未接入"
      hint="19 章接入后，这里显示识别结果、置信度与文本层合并状态。"
    >
      <Button size="sm" disabled title="OCR 由 19 章接入">
        识别当前页
      </Button>
    </EmptyWorkspace>
  );
}

/** 批注：20 章。 */
export function AnnotationPanel() {
  return (
    <EmptyWorkspace
      title="批注"
      text="暂无批注"
      hint="20 章接入后，这里显示批注列表、作者与回复。"
    >
      <Button size="sm" disabled title="批注由 20 章接入">
        新建批注
      </Button>
    </EmptyWorkspace>
  );
}

/** AI：21 章。禁止浮窗，统一在本 Workspace 内对话。 */
export function AiPanel() {
  return (
    <Panel
      title="AI"
      footer={
        <div className="dw-ai-composer">
          <textarea
            className="dw-ai-input"
            placeholder="AI 能力由 21 章接入，当前不可用"
            disabled
            rows={2}
          />
          <Button size="sm" variant="primary" disabled title="AI 由 21 章接入">
            发送
          </Button>
        </div>
      }
    >
      <PanelEmpty
        text="AI 尚未接入"
        hint="21 章接入后，这里显示摘要 / 翻译 / 改写结果与会话历史；结果只写 Workspace，不弹浮窗。"
      />
    </Panel>
  );
}
