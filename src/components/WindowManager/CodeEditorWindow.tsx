import { useState } from 'react';
import { WindowFrame } from './WindowFrame';
import { CodeEditor, LanguageSelector } from '../CodeEditor/CodeEditor';
import { useWindowStore, type WindowInstance, detectLanguage, getLanguageName, type EditorLanguage } from '../../stores/windowStore';
import { Save, FileCode, Type } from 'lucide-react';

interface CodeEditorWindowProps {
  windowInstance: WindowInstance;
}

export function CodeEditorWindow({ windowInstance }: CodeEditorWindowProps) {
  const { updateWindowData, updateWindowTitle, getWindowById } = useWindowStore();
  const [showLanguageMenu, setShowLanguageMenu] = useState(false);
  
  const { id, data, title } = windowInstance;
  const content = (data.content as string) || '';
  const language = (data.language as EditorLanguage) || 'plaintext';
  const filePath = data.filePath as string | undefined;
  const isDirty = (data.isDirty as boolean) || false;

  const handleContentChange = (newContent: string) => {
    const wasDirty = isDirty;
    updateWindowData(id, { 
      content: newContent, 
      isDirty: true,
      originalContent: data.originalContent || content 
    });
    
    // 更新标题显示修改状态
    if (!wasDirty && !title.endsWith(' *')) {
      updateWindowTitle(id, `${title} *`);
    }
  };

  const handleLanguageChange = (newLanguage: EditorLanguage) => {
    updateWindowData(id, { language: newLanguage, isDirty: true });
  };

  const handleSave = async () => {
    if (filePath) {
      // TODO: 调用后端保存
      console.log('Saving to:', filePath);
      updateWindowData(id, { isDirty: false });
      
      // 移除标题的修改标记
      if (title.endsWith(' *')) {
        updateWindowTitle(id, title.slice(0, -2));
      }
    } else {
      // TODO: 打开保存对话框
      console.log('Open save dialog');
    }
  };

  // 工具栏内容
  const toolbarContent = (
    <>
      <button
        onClick={handleSave}
        disabled={!isDirty}
        className="flex items-center gap-1 px-2 py-1 text-xs bg-blue-600 hover:bg-blue-700 disabled:bg-gray-300 text-white rounded transition-colors"
      >
        <Save className="w-3 h-3" />
        保存
      </button>

      <div className="w-px h-4 bg-gray-300 dark:bg-gray-600" />

      <div className="relative">
        <button
          onClick={() => setShowLanguageMenu(!showLanguageMenu)}
          className="flex items-center gap-1 px-2 py-1 text-xs bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded transition-colors"
        >
          <Type className="w-3 h-3" />
          {getLanguageName(language)}
        </button>
        
        {showLanguageMenu && (
          <div className="absolute top-full left-0 mt-1 w-32 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded shadow-lg z-50">
            {(['python', 'javascript', 'typescript', 'html', 'css', 'json', 'rust', 'markdown', 'plaintext'] as EditorLanguage[]).map((lang) => (
              <button
                key={lang}
                onClick={() => {
                  handleLanguageChange(lang);
                  setShowLanguageMenu(false);
                }}
                className={`w-full text-left px-3 py-1.5 text-xs hover:bg-gray-100 dark:hover:bg-gray-700 ${
                  language === lang ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-600' : ''
                }`}
              >
                {getLanguageName(lang)}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="flex-1" />
      
      <span className="text-xs text-gray-400">
        {content.length} 字符
      </span>
    </>
  );

  // 自定义头部内容
  const headerContent = (
    <div className="flex items-center gap-2">
      <FileCode className="w-4 h-4 text-blue-500" />
      <span className={`text-sm font-medium truncate max-w-[200px] ${isDirty ? 'italic' : ''}`}>
        {title}
      </span>
      <span className="text-xs text-gray-400">
        ({getLanguageName(language)})
      </span>
    </div>
  );

  return (
    <WindowFrame
      windowInstance={windowInstance}
      headerContent={headerContent}
      toolbarContent={toolbarContent}
    >
      <CodeEditor
        initialContent={content}
        language={language}
        theme="dark"
        onChange={handleContentChange}
        onSave={handleSave}
      />
      
      {/* 点击外部关闭语言菜单 */}
      {showLanguageMenu && (
        <div
          className="fixed inset-0 z-40"
          onClick={() => setShowLanguageMenu(false)}
        />
      )}
    </WindowFrame>
  );
}

// 创建代码编辑器窗口的辅助函数
export function createCodeEditorWindow(
  createWindow: (options: import('../../stores/windowStore').CreateWindowOptions) => string,
  options: {
    title?: string;
    content?: string;
    filePath?: string;
    language?: EditorLanguage;
  } = {}
) {
  const { title, content, filePath, language } = options;
  
  // 从文件路径或标题检测语言
  const detectedLang = language || (filePath 
    ? detectLanguage(filePath) 
    : title 
      ? detectLanguage(title)
      : 'plaintext'
  );
  
  // 从文件路径获取文件名
  const fileName = filePath 
    ? filePath.split('/').pop() || filePath.split('\\').pop() || 'Untitled'
    : title || 'Untitled';

  return createWindow({
    title: fileName,
    contentType: 'code-editor',
    data: {
      content: content || '',
      originalContent: content || '',
      language: detectedLang,
      filePath,
      isDirty: false,
    },
  });
}
