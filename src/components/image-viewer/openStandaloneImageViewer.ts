import { TauriEvent } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { trackStandaloneWindow, untrackStandaloneWindow } from '../../utils/appSession';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export interface OpenStandaloneImageViewerOptions {
  filePath: string;
  title?: string;
  projectPath?: string;
  visible?: boolean;
  focus?: boolean;
}

export async function openStandaloneImageViewer(
  options: string | OpenStandaloneImageViewerOptions,
): Promise<WebviewWindow> {
  const normalizedOptions = typeof options === 'string'
    ? { filePath: options }
    : options;

  const {
    filePath,
    title = getFileNameFromPath(normalizedOptions.filePath),
    projectPath,
    visible = true,
    focus = true,
  } = normalizedOptions;

  const label = `image-viewer-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'image-viewer',
    path: filePath,
    title,
  });

  if (projectPath) {
    searchParams.set('projectPath', projectPath);
  }

  const imageWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1180,
    height: 860,
    minWidth: 480,
    minHeight: 320,
    center: true,
    resizable: true,
    focus,
    visible,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void imageWindow.once('tauri://created', () => {
      trackStandaloneWindow({
        instanceId: label,
        type: 'image',
        filePath,
        projectPath,
        title,
      });
      void imageWindow.once(TauriEvent.WINDOW_DESTROYED, () => {
        untrackStandaloneWindow(label);
      });
      resolve(imageWindow);
    });

    void imageWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建图片查看窗口失败'));
    });
  });
}

