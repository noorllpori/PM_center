import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, Braces, CheckCircle2, Database, FlaskConical, Radio, Workflow } from 'lucide-react';
import {
  inspectContributionCatalog,
  type ContributionCatalogReport,
  type ContributionImplementationInventory,
} from '../../features/contributionCatalogDiagnostics';
import {
  DATA_SOURCE_CONTRIBUTIONS,
  WIDGET_CONTRIBUTIONS,
  WORKFLOW_NODE_CONTRIBUTIONS,
  getContributionUnavailableReason,
} from '../../features/contributionRegistry';
import {
  getContributionRegistrySubscriptionDiagnostics,
  runContributionRegistrySubscriptionProbe,
  useContributionRegistryStore,
  type ContributionRegistrySubscriptionProbe,
} from '../../stores/contributionRegistryStore';
import { ContributedWidget } from './ContributedWidget';

function StatusRow({
  label,
  contributionId,
  available,
}: {
  label: string;
  contributionId: string;
  available: boolean;
}) {
  return (
    <div className="flex min-w-0 items-center gap-3 border-b border-gray-100 py-2.5 last:border-b-0 dark:border-gray-800">
      <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${available ? 'bg-emerald-500' : 'bg-gray-300 dark:bg-gray-700'}`} />
      <span className="w-24 shrink-0 text-sm text-gray-700 dark:text-gray-200">{label}</span>
      <code className="min-w-0 flex-1 truncate text-xs text-gray-500 dark:text-gray-400" title={contributionId}>
        {contributionId}
      </code>
    </div>
  );
}

export function ContributionDiagnosticsSurface({
  isActive,
  implementationInventory,
}: {
  isActive: boolean;
  implementationInventory: ContributionImplementationInventory;
}) {
  const snapshot = useContributionRegistryStore((state) => state.snapshot);
  const [subscriptionProbe, setSubscriptionProbe] = useState<ContributionRegistrySubscriptionProbe | null>(null);
  const widget = WIDGET_CONTRIBUTIONS.diagnosticRegistrySummary;
  const dataSource = DATA_SOURCE_CONTRIBUTIONS.diagnosticRegistrySummary;
  const workflowNode = WORKFLOW_NODE_CONTRIBUTIONS.diagnosticEcho;
  const entries = [
    { label: 'Widget', definition: widget },
    { label: 'DataSource', definition: dataSource },
    { label: 'WorkflowNode', definition: workflowNode },
  ];
  const catalogReport = useMemo<ContributionCatalogReport>(
    () => inspectContributionCatalog(snapshot, implementationInventory),
    [implementationInventory, snapshot],
  );
  const subscriptionDiagnostics = getContributionRegistrySubscriptionDiagnostics();

  useEffect(() => {
    if (isActive) {
      setSubscriptionProbe(runContributionRegistrySubscriptionProbe());
    }
  }, [isActive, snapshot]);

  return (
    <div className="h-full min-h-0 overflow-auto bg-gray-50 dark:bg-gray-950">
      <header className="border-b border-gray-200 bg-white px-5 py-4 dark:border-gray-800 dark:bg-gray-900">
        <div className="mx-auto flex max-w-5xl items-start gap-3">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-fuchsia-100 text-fuchsia-700 dark:bg-fuchsia-950/50 dark:text-fuchsia-300">
            <FlaskConical className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">贡献隔离样本</h2>
            <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
              该页面由诊断模块完整声明，用于验证贡献目录、动态挂载和停用撤下。
            </p>
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-5xl space-y-5 px-5 py-5">
        <ContributedWidget widgetId={widget.id} />

        <section className="min-w-0">
          <div className="mb-2 flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              {catalogReport.healthy ? (
                <CheckCircle2 className="h-4 w-4 text-emerald-500" />
              ) : (
                <AlertTriangle className="h-4 w-4 text-amber-500" />
              )}
              <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">目录一致性</h3>
            </div>
            <span className={`text-xs ${catalogReport.healthy ? 'text-emerald-600 dark:text-emerald-300' : 'text-amber-700 dark:text-amber-300'}`}>
              {catalogReport.healthy ? '检查通过' : `${catalogReport.errorCount} 错误 · ${catalogReport.warningCount} 警告`}
            </span>
          </div>
          <div className="overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
            <div className="grid grid-cols-2 divide-x divide-y divide-gray-100 sm:grid-cols-4 sm:divide-y-0 dark:divide-gray-800">
              {[
                ['前端定义', catalogReport.catalogDefinitionCount],
                ['模块定义', catalogReport.moduleOwnedDefinitionCount],
                ['Manifest 声明', catalogReport.manifestClaimCount],
                ['实现注册', catalogReport.rendererCount],
              ].map(([label, value]) => (
                <div key={String(label)} className="px-4 py-3">
                  <p className="text-xs text-gray-500 dark:text-gray-400">{label}</p>
                  <p className="mt-1 text-lg font-semibold text-gray-900 dark:text-gray-100">{value}</p>
                </div>
              ))}
            </div>
            {catalogReport.issues.length > 0 && (
              <div className="border-t border-gray-100 px-4 py-3 dark:border-gray-800">
                {catalogReport.issues.map((issue) => (
                  <p key={`${issue.code}:${issue.contributionId}`} className="py-1 text-xs text-amber-800 dark:text-amber-200">
                    {issue.code} · {issue.contributionId}：{issue.message}
                  </p>
                ))}
              </div>
            )}
          </div>
        </section>

        <section className="min-w-0">
          <div className="mb-2 flex items-center gap-2">
            <Radio className="h-4 w-4 text-sky-500" />
            <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">订阅释放</h3>
          </div>
          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-md border border-gray-200 bg-white px-4 py-3 text-xs text-gray-600 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300">
            <span>当前消费者：{subscriptionDiagnostics.consumerCount}</span>
            <span>事件监听：{subscriptionDiagnostics.listenerInstalled ? '已安装' : '未安装'}</span>
            <span>刷新任务：{subscriptionDiagnostics.refreshInFlight ? '进行中' : '空闲'}</span>
            <span className={subscriptionProbe?.success ? 'text-emerald-600 dark:text-emerald-300' : 'text-amber-700 dark:text-amber-300'}>
              引用计数自检：{subscriptionProbe ? (subscriptionProbe.success ? '通过' : '失败') : '等待页面激活'}
            </span>
          </div>
        </section>

        <div className="grid gap-5 lg:grid-cols-[minmax(0,1.2fr)_minmax(280px,0.8fr)]">
          <section className="min-w-0">
            <div className="mb-2 flex items-center gap-2">
              <Braces className="h-4 w-4 text-blue-500" />
              <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">运行时目录</h3>
            </div>
            <div className="rounded-md border border-gray-200 bg-white px-4 dark:border-gray-700 dark:bg-gray-900">
              {entries.map(({ label, definition }) => (
                <StatusRow
                  key={definition.id}
                  label={label}
                  contributionId={definition.id}
                  available={!getContributionUnavailableReason(snapshot, definition)}
                />
              ))}
            </div>
          </section>

          <section className="min-w-0">
            <div className="mb-2 flex items-center gap-2">
              <Workflow className="h-4 w-4 text-emerald-500" />
              <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100">节点合同</h3>
            </div>
            <div className="rounded-md border border-gray-200 bg-white px-4 py-3 dark:border-gray-700 dark:bg-gray-900">
              <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{workflowNode.title}</p>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{workflowNode.description}</p>
              <div className="mt-3 space-y-2 text-xs text-gray-600 dark:text-gray-300">
                <div className="flex items-center gap-2">
                  <Database className="h-3.5 w-3.5 text-gray-400" />
                  <span>数据源：{dataSource.title}</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-gray-400">IN</span>
                  <span>{workflowNode.inputs.map((port) => `${port.name}:${port.type}`).join('、')}</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-gray-400">OUT</span>
                  <span>{workflowNode.outputs.map((port) => `${port.name}:${port.type}`).join('、')}</span>
                </div>
              </div>
              <p className="mt-3 border-t border-gray-100 pt-3 text-xs text-gray-500 dark:border-gray-800 dark:text-gray-400">
                当前只验证节点目录与端口合同；执行器在 R11 接入。
              </p>
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}
