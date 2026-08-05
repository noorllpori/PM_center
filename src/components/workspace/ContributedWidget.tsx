import type { ComponentType } from 'react';
import { AlertTriangle, Boxes, CircleCheck, GitBranch, Layers3 } from 'lucide-react';
import type { JsonValue } from '../../types/platform';
import {
  DATA_SOURCE_CONTRIBUTION_BY_ID,
  WIDGET_CONTRIBUTION_BY_ID,
  getContributionUnavailableReason,
  type WidgetContributionDefinition,
} from '../../features/contributionRegistry';
import { readContributionDataSource } from '../../features/contributionDataSources';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';
import {
  ProjectCatalogWidget,
  ProjectDirectoryWidget,
  ProjectQuickActionsWidget,
  RecentProjectsWidget,
} from '../project-home/ProjectHomeWidgets';

function WidgetUnavailableState({ message, contributionId }: { message: string; contributionId: string }) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200">
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
      <span className="min-w-0">
        <span className="block">{message}</span>
        <code className="mt-1 block break-all text-[11px] opacity-75">{contributionId}</code>
      </span>
    </div>
  );
}

export interface ContributedWidgetRendererProps {
  definition: WidgetContributionDefinition;
  value: JsonValue | null;
  executeCommand: (commandId: string, payload?: JsonValue) => Promise<void>;
}

export interface ContributedWidgetRuntime {
  dataSourceValues?: Record<string, JsonValue>;
  executeCommand?: (commandId: string, payload?: JsonValue) => Promise<void> | void;
}

function objectNumber(value: JsonValue | null, key: string) {
  if (!value || Array.isArray(value) || typeof value !== 'object') {
    return 0;
  }
  const candidate = value[key];
  return typeof candidate === 'number' ? candidate : 0;
}

function DiagnosticRegistrySummaryWidget({
  definition,
  value,
}: ContributedWidgetRendererProps) {
  const metrics = [
    { label: '模块', value: objectNumber(value, 'moduleCount'), icon: Boxes },
    { label: '运行中', value: objectNumber(value, 'runningModuleCount'), icon: CircleCheck },
    { label: '有效贡献', value: objectNumber(value, 'contributionCount'), icon: Layers3 },
    { label: '冲突', value: objectNumber(value, 'conflictCount'), icon: GitBranch },
  ];

  return (
    <section className="overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
      <div className="flex items-center justify-between gap-3 border-b border-gray-100 px-4 py-3 dark:border-gray-800">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{definition.title}</h3>
          <p className="mt-0.5 truncate text-xs text-gray-500 dark:text-gray-400">
            {definition.description}
          </p>
        </div>
        <span className="h-2.5 w-2.5 shrink-0 rounded-full bg-emerald-500" title="数据源已连接" />
      </div>
      <div className="grid grid-cols-2 divide-x divide-y divide-gray-100 sm:grid-cols-4 sm:divide-y-0 dark:divide-gray-800">
        {metrics.map((metric) => {
          const Icon = metric.icon;
          return (
            <div key={metric.label} className="min-w-0 px-4 py-3">
              <div className="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
                <Icon className="h-3.5 w-3.5" />
                <span>{metric.label}</span>
              </div>
              <p className="mt-1 text-xl font-semibold text-gray-900 dark:text-gray-100">
                {metric.value}
              </p>
            </div>
          );
        })}
      </div>
    </section>
  );
}

const WIDGET_RENDERERS: Record<string, ComponentType<ContributedWidgetRendererProps>> = {
  'builtin.project-manager.project-directory-widget': ProjectDirectoryWidget,
  'builtin.project-manager.quick-actions-widget': ProjectQuickActionsWidget,
  'builtin.project-manager.recent-projects-widget': RecentProjectsWidget,
  'builtin.project-manager.project-catalog-widget': ProjectCatalogWidget,
  'diagnostic.contribution-sample.registry-widget': DiagnosticRegistrySummaryWidget,
};

export function getContributedWidgetRendererIds() {
  return Object.keys(WIDGET_RENDERERS);
}

export function ContributedWidget({
  widgetId,
  dataSourceId,
  runtime,
}: {
  widgetId: string;
  dataSourceId?: string;
  runtime?: ContributedWidgetRuntime;
}) {
  const snapshot = useContributionRegistryStore((state) => state.snapshot);
  const definition = WIDGET_CONTRIBUTION_BY_ID.get(widgetId);
  if (!definition) {
    return <WidgetUnavailableState message="Widget 定义未注册" contributionId={widgetId} />;
  }

  const unavailableReason = getContributionUnavailableReason(snapshot, definition);
  if (unavailableReason) {
    return <WidgetUnavailableState message={unavailableReason} contributionId={definition.id} />;
  }

  const resolvedDataSourceId = dataSourceId || definition.dataSourceId;
  const dataSource = resolvedDataSourceId
    ? DATA_SOURCE_CONTRIBUTION_BY_ID.get(resolvedDataSourceId)
    : undefined;
  if (resolvedDataSourceId && !dataSource) {
    return (
      <WidgetUnavailableState
        message="Widget 引用的 DataSource 未注册"
        contributionId={resolvedDataSourceId}
      />
    );
  }
  const result = dataSource
    ? readContributionDataSource(snapshot, dataSource, runtime?.dataSourceValues)
    : { value: null, error: null };
  const Renderer = WIDGET_RENDERERS[definition.id];

  if (!Renderer || result.error) {
    return (
      <WidgetUnavailableState
        message={result.error || 'Widget 尚未注册渲染器'}
        contributionId={definition.id}
      />
    );
  }

  return (
    <Renderer
      definition={definition}
      value={result.value}
      executeCommand={async (commandId, payload) => {
        if (!runtime?.executeCommand) {
          throw new Error(`Widget 未绑定命令运行时：${commandId}`);
        }
        await runtime.executeCommand(commandId, payload);
      }}
    />
  );
}
