import { AlertTriangle, RefreshCw } from 'lucide-react';
import { useContributionRegistryStore } from '../../stores/contributionRegistryStore';

export function ContributionUnavailableState({
  title,
  contributionId,
  message,
}: {
  title: string;
  contributionId: string;
  message: string;
}) {
  const refresh = useContributionRegistryStore((state) => state.refresh);
  const isRefreshing = useContributionRegistryStore((state) => state.isRefreshing);

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-gray-50 px-5 py-8 dark:bg-gray-950">
      <div className="w-full max-w-xl rounded-md border border-amber-200 bg-white px-5 py-4 dark:border-amber-900/60 dark:bg-gray-900">
        <div className="flex items-start gap-3">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-amber-100 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">{title}</h2>
            <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">{message}</p>
            <code className="mt-2 block break-all text-xs text-gray-400">{contributionId}</code>
          </div>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={isRefreshing}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-gray-500 hover:bg-gray-100 disabled:opacity-50 dark:hover:bg-gray-800"
            title="刷新贡献注册表"
          >
            <RefreshCw className={`h-4 w-4 ${isRefreshing ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>
    </div>
  );
}
