import {
  ChevronRight,
  Clock,
  EyeOff,
  Folder,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  Settings,
  X,
} from 'lucide-react';
import type { JsonValue } from '../../types/platform';
import { COMMAND_CONTRIBUTIONS } from '../../features/contributionRegistry';
import type { ContributedWidgetRendererProps } from '../workspace/ContributedWidget';

interface ProjectListItem {
  path: string;
  name: string;
  openedAt?: number;
  hasPmCenter?: boolean;
}

function objectValue(value: JsonValue | null) {
  return value && !Array.isArray(value) && typeof value === 'object' ? value : {};
}

function stringValue(value: JsonValue | undefined) {
  return typeof value === 'string' ? value : null;
}

function numberValue(value: JsonValue | undefined) {
  return typeof value === 'number' ? value : 0;
}

function booleanValue(value: JsonValue | undefined) {
  return value === true;
}

function projectItems(value: JsonValue | undefined): ProjectListItem[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((item) => {
    if (!item || Array.isArray(item) || typeof item !== 'object') {
      return [];
    }
    const path = stringValue(item.path);
    const name = stringValue(item.name);
    if (!path || !name) {
      return [];
    }
    return [{
      path,
      name,
      openedAt: typeof item.openedAt === 'number' ? item.openedAt : undefined,
      hasPmCenter: typeof item.hasPmCenter === 'boolean' ? item.hasPmCenter : undefined,
    }];
  });
}

function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  const diff = Date.now() - date.getTime();
  if (diff < 60 * 60 * 1000) {
    const minutes = Math.floor(diff / (60 * 1000));
    return minutes < 1 ? '刚刚' : `${minutes}分钟前`;
  }
  if (diff < 24 * 60 * 60 * 1000) {
    return `${Math.floor(diff / (60 * 60 * 1000))}小时前`;
  }
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function ProjectDirectoryWidget({ value, executeCommand }: ContributedWidgetRendererProps) {
  const data = objectValue(value);
  const projectsRootDir = stringValue(data.projectsRootDir);

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
      <h3 className="mb-3 flex items-center gap-2 font-medium text-gray-900 dark:text-gray-100">
        <Settings className="h-4 w-4" />
        项目目录
      </h3>
      {projectsRootDir ? (
        <div className="space-y-3">
          <div className="rounded-lg bg-blue-50 p-2 dark:bg-blue-900/20">
            <p className="mb-1 text-xs text-gray-500 dark:text-gray-400">当前目录</p>
            <p className="break-all text-sm text-gray-900 dark:text-gray-100">{projectsRootDir}</p>
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.selectProjectRoot.id)}
              className="flex-1 rounded-lg bg-gray-100 px-3 py-1.5 text-xs text-gray-700 transition-colors hover:bg-gray-200 dark:bg-gray-700 dark:text-gray-300 dark:hover:bg-gray-600"
            >
              更换
            </button>
            <button
              type="button"
              onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.clearProjectRoot.id)}
              className="rounded-lg px-3 py-1.5 text-xs text-red-600 transition-colors hover:bg-red-50 dark:hover:bg-red-900/20"
            >
              清除
            </button>
          </div>
        </div>
      ) : (
        <div className="py-4 text-center">
          <p className="mb-3 text-sm text-gray-500 dark:text-gray-400">设置项目根目录以管理多个项目</p>
          <button
            type="button"
            onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.selectProjectRoot.id)}
            className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white transition-colors hover:bg-blue-700"
          >
            选择目录
          </button>
        </div>
      )}
    </section>
  );
}

export function ProjectQuickActionsWidget({ value, executeCommand }: ContributedWidgetRendererProps) {
  const data = objectValue(value);
  const hasProjectsRoot = booleanValue(data.hasProjectsRoot);
  const ignoredProjectCount = numberValue(data.ignoredProjectCount);

  return (
    <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800">
      <h3 className="mb-3 font-medium text-gray-900 dark:text-gray-100">快速操作</h3>
      {!hasProjectsRoot && (
        <p className="mb-3 text-xs leading-5 text-gray-500 dark:text-gray-400">
          不设置项目根目录时，也可以直接手动导入并打开单个项目。
        </p>
      )}
      <div className="space-y-2">
        {hasProjectsRoot && (
          <button
            type="button"
            onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.createProject.id)}
            className="flex w-full items-center gap-2 rounded-lg bg-blue-50 px-3 py-2 text-sm text-blue-600 transition-colors hover:bg-blue-100 dark:bg-blue-900/20 dark:text-blue-400 dark:hover:bg-blue-900/30"
          >
            <FolderPlus className="h-4 w-4" />
            创建新项目
          </button>
        )}
        <button
          type="button"
          onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.importProject.id)}
          className="flex w-full items-center gap-2 rounded-lg bg-gray-50 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:bg-gray-700/50 dark:text-gray-300 dark:hover:bg-gray-700"
        >
          <FolderOpen className="h-4 w-4" />
          手动导入单个项目
        </button>
        {ignoredProjectCount > 0 && (
          <button
            type="button"
            onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.showIgnoredProjects.id)}
            className="flex w-full items-center gap-2 rounded-lg bg-orange-50 px-3 py-2 text-sm text-orange-600 transition-colors hover:bg-orange-100 dark:bg-orange-900/10 dark:text-orange-400 dark:hover:bg-orange-900/20"
          >
            <EyeOff className="h-4 w-4" />
            已忽略项目 ({ignoredProjectCount})
          </button>
        )}
      </div>
    </section>
  );
}

export function RecentProjectsWidget({ value, executeCommand }: ContributedWidgetRendererProps) {
  const data = objectValue(value);
  const settingsLoaded = booleanValue(data.settingsLoaded);
  const projects = projectItems(data.projects);

  if (!settingsLoaded) {
    return <LoadingState label="加载项目列表..." />;
  }
  if (projects.length === 0) {
    return (
      <div className="py-12 text-center text-gray-400">
        <Clock className="mx-auto mb-3 h-12 w-12 opacity-50" />
        <p className="text-sm">暂无最近打开的项目</p>
      </div>
    );
  }
  return (
    <div className="space-y-2">
      {projects.map((project) => (
        <div
          key={project.path}
          role="button"
          tabIndex={0}
          onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.openProject.id, { path: project.path })}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              void executeCommand(COMMAND_CONTRIBUTIONS.openProject.id, { path: project.path });
            }
          }}
          className="group flex cursor-pointer items-center gap-3 rounded-lg border border-gray-200 bg-white p-3 transition-all hover:border-blue-300 hover:shadow-sm dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-700"
        >
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-blue-50 dark:bg-blue-900/30">
            <Folder className="h-5 w-5 text-blue-500" />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{project.name}</p>
            <p className="truncate text-xs text-gray-500 dark:text-gray-400">{project.path}</p>
          </div>
          <div className="flex items-center gap-2">
            {project.openedAt !== undefined && (
              <span className="whitespace-nowrap text-xs text-gray-400">{formatTime(project.openedAt)}</span>
            )}
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                void executeCommand(COMMAND_CONTRIBUTIONS.removeRecentProject.id, { path: project.path });
              }}
              className="p-1 text-gray-400 opacity-0 transition-opacity hover:text-red-500 group-hover:opacity-100"
              title="从最近项目中移除"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function ProjectCatalogWidget({ value, executeCommand }: ContributedWidgetRendererProps) {
  const data = objectValue(value);
  const settingsLoaded = booleanValue(data.settingsLoaded);
  const isScanning = booleanValue(data.isScanning);
  const projects = projectItems(data.projects);

  if (!settingsLoaded) {
    return <LoadingState label="加载项目列表..." />;
  }
  if (isScanning) {
    return <LoadingState label="扫描中..." />;
  }
  if (projects.length === 0) {
    return (
      <div className="py-12 text-center text-gray-400">
        <Folder className="mx-auto mb-3 h-12 w-12 opacity-50" />
        <p className="text-sm">该目录下暂无项目</p>
        <p className="mt-1 text-xs opacity-70">点击“创建新项目”或“手动导入单个项目”</p>
      </div>
    );
  }
  return (
    <div className="min-h-[100px] space-y-2">
      {projects.map((project) => {
        const hasPmCenter = project.hasPmCenter !== false;
        return (
          <div
            key={project.path}
            role="button"
            tabIndex={0}
            onClick={() => executeCommand(COMMAND_CONTRIBUTIONS.openProject.id, { path: project.path })}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                void executeCommand(COMMAND_CONTRIBUTIONS.openProject.id, { path: project.path });
              }
            }}
            className={`group flex cursor-pointer items-center gap-3 rounded-lg border p-3 transition-all hover:shadow-sm ${
              hasPmCenter
                ? 'border-gray-200 bg-white hover:border-blue-300 dark:border-gray-700 dark:bg-gray-800 dark:hover:border-blue-700'
                : 'border-gray-200 bg-gray-50 hover:border-yellow-300 dark:border-gray-700 dark:bg-gray-800/50 dark:hover:border-yellow-700'
            }`}
          >
            <div className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-lg ${
              hasPmCenter ? 'bg-blue-50 dark:bg-blue-900/30' : 'bg-yellow-50 dark:bg-yellow-900/20'
            }`}>
              <Folder className={`h-5 w-5 ${hasPmCenter ? 'text-blue-500' : 'text-yellow-600'}`} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{project.name}</p>
                {!hasPmCenter && (
                  <span className="rounded bg-yellow-100 px-1.5 py-0.5 text-xs text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400">
                    未初始化
                  </span>
                )}
              </div>
              <p className="truncate text-xs text-gray-500 dark:text-gray-400">{project.path}</p>
            </div>
            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={(event) => {
                  event.stopPropagation();
                  void executeCommand(COMMAND_CONTRIBUTIONS.ignoreProject.id, {
                    path: project.path,
                    name: project.name,
                  });
                }}
                className="rounded-lg p-1.5 text-gray-400 opacity-0 transition-all hover:bg-orange-50 hover:text-orange-500 group-hover:opacity-100 dark:hover:bg-orange-900/20"
                title="忽略此项目"
              >
                <EyeOff className="h-4 w-4" />
              </button>
              <ChevronRight className={`h-4 w-4 ${hasPmCenter ? 'text-gray-400' : 'text-yellow-400'}`} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center py-12 text-gray-400">
      <RefreshCw className="mr-2 h-5 w-5 animate-spin" />
      {label}
    </div>
  );
}
