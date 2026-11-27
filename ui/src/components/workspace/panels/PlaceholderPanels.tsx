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
  details = [],
  children,
}: {
  title: string;
  text: string;
  hint: string;
  details?: string[];
  children?: ReactNode;
}) {
  return (
    <Panel title={title} footer={children}>
      <PanelEmpty text={text} hint={hint} details={details} />
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

/** Search：21 章（规范 22），本 Sprint 只提供分类入口与统一空态。 */
export function SearchPanel() {
  return (
    <EmptyWorkspace
      title="搜索"
      text="暂无搜索记录"
      hint="21 章接入后，这里显示查询历史、结果列表与来源跳转。"
      details={['查询历史', '结果列表', '来源跳转']}
    >
      <Button size="sm" disabled title="全文搜索由 21 章接入">
        新建搜索
      </Button>
    </EmptyWorkspace>
  );
}

/** Plugin：23 章（规范 24），Marketplace 为 Placeholder。 */
export function PluginPanel() {
  return (
    <EmptyWorkspace
      title="插件"
      text="暂无插件"
      hint="23 章接入后，这里显示已安装 / 已禁用插件、权限、版本与能力。"
      details={[
        'Installed 已安装',
        'Disabled 已禁用',
        'Permission 权限',
        'Version 版本',
        'Capability 能力',
        'Marketplace 插件市场（Placeholder）',
      ]}
    >
      <Button size="sm" disabled title="插件市场由 23 章接入">
        打开插件市场
      </Button>
    </EmptyWorkspace>
  );
}

/** Settings：能力设置入口，本 Sprint 只提供分类入口与统一空态。 */
export function SettingsPanel() {
  return (
    <EmptyWorkspace
      title="设置"
      text="暂无设置项"
      hint="这里将集中展示编辑器、OCR、搜索与插件的偏好设置。"
    />
  );
}

/** Collaboration：24 章（规范 25），六项能力统一 Coming Soon 占位。 */
export function CollaborationPanel() {
  return (
    <EmptyWorkspace
      title="协作"
      text="协作能力尚未接入"
      hint="24 章接入后，这里显示版本、时间线、审计、在线状态、同步与冲突处理。"
      details={['Version 版本', 'Timeline 时间线', 'Audit 审计', 'Presence 在线状态', 'Sync 同步', 'Conflict 冲突']}
    >
      <Button size="sm" disabled title="24 章接入后显示">
        新建协作会话
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
        hint="22 章接入后，这里显示会话与审阅结果；结果只写 Workspace，不弹浮窗。"
        details={['Prompt', 'Review', 'Apply', 'History', 'Context', 'Provider']}
      />
    </Panel>
  );
}
