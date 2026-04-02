import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getFileNameFromPath } from '../workspace/fileOpeners';

export async function openStandaloneTextEditor(filePath: string): Promise<WebviewWindow> {
  const title = getFileNameFromPath(filePath);
  const label = `text-editor-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'text-editor',
    path: filePath,
    title,
  });

  const textEditorWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1240,
    height: 860,
    minWidth: 560,
    minHeight: 380,
    center: true,
    resizable: true,
    focus: true,
  });

  return await new Promise<WebviewWindow>((resolve, reject) => {
    void textEditorWindow.once('tauri://created', () => {
      resolve(textEditorWindow);
    });

    void textEditorWindow.once('tauri://error', (event) => {
      reject(event.payload ?? new Error('创建文本编辑窗口失败'));
    });
  });
}
