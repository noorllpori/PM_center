import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getFileNameFromPath } from '../workspace/fileOpeners';

export interface OpenStandaloneTextEditorOptions {
  filePath: string;
  title?: string;
  projectPath?: string;
  transferId?: string;
  visible?: boolean;
  focus?: boolean;
}

export async function openStandaloneTextEditor(
  options: string | OpenStandaloneTextEditorOptions,
): Promise<WebviewWindow> {
  const normalizedOptions = typeof options === 'string'
    ? { filePath: options }
    : options;

  const {
    filePath,
    title = getFileNameFromPath(normalizedOptions.filePath),
    projectPath,
    transferId,
    visible = true,
    focus = true,
  } = normalizedOptions;

  const label = `text-editor-${Date.now()}`;
  const searchParams = new URLSearchParams({
    view: 'text-editor',
    path: filePath,
    title,
  });

  if (transferId) {
    searchParams.set('transferId', transferId);
  }

  if (projectPath) {
    searchParams.set('projectPath', projectPath);
  }

  const textEditorWindow = new WebviewWindow(label, {
    url: `/?${searchParams.toString()}`,
    title,
    width: 1240,
    height: 860,
    minWidth: 560,
    minHeight: 380,
    center: true,
    resizable: true,
    focus,
    visible,
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
