import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Clock3,
  FlaskConical,
  KeyRound,
  Play,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from 'lucide-react';
import {
  decidePlatformCapability,
  getCapabilityGatewayOverview,
  requestPlatformCapability,
  revokePlatformCapabilityGrant,
  runPlatformCapabilityDiagnostic,
  runPlatformCapabilityOperation,
} from '../../api/platformCapabilities';
import type {
  CapabilityApprovalRequest,
  CapabilityDecision,
  CapabilityDiagnosticScenario,
  CapabilityGatewayCommandError,
  CapabilityGatewayOverview,
  CapabilityOperationResult,
  CapabilitySecurityDiagnosticResult,
} from '../../types/capabilityRuntime';
import type { CapabilityRisk } from '../../types/platform';
import { HelpAssistant } from '../ui/HelpAssistant';

const RISK_LABELS: Record<CapabilityRisk, string> = {
  normal: '普通',
  sensitive: '敏感',
  critical: '关键',
};

const OPERATION_LABELS: Record<string, string> = {
  read: '读取',
  write: '写入',
  delete: '删除',
  execute: '执行',
  connect: '连接',
  notify: '通知',
};

function riskTone(risk: CapabilityRisk) {
  if (risk === 'critical') {
    return 'bg-red-100 text-red-700 dark:bg-red-950/50 dark:text-red-300';
  }
  if (risk === 'sensitive') {
    return 'bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300';
  }
  return 'bg-blue-100 text-blue-700 dark:bg-blue-950/50 dark:text-blue-300';
}

function formatError(error: unknown) {
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object') {
    const typed = error as CapabilityGatewayCommandError;
    const details = typed.details?.length ? `\n${typed.details.join('\n')}` : '';
    return `${typed.code ? `${typed.code}: ` : ''}${typed.message || String(error)}${details}`;
  }
  return String(error);
}

function formatTime(value: number) {
  return new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function scopeLabel(approval: CapabilityApprovalRequest) {
  if (approval.scope.kind === 'none') {
    return '不访问文件路径';
  }
  const root = approval.scope.rootPath || '-';
  const relative = approval.scope.relativePath || '-';
  return `${approval.scope.kind}: ${root} → ${relative}`;
}

export function CapabilityDiagnosticsSection() {
  const [overview, setOverview] = useState<CapabilityGatewayOverview | null>(null);
  const [selectedScenarioId, setSelectedScenarioId] = useState('');
  const [approval, setApproval] = useState<CapabilityApprovalRequest | null>(null);
  const [result, setResult] = useState<CapabilityOperationResult | null>(null);
  const [securityResult, setSecurityResult] = useState<CapabilitySecurityDiagnosticResult | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const next = await getCapabilityGatewayOverview();
      setOverview(next);
      setSelectedScenarioId((current) => current || next.diagnosticScenarios[0]?.id || '');
    } catch (nextError) {
      setError(formatError(nextError));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const scenario = useMemo(
    () => overview?.diagnosticScenarios.find((item) => item.id === selectedScenarioId) || null,
    [overview, selectedScenarioId],
  );

  const executeToken = useCallback(async (token: string, selected: CapabilityDiagnosticScenario) => {
    const operationResult = await runPlatformCapabilityOperation(token, selected.request);
    setResult(operationResult);
    setApproval(null);
  }, []);

  const requestAndRun = useCallback(async () => {
    if (!scenario) {
      return;
    }
    setPending('request');
    setError(null);
    setResult(null);
    try {
      const response = await requestPlatformCapability(scenario.request);
      if (response.status === 'approval-required' && response.approval) {
        setApproval(response.approval);
      } else if (response.status === 'granted' && response.token) {
        await executeToken(response.token.value, scenario);
      } else {
        setError(response.message);
      }
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      await load();
      setPending(null);
    }
  }, [executeToken, load, scenario]);

  const decide = useCallback(async (decision: CapabilityDecision) => {
    if (!approval || !scenario) {
      return;
    }
    setPending(`decision:${decision}`);
    setError(null);
    try {
      const response = await decidePlatformCapability(approval.requestId, decision);
      if (response.status === 'granted' && response.token) {
        await executeToken(response.token.value, scenario);
      } else {
        setApproval(null);
        setResult(null);
      }
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      await load();
      setPending(null);
    }
  }, [approval, executeToken, load, scenario]);

  const revoke = useCallback(async (grantId: string) => {
    if (!confirm('撤销后，下次使用此能力需要重新批准。')) {
      return;
    }
    setPending(`revoke:${grantId}`);
    setError(null);
    try {
      await revokePlatformCapabilityGrant(grantId);
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      await load();
      setPending(null);
    }
  }, [load]);

  const runSecurityTest = useCallback(async () => {
    setPending('security-test');
    setError(null);
    setSecurityResult(null);
    try {
      setSecurityResult(await runPlatformCapabilityDiagnostic());
    } catch (nextError) {
      setError(formatError(nextError));
    } finally {
      await load();
      setPending(null);
    }
  }, [load]);

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-blue-500" />
            <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">Capability 权限网关</h4>
            <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[11px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">R3</span>
            <HelpAssistant
              title="能力权限"
              text={[
                '组件必须先在 manifest 中声明能力，运行时再由网关签发短期令牌。',
                '仅本次：批准一次操作；本次会话：关闭 Nexora 前有效；始终允许：绑定当前组件版本、操作和路径范围。',
                '令牌默认 60 秒有效且只能使用一次，不能跨组件或路径复用。',
              ]}
              placement="bottom-start"
              width={360}
            />
          </div>
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            当前使用隔离诊断组件验证权限；业务组件通过同一权限网关接受检查。
          </p>
        </div>
        <button
          type="button"
          title="刷新权限状态"
          onClick={() => void load()}
          disabled={pending !== null}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
        >
          <RefreshCw className={`h-4 w-4 ${pending ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="mt-3 whitespace-pre-wrap rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-900/50 dark:bg-red-950/20 dark:text-red-300">
          {error}
        </div>
      )}

      <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-end">
        <label className="min-w-0">
          <span className="mb-1 block text-xs font-medium text-gray-600 dark:text-gray-300">诊断场景</span>
          <select
            value={selectedScenarioId}
            onChange={(event) => {
              setSelectedScenarioId(event.target.value);
              setApproval(null);
              setResult(null);
            }}
            disabled={pending !== null}
            className="h-9 w-full rounded-md border border-gray-300 bg-white px-2.5 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-gray-950"
          >
            {overview?.diagnosticScenarios.map((item) => (
              <option key={item.id} value={item.id}>{item.name}</option>
            ))}
          </select>
          {scenario && <span className="mt-1 block text-xs text-gray-500">{scenario.description}</span>}
        </label>
        <button
          type="button"
          onClick={() => void requestAndRun()}
          disabled={!scenario || pending !== null}
          className="inline-flex h-9 items-center justify-center gap-1.5 rounded-md bg-gray-900 px-3 text-xs font-medium text-white disabled:opacity-50 dark:bg-white dark:text-gray-900"
        >
          <Play className="h-3.5 w-3.5" />
          申请并运行
        </button>
      </div>

      {scenario && (
        <div className="mt-3 grid gap-x-6 gap-y-1 border-y border-gray-100 py-3 text-xs text-gray-500 dark:border-gray-800 sm:grid-cols-2">
          <span>组件：{scenario.request.componentId || scenario.request.moduleId}</span>
          <span>能力：{scenario.request.capability}</span>
          <span>操作：{OPERATION_LABELS[scenario.request.operation]}</span>
          <span className="truncate" title={scenario.request.scope.rootPath || ''}>范围：{scenario.request.scope.kind}</span>
        </div>
      )}

      {approval && (
        <div className="mt-4 overflow-hidden rounded-md border border-amber-300 bg-amber-50/60 dark:border-amber-900/70 dark:bg-amber-950/20">
          <div className="flex items-start gap-3 border-b border-amber-200 px-3 py-3 dark:border-amber-900/60">
            <KeyRound className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-semibold text-gray-900 dark:text-gray-100">权限请求</span>
                <span className={`rounded px-1.5 py-0.5 text-[11px] ${riskTone(approval.risk)}`}>
                  {RISK_LABELS[approval.risk]}风险
                </span>
              </div>
              <p className="mt-1 text-xs text-gray-700 dark:text-gray-300">
                {approval.subjectName} v{approval.subjectVersion}，所属 {approval.moduleName} v{approval.moduleVersion}
              </p>
            </div>
          </div>
          <dl className="grid gap-x-6 gap-y-2 px-3 py-3 text-xs sm:grid-cols-[88px_minmax(0,1fr)]">
            <dt className="text-gray-500">需要能力</dt><dd className="break-all">{approval.capability} · {OPERATION_LABELS[approval.operation]}</dd>
            <dt className="text-gray-500">申请原因</dt><dd>{approval.reason}</dd>
            <dt className="text-gray-500">访问范围</dt><dd className="break-all">{scopeLabel(approval)}</dd>
            <dt className="text-gray-500">请求有效期</dt><dd>{formatTime(approval.expiresAt)} 前</dd>
          </dl>
          <div className="flex flex-wrap justify-end gap-2 border-t border-amber-200 px-3 py-2.5 dark:border-amber-900/60">
            <button type="button" onClick={() => void decide('deny')} disabled={pending !== null} className="h-8 rounded-md px-3 text-xs text-gray-600 hover:bg-white/70 disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-900">拒绝</button>
            <button type="button" onClick={() => void decide('allowOnce')} disabled={pending !== null} className="h-8 rounded-md border border-amber-300 bg-white px-3 text-xs font-medium text-gray-700 disabled:opacity-50 dark:border-amber-800 dark:bg-gray-900 dark:text-gray-200">仅本次</button>
            <button type="button" onClick={() => void decide('allowSession')} disabled={pending !== null} className="h-8 rounded-md border border-amber-300 bg-white px-3 text-xs font-medium text-gray-700 disabled:opacity-50 dark:border-amber-800 dark:bg-gray-900 dark:text-gray-200">本次会话</button>
            <button type="button" onClick={() => void decide('allowAlways')} disabled={pending !== null} className="h-8 rounded-md bg-amber-600 px-3 text-xs font-medium text-white disabled:opacity-50">始终允许</button>
          </div>
        </div>
      )}

      {result && (
        <div className="mt-3 flex items-start gap-2 rounded-md bg-emerald-50 px-3 py-2 text-xs text-emerald-700 dark:bg-emerald-950/20 dark:text-emerald-300">
          <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>{result.message}{result.resolvedPath ? ` · ${result.resolvedPath}` : ''}</span>
        </div>
      )}

      <div className="mt-5 border-t border-gray-200 pt-4 dark:border-gray-800">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h5 className="text-xs font-semibold text-gray-800 dark:text-gray-200">长期授权</h5>
            <p className="mt-0.5 text-[11px] text-gray-500">组件版本变化后自动失效。</p>
          </div>
          <span className="text-xs text-gray-500">{overview?.grants.length || 0} 条</span>
        </div>
        <div className="mt-2 divide-y divide-gray-100 border-y border-gray-100 dark:divide-gray-800 dark:border-gray-800">
          {overview?.grants.length ? overview.grants.map((grant) => (
            <div key={grant.id} className="flex min-w-0 items-center gap-3 py-2.5 text-xs">
              <span className={`shrink-0 rounded px-1.5 py-0.5 ${riskTone(grant.risk)}`}>{RISK_LABELS[grant.risk]}</span>
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-gray-800 dark:text-gray-200">{grant.componentId || grant.moduleId} · {grant.capability}</p>
                <p className="mt-0.5 truncate text-[11px] text-gray-500">{OPERATION_LABELS[grant.operation]} · {grant.scope.kind} · {grant.valid ? '当前版本有效' : '版本已变化'}</p>
              </div>
              <button type="button" title="撤销授权" onClick={() => void revoke(grant.id)} disabled={pending !== null} className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-gray-400 hover:bg-red-50 hover:text-red-600 disabled:opacity-50 dark:hover:bg-red-950/30">
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          )) : <p className="py-3 text-xs text-gray-500">暂无长期授权。</p>}
        </div>
      </div>

      <div className="mt-5 grid gap-4 xl:grid-cols-[minmax(0,1fr)_auto]">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Clock3 className="h-3.5 w-3.5 text-gray-400" />
            <h5 className="text-xs font-semibold text-gray-800 dark:text-gray-200">最近审计</h5>
          </div>
          <div className="mt-2 max-h-44 overflow-auto border-y border-gray-100 text-[11px] dark:border-gray-800">
            {overview?.recentAudit.length ? overview.recentAudit.slice(0, 12).map((entry) => (
              <div key={entry.id} className="grid grid-cols-[68px_88px_minmax(0,1fr)] gap-2 border-b border-gray-100 py-1.5 last:border-b-0 dark:border-gray-800">
                <span className="text-gray-400">{formatTime(entry.occurredAt)}</span>
                <span className={entry.outcome === 'denied' ? 'text-red-600 dark:text-red-300' : entry.outcome === 'consumed' ? 'text-emerald-600 dark:text-emerald-300' : 'text-amber-600'}>{entry.outcome}</span>
                <span className="truncate text-gray-600 dark:text-gray-300" title={`${entry.reasonCode}: ${entry.reason}`}>{entry.subjectId} · {entry.capability}</span>
              </div>
            )) : <p className="py-3 text-gray-500">暂无审计记录。</p>}
          </div>
        </div>
        <div className="flex min-w-[210px] flex-col justify-end gap-2">
          {securityResult && (
            <div className={`flex items-start gap-2 text-xs ${securityResult.success ? 'text-emerald-600 dark:text-emerald-300' : 'text-red-600 dark:text-red-300'}`}>
              {securityResult.success ? <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" /> : <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />}
              <span>{securityResult.message}</span>
            </div>
          )}
          <button
            type="button"
            onClick={() => void runSecurityTest()}
            disabled={pending !== null}
            className="inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-gray-200 px-3 text-xs text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-700 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            <FlaskConical className="h-3.5 w-3.5" />
            运行权限安全自检
          </button>
        </div>
      </div>

      <p className="mt-3 break-all text-[10px] text-gray-400">授权数据库：{overview?.databasePath || '加载中...'}</p>
    </section>
  );
}
