import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getFileNameFromPath } from '../workspace/fileOpeners';

export async function openStandaloneVideoPlayer(filePath: string): Promise<WebviewWindow> {
  const title = getFileNameFromPath(filePath);
  const label = `video-player-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'video-player',
    path: filePath,
    title,
  });

  const videoWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1280,
    height: 860,
    minWidth: 640,
    minHeight: 420,
    center: true,
    resizable: true,
    focus: true,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void videoWindow.once('tauri://created', () => {
      resolve(videoWindow);
    });

    void videoWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建视频播放窗口失败'));
    });
  });
}
