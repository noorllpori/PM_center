import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

function getFileNameFromPath(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export async function openStandaloneImageViewer(filePath: string): Promise<WebviewWindow> {
  const title = getFileNameFromPath(filePath);
  const label = `image-viewer-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'image-viewer',
    path: filePath,
    title,
  });

  const imageWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1180,
    height: 860,
    minWidth: 480,
    minHeight: 320,
    center: true,
    resizable: true,
    focus: true,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void imageWindow.once('tauri://created', () => {
      resolve(imageWindow);
    });

    void imageWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建图片查看窗口失败'));
    });
  });
}

