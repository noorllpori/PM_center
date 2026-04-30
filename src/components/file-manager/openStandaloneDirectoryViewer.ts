import { TauriEvent } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getFileNameFromPath } from '../workspace/fileOpeners';
import { trackStandaloneWindow, untrackStandaloneWindow } from '../../utils/appSession';

export interface OpenStandaloneDirectoryViewerOptions {
  directoryPath: string;
  title?: string;
  projectPath?: string;
  projectName?: string;
  visible?: boolean;
  focus?: boolean;
}

export async function openStandaloneDirectoryViewer(
  options: string | OpenStandaloneDirectoryViewerOptions,
): Promise<WebviewWindow> {
  const normalizedOptions = typeof options === 'string'
    ? { directoryPath: options }
    : options;

  const {
    directoryPath,
    title = getFileNameFromPath(normalizedOptions.directoryPath) || '目录',
    projectPath,
    projectName,
    visible = true,
    focus = true,
  } = normalizedOptions;

  const label = `directory-viewer-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'directory-viewer',
    path: directoryPath,
    title,
  });

  if (projectPath) {
    searchParams.set('projectPath', projectPath);
  }

  if (projectName) {
    searchParams.set('projectName', projectName);
  }

  const directoryWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1380,
    height: 900,
    minWidth: 780,
    minHeight: 520,
    center: true,
    resizable: true,
    focus,
    visible,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void directoryWindow.once('tauri://created', () => {
      trackStandaloneWindow({
        instanceId: label,
        type: 'directory',
        filePath: directoryPath,
        projectPath,
        title,
      });
      void directoryWindow.once(TauriEvent.WINDOW_DESTROYED, () => {
        untrackStandaloneWindow(label);
      });
      resolve(directoryWindow);
    });

    void directoryWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建目录窗口失败'));
    });
  });
}
