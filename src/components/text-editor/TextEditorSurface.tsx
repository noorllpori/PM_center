import { useEffect, useEffectEvent, useMemo, useRef, useState } from 'react';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { AlertCircle, CheckCircle2, FileCode, Loader2, Save, Type } from 'lucide-react';
import { CodeEditor } from '../CodeEditor/CodeEditor';
import { detectLanguage, getLanguageName, type EditorLanguage } from '../../stores/windowStore';
import type { TextEditorTransferPayload } from './textEditorWindowTransfer';

interface TextEditorSurfaceProps {
  title: string;
  filePath?: string;
  initialContent?: string;
  initialOriginalContent?: string;
  initialLanguage?: EditorLanguage;
  showTitleInToolbar?: boolean;
  onDirtyChange?: (isDirty: boolean) => void;
  onEditorStateChange?: (snapshot: TextEditorTransferPayload) => void;
}

function resolveLanguage(
  filePath?: string,
  title?: string,
  initialLanguage?: EditorLanguage,
): EditorLanguage {
  if (initialLanguage) {
    return initialLanguage;
  }

  if (filePath) {
    return detectLanguage(filePath);
  }

  return detectLanguage(title || '');
}

export function TextEditorSurface({
  title,
  filePath,
  initialContent,
  initialOriginalContent,
  initialLanguage,
  showTitleInToolbar = true,
  onDirtyChange,
  onEditorStateChange,
}: TextEditorSurfaceProps) {
  const [content, setContent] = useState(initialContent ?? '');
  const [originalContent, setOriginalContent] = useState(initialOriginalContent ?? initialContent ?? '');
  const [language, setLanguage] = useState<EditorLanguage>(() => resolveLanguage(filePath, title, initialLanguage));
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [showLanguageMenu, setShowLanguageMenu] = useState(false);
  const [lineWrapping, setLineWrapping] = useState(true);
  const [hasResolvedInitialState, setHasResolvedInitialState] = useState(() => initialContent !== undefined || !filePath);
  const saveMessageTimerRef = useRef<number | null>(null);

  const isDirty = content !== originalContent;
  const emitDirtyChange = useEffectEvent((nextIsDirty: boolean) => {
    onDirtyChange?.(nextIsDirty);
  });
  const emitEditorStateChange = useEffectEvent((snapshot: TextEditorTransferPayload) => {
    onEditorStateChange?.(snapshot);
  });

  useEffect(() => {
    setLanguage(resolveLanguage(filePath, title, initialLanguage));
  }, [filePath, initialLanguage, title]);

  useEffect(() => {
    emitDirtyChange(isDirty);
  }, [emitDirtyChange, isDirty]);

  useEffect(() => {
    if (!filePath || !hasResolvedInitialState) {
      return;
    }

    emitEditorStateChange({
      filePath,
      title,
      content,
      originalContent,
      language,
      isDirty,
    });
  }, [
    content,
    emitEditorStateChange,
    filePath,
    hasResolvedInitialState,
    isDirty,
    language,
    originalContent,
    title,
  ]);

  useEffect(() => {
    if (saveMessageTimerRef.current) {
      window.clearTimeout(saveMessageTimerRef.current);
      saveMessageTimerRef.current = null;
    }

    return () => {
      if (saveMessageTimerRef.current) {
        window.clearTimeout(saveMessageTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    let isActive = true;

    async function loadContent() {
      if (initialContent !== undefined) {
        const nextOriginalContent = initialOriginalContent ?? initialContent;
        setContent(initialContent);
        setOriginalContent(nextOriginalContent);
        setErrorMessage(null);
        setIsLoading(false);
        setHasResolvedInitialState(true);
        return;
      }

      if (!filePath) {
        setContent('');
        setOriginalContent('');
        setErrorMessage('没有可读取的文件路径。');
        setIsLoading(false);
        setHasResolvedInitialState(true);
        return;
      }

      setIsLoading(true);
      setErrorMessage(null);
      setHasResolvedInitialState(false);

      try {
        const nextContent = await readTextFile(filePath);
        if (!isActive) {
          return;
        }

        setContent(nextContent);
        setOriginalContent(nextContent);
        setErrorMessage(null);
        setHasResolvedInitialState(true);
      } catch (error) {
        if (!isActive) {
          return;
        }

        setContent('');
        setOriginalContent('');
        setErrorMessage(`读取文本失败：${String(error)}`);
        setHasResolvedInitialState(true);
      } finally {
        if (isActive) {
          setIsLoading(false);
        }
      }
    }

    void loadContent();

    return () => {
      isActive = false;
    };
  }, [filePath, initialContent, initialOriginalContent]);

  const dismissLanguageMenu = () => {
    setShowLanguageMenu(false);
  };

  const flashSaveMessage = (message: string) => {
    setSaveMessage(message);

    if (saveMessageTimerRef.current) {
      window.clearTimeout(saveMessageTimerRef.current);
    }

    saveMessageTimerRef.current = window.setTimeout(() => {
      setSaveMessage(null);
      saveMessageTimerRef.current = null;
    }, 2400);
  };

  const handleSave = async () => {
    if (!filePath) {
      flashSaveMessage('未绑定文件路径');
      return;
    }

    try {
      await writeTextFile(filePath, content);
      setOriginalContent(content);
      setErrorMessage(null);
      flashSaveMessage('已保存');
    } catch (error) {
      setErrorMessage(`保存失败：${String(error)}`);
      flashSaveMessage('保存失败');
    }
  };

  const statusText = useMemo(() => {
    if (isLoading) {
      return '加载中...';
    }

    if (errorMessage) {
      return '读取失败';
    }

    if (saveMessage) {
      return saveMessage;
    }

    return `${content.length} 字符`;
  }, [content.length, errorMessage, isLoading, saveMessage]);

  const statusIcon = useMemo(() => {
    if (isLoading) {
      return <Loader2 className="h-3.5 w-3.5 animate-spin" />;
    }

    if (errorMessage) {
      return <AlertCircle className="h-3.5 w-3.5" />;
    }

    if (saveMessage === '已保存') {
      return <CheckCircle2 className="h-3.5 w-3.5" />;
    }

    return null;
  }, [errorMessage, isLoading, saveMessage]);

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-white dark:bg-gray-900">
      <div className="flex items-center gap-2 border-b border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/80">
        {showTitleInToolbar && (
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <FileCode className="h-4 w-4 shrink-0 text-sky-500" />
              <p className="truncate text-sm font-medium text-gray-800 dark:text-gray-100">
                {title}
                {isDirty ? ' *' : ''}
              </p>
            </div>
            {filePath && (
              <p className="mt-0.5 truncate text-xs text-gray-500 dark:text-gray-400">
                {filePath}
              </p>
            )}
          </div>
        )}

        <button
          onClick={() => setLineWrapping((value) => !value)}
          className={`flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors ${
            lineWrapping
              ? 'bg-blue-50 text-blue-600 hover:bg-blue-100 dark:bg-blue-900/30 dark:text-blue-300 dark:hover:bg-blue-900/50'
              : 'text-gray-600 hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100'
          }`}
          title={lineWrapping ? '关闭自动换行' : '开启自动换行'}
        >
          自动换行
        </button>

        <button
          onClick={handleSave}
          disabled={isLoading || !isDirty}
          className="flex items-center gap-1 rounded bg-blue-600 px-2 py-1 text-xs text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 dark:disabled:bg-gray-700"
          title="保存"
        >
          <Save className="h-3.5 w-3.5" />
          保存
        </button>

        <div className="relative">
          <button
            onClick={() => setShowLanguageMenu((value) => !value)}
            className="flex items-center gap-1 rounded px-2 py-1 text-xs text-gray-600 transition-colors hover:bg-gray-200 hover:text-gray-900 dark:text-gray-300 dark:hover:bg-gray-700 dark:hover:text-gray-100"
            title="语言"
          >
            <Type className="h-3.5 w-3.5" />
            {getLanguageName(language)}
          </button>

          {showLanguageMenu && (
            <div className="absolute left-0 top-full z-50 mt-1 w-36 rounded border border-gray-200 bg-white py-1 shadow-lg dark:border-gray-700 dark:bg-gray-800">
              {(['python', 'javascript', 'typescript', 'html', 'css', 'json', 'rust', 'markdown', 'plaintext'] as EditorLanguage[]).map((item) => (
                <button
                  key={item}
                  onClick={() => {
                    setLanguage(item);
                    dismissLanguageMenu();
                  }}
                  className={`w-full px-3 py-1.5 text-left text-xs transition-colors hover:bg-gray-100 dark:hover:bg-gray-700 ${
                    language === item ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/30 dark:text-blue-300' : 'text-gray-700 dark:text-gray-200'
                  }`}
                >
                  {getLanguageName(item)}
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="flex-1" />

        <div className={`flex min-w-[120px] items-center justify-end gap-1.5 text-xs ${
          errorMessage
            ? 'text-red-500'
            : saveMessage === '已保存'
              ? 'text-green-600'
              : 'text-gray-500 dark:text-gray-400'
        }`}>
          {statusIcon}
          <span>{statusText}</span>
        </div>
      </div>

      <div className="relative flex-1 min-h-0 overflow-hidden bg-white dark:bg-gray-950">
        {isLoading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-400">
            <div className="flex items-center gap-2">
              <Loader2 className="h-4 w-4 animate-spin" />
              正在读取文本...
            </div>
          </div>
        ) : errorMessage ? (
          <div className="flex h-full items-center justify-center p-6 text-center">
            <div className="max-w-lg">
              <AlertCircle className="mx-auto mb-3 h-10 w-10 text-red-400" />
              <p className="text-sm font-medium text-gray-800 dark:text-gray-100">无法打开这个文本文件</p>
              <p className="mt-2 break-all text-xs text-gray-500 dark:text-gray-400">{errorMessage}</p>
            </div>
          </div>
        ) : (
          <CodeEditor
            initialContent={content}
            language={language}
            theme="light"
            lineWrapping={lineWrapping}
            onChange={setContent}
            onSave={handleSave}
          />
        )}
      </div>

      {showLanguageMenu && (
        <button
          type="button"
          className="fixed inset-0 z-40 cursor-default"
          onClick={dismissLanguageMenu}
          aria-label="关闭语言菜单"
        />
      )}
    </div>
  );
}
