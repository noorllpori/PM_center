import {
  AlertTriangle,
  LayoutDashboard,
  Loader2,
  ShieldCheck,
  Wrench,
} from 'lucide-react';
import nexoraLogo from '../../assets/nexora-logo.png';
import {
  APP_AUTHOR_CONTACT,
  APP_AUTHOR_NAME,
  APP_NAME,
  APP_VERSION_TEXT,
} from '../../config/appMeta';
import { OPEN_BUILTIN_TOOLS_CENTER_EVENT } from '../../features/builtinTools';
import type { FallbackProfileHome } from '../../features/profileHome';

export function MinimalSafeHome({
  resolution,
  runtimeError,
  onOpenRecovery,
}: {
  resolution: FallbackProfileHome;
  runtimeError?: string | null;
  onOpenRecovery: () => void;
}) {
  const isLoading = resolution.code === 'PROFILE_LOADING' && !runtimeError;
  const profile = resolution.profile;
  const message = runtimeError
    ? `装配方案读取失败：${runtimeError}`
    : resolution.message;

  return (
    <div className="h-full overflow-y-auto bg-gray-50 px-5 py-8 dark:bg-gray-900 sm:px-8">
      <div className="mx-auto flex min-h-full max-w-3xl flex-col justify-center py-6">
        <header className="flex items-center gap-4">
          <img src={nexoraLogo} alt="" className="h-12 w-12 object-contain" />
          <div className="min-w-0">
            <p className="text-xs font-medium text-gray-500 dark:text-gray-400">{APP_NAME} · {APP_VERSION_TEXT}</p>
            <h1 className="mt-1 text-xl font-semibold text-gray-900 dark:text-gray-100">最小安全主页</h1>
          </div>
        </header>

        <div className="mt-7 border-y border-gray-200 py-5 dark:border-gray-800">
          <div className="flex items-start gap-3">
            <div className={`mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md ${
              isLoading
                ? 'bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300'
                : 'bg-amber-50 text-amber-600 dark:bg-amber-950/40 dark:text-amber-300'
            }`}>
              {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <AlertTriangle className="h-4 w-4" />}
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
                {isLoading ? '正在准备当前装配方案' : '已回退到安全主页'}
              </h2>
              <p className="mt-1 whitespace-pre-wrap break-words text-sm leading-6 text-gray-600 dark:text-gray-300">
                {message}
              </p>
              {!isLoading ? (
                <p className="mt-2 text-xs leading-5 text-gray-500 dark:text-gray-400">
                  这里不会加载项目扫描或业务页面，你仍可打开维护中心修正装配方案。
                </p>
              ) : null}
            </div>
          </div>
        </div>

        <div className="mt-5 grid gap-3 sm:grid-cols-3">
          <div className="min-w-0">
            <p className="text-xs text-gray-500 dark:text-gray-400">当前方案</p>
            <p className="mt-1 truncate text-sm font-medium text-gray-900 dark:text-gray-100" title={profile?.name || '读取中'}>
              {profile?.name || '读取中'}
            </p>
          </div>
          <div>
            <p className="text-xs text-gray-500 dark:text-gray-400">启用组件</p>
            <p className="mt-1 text-sm font-medium text-gray-900 dark:text-gray-100">
              {profile?.enabledModules?.length ?? 0}
            </p>
          </div>
          <div className="min-w-0">
            <p className="text-xs text-gray-500 dark:text-gray-400">主页目标</p>
            <p className="mt-1 truncate font-mono text-xs text-gray-700 dark:text-gray-200" title={resolution.requestedSurfaceId || '未配置'}>
              {resolution.requestedSurfaceId || '未配置'}
            </p>
          </div>
        </div>

        <div className="mt-7 flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => window.dispatchEvent(new Event(OPEN_BUILTIN_TOOLS_CENTER_EVENT))}
            className="inline-flex h-9 items-center gap-2 rounded-md bg-blue-600 px-3.5 text-sm font-medium text-white transition-colors hover:bg-blue-700"
          >
            <Wrench className="h-4 w-4" />
            打开功能中心
          </button>
          <button
            type="button"
            onClick={onOpenRecovery}
            className="inline-flex h-9 items-center gap-2 rounded-md border border-gray-300 bg-white px-3.5 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200 dark:hover:bg-gray-800"
          >
            <ShieldCheck className="h-4 w-4" />
            维护中心
          </button>
        </div>

        <footer className="mt-8 flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-gray-400">
          <span className="inline-flex items-center gap-1.5"><ShieldCheck className="h-3.5 w-3.5" />维护入口保持可用</span>
          <span className="inline-flex items-center gap-1.5"><LayoutDashboard className="h-3.5 w-3.5" />错误主页不会产生空白界面</span>
          <span>{APP_AUTHOR_NAME} · {APP_AUTHOR_CONTACT}</span>
        </footer>
      </div>
    </div>
  );
}
