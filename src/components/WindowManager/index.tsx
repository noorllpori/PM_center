import { useWindowStore, type WindowContentType } from '../../stores/windowStore';
import { WindowFrame, WindowTaskbar } from './WindowFrame';
import { CodeEditorWindow, createCodeEditorWindow } from './CodeEditorWindow';
import { ImageViewerWindow, createImageViewerWindow } from './ImageViewerWindow';
import { isImageExtension } from '../image-viewer/imageViewerUtils';

// 窗口内容组件映射
const windowContentComponents: Record<WindowContentType, React.FC<{ windowInstance: import('../../stores/windowStore').WindowInstance }>> = {
  'code-editor': CodeEditorWindow,
  'image-viewer': ImageViewerWindow,
  'markdown-preview': CodeEditorWindow, // 临时使用代码编辑器
  'terminal': CodeEditorWindow, // TODO: 实现终端窗口
  'settings': CodeEditorWindow, // TODO: 实现设置窗口
  'custom': CodeEditorWindow, // 自定义内容
};

// 窗口管理器主组件
export function WindowManager() {
  const { windows, showTaskbar } = useWindowStore();

  return (
    <>
      {/* 渲染所有窗口 */}
      {windows.map((windowInstance) => {
        const ContentComponent = windowContentComponents[windowInstance.contentType];
        
        if (!ContentComponent) {
          // 未知类型显示默认窗口
          return (
            <WindowFrame key={windowInstance.id} windowInstance={windowInstance}>
              <div className="flex items-center justify-center h-full text-gray-400">
                <p>未知窗口类型: {windowInstance.contentType}</p>
              </div>
            </WindowFrame>
          );
        }

        return <ContentComponent key={windowInstance.id} windowInstance={windowInstance} />;
      })}

      {/* 任务栏 */}
      {showTaskbar && <WindowTaskbar />}
    </>
  );
}

// 重新导出
export { WindowFrame, WindowTaskbar } from './WindowFrame';
export { CodeEditorWindow, createCodeEditorWindow } from './CodeEditorWindow';
export { ImageViewerWindow, createImageViewerWindow } from './ImageViewerWindow';
export { useWindowStore } from '../../stores/windowStore';
export type { WindowInstance, CreateWindowOptions, WindowContentType } from '../../stores/windowStore';

// 辅助函数：打开文件（根据类型自动选择窗口类型）
export function openFile(
  createWindow: (options: import('../../stores/windowStore').CreateWindowOptions) => string,
  filePath: string,
  content?: string
): string {
  const ext = filePath.split('.').pop()?.toLowerCase();
  const fileName = filePath.split('/').pop() || filePath.split('\\').pop() || 'Untitled';

  // 图片文件
  if (isImageExtension(ext)) {
    return createImageViewerWindow(createWindow, {
      title: fileName,
      imageUrl: filePath, // 需要转换为实际可访问的 URL
    });
  }

  // 代码/文本文件
  return createCodeEditorWindow(createWindow, {
    title: fileName,
    content,
    filePath,
  });
}

// 窗口管理器快捷操作
export function useWindowActions() {
  const store = useWindowStore();

  return {
    // 创建窗口
    newCodeEditor: (options?: Parameters<typeof createCodeEditorWindow>[1]) => 
      createCodeEditorWindow(store.createWindow, options || {}),
    
    newImageViewer: (options: Parameters<typeof createImageViewerWindow>[1]) => 
      createImageViewerWindow(store.createWindow, options),
    
    // 布局
    cascade: store.cascadeWindows,
    tile: store.tileWindows,
    stack: store.stackWindows,
    
    // 关闭
    closeAll: store.closeAllWindows,
    closeOthers: store.closeOtherWindows,
  };
}
