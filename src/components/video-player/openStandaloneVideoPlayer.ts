import { TauriEvent } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getFileNameFromPath } from '../workspace/fileOpeners';
import { trackStandaloneWindow, untrackStandaloneWindow } from '../../utils/appSession';

export interface OpenStandaloneVideoPlayerOptions {
  filePath: string;
  title?: string;
  projectPath?: string;
  visible?: boolean;
  focus?: boolean;
}

export async function openStandaloneVideoPlayer(
  options: string | OpenStandaloneVideoPlayerOptions,
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

  const label = `video-player-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'video-player',
    path: filePath,
    title,
  });

  if (projectPath) {
    searchParams.set('projectPath', projectPath);
  }

  const videoWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1280,
    height: 860,
    minWidth: 640,
    minHeight: 420,
    center: true,
    resizable: true,
    focus,
    visible,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void videoWindow.once('tauri://created', () => {
      trackStandaloneWindow({
        instanceId: label,
        type: 'video',
        filePath,
        projectPath,
        title,
      });
      void videoWindow.once(TauriEvent.WINDOW_DESTROYED, () => {
        untrackStandaloneWindow(label);
      });
      resolve(videoWindow);
    });

    void videoWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建视频播放窗口失败'));
    });
  });
}
