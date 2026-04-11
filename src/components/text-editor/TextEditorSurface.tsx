import { lazy, Suspense, useEffect, useEffectEvent, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  AlertCircle,
  CheckCircle2,
  Code2,
  Eye,
  FileCode,
  FolderPlus,
  Loader2,
  Settings2,
  Save,
  TriangleAlert,
  Type,
} from 'lucide-react';
import { CodeEditor } from '../CodeEditor/CodeEditor';
import type { FileInfo } from '../../types';
import { detectLanguage, getLanguageName, type EditorLanguage } from '../../stores/windowStore';
import { buildMdtInlineReference, ensureMdtContent, isMdtPath } from '../../utils/mdt';
import {
  MdtRelatedFilePickerDialog,
  type MdtRelatedFilePickResult,
} from './MdtRelatedFilePickerDialog';
import type { MarkdownEditorInsertRequest } from './MarkdownEditorSurface';
import type { MarkdownViewMode, TextEditorTransferPayload } from './textEditorWindowTransfer';

const MarkdownEditorSurface = lazy(async () => {
  const module = await import('./MarkdownEditorSurface');
  return { default: module.MarkdownEditorSurface };
});

interface TextEditorSurfaceProps {
  title: string;
  filePath?: string;
  projectPath?: string;
  initialContent?: string;
  initialOriginalContent?: string;
  initialLanguage?: EditorLanguage;
  initialMarkdownViewMode?: MarkdownViewMode;
  isActive?: boolean;
  showTitleInToolbar?: boolean;
  onDirtyChange?: (isDirty: boolean) => void;
  onEditorStateChange?: (snapshot: TextEditorTransferPayload) => void;
}

const SAVE_SUCCESS_MESSAGE = '已保存';
const SAVE_FAILURE_MESSAGE = '保存失败';
const MARKDOWN_SOURCE_WARNING =
  '当前文档包含暂不支持可视化编辑的 Markdown/MDX 结构，已切换到源码模式。';
const MARKDOWN_RICH_TEXT_WARNING =
  '检测到部分 Markdown/MDX 语法需要在源码模式下编辑。';

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

function resolveMarkdownViewMode(initialMarkdownViewMode?: MarkdownViewMode): MarkdownViewMode {
  return initialMarkdownViewMode ?? 'rich-text';
}

export function TextEditorSurface({
  title,
  filePath,
  projectPath,
  initialContent,
  initialOriginalContent,
  initialLanguage,
  initialMarkdownViewMode,
  isActive = true,
  showTitleInToolbar = true,
  onDirtyChange,
  onEditorStateChange,
}: TextEditorSurfaceProps) {
  const [content, setContent] = useState(initialContent ?? '');
  const [originalContent, setOriginalContent] = useState(
    initialOriginalContent ?? initialContent ?? '',
  );
  const [language, setLanguage] = useState<EditorLanguage>(() =>
    resolveLanguage(filePath, title, initialLanguage),
  );
  const [markdownViewMode, setMarkdownViewMode] = useState<MarkdownViewMode>(() =>
    resolveMarkdownViewMode(initialMarkdownViewMode),
  );
  const [isLoading, setIsLoading] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [markdownErrorMessage, setMarkdownErrorMessage] = useState<string | null>(null);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [mdtInfoMessage, setMdtInfoMessage] = useState<string | null>(null);
  const [showLanguageMenu, setShowLanguageMenu] = useState(false);
  const [isMdtFilePickerOpen, setIsMdtFilePickerOpen] = useState(false);
  const [frontmatterEditRequestKey, setFrontmatterEditRequestKey] = useState(0);
  const [insertRequest, setInsertRequest] = useState<MarkdownEditorInsertRequest | null>(null);
  const [lineWrapping, setLineWrapping] = useState(true);
  const [hasResolvedInitialState, setHasResolvedInitialState] = useState(
    () => initialContent !== undefined || !filePath,
  );
  const saveMessageTimerRef = useRef<number | null>(null);
  const insertRequestCounterRef = useRef(0);

  const isMarkdownDocument = language === 'markdown';
  const isMdtDocument = isMdtPath(filePath || title);
  const isDirty = content !== originalContent;
  const markdownEditorId = useMemo(
    () => `markdown-editor:${filePath || title}`,
    [filePath, title],
  );

  const emitDirtyChange = useEffectEvent((nextIsDirty: boolean) => {
    onDirtyChange?.(nextIsDirty);
  });

  const emitEditorStateChange = useEffectEvent((snapshot: TextEditorTransferPayload) => {
    onEditorStateChange?.(snapshot);
  });

  const handleMarkdownChange = useEffectEvent(
    (nextContent: string, initialNormalize: boolean) => {
      setMarkdownErrorMessage(null);

      if (initialNormalize && !isDirty) {
        setContent(nextContent);
        setOriginalContent(nextContent);
        return;
      }

      setContent(nextContent);
    },
  );

  const handleMarkdownErrorChange = useEffectEvent((nextMessage: string | null) => {
    setMarkdownErrorMessage(nextMessage);

    if (nextMessage && markdownViewMode === 'rich-text') {
      setMarkdownViewMode('source');
    }
  });

  useEffect(() => {
    setLanguage(resolveLanguage(filePath, title, initialLanguage));
  }, [filePath, initialLanguage, title]);

  useEffect(() => {
    if (language === 'markdown') {
      setMarkdownViewMode(resolveMarkdownViewMode(initialMarkdownViewMode));
      setShowLanguageMenu(false);
      return;
    }

    setMarkdownErrorMessage(null);
    setMdtInfoMessage(null);
  }, [filePath, initialMarkdownViewMode, language]);

  useEffect(() => {
    emitDirtyChange(isDirty);
  }, [isDirty]);

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
      markdownViewMode: isMarkdownDocument ? markdownViewMode : undefined,
    });
  }, [
    content,
    filePath,
    hasResolvedInitialState,
    isDirty,
    isMarkdownDocument,
    language,
    markdownViewMode,
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
        setMarkdownErrorMessage(null);
        setMdtInfoMessage(null);
        setIsLoading(false);
        setHasResolvedInitialState(true);
        return;
      }

      if (!filePath) {
        setContent('');
        setOriginalContent('');
        setErrorMessage('没有可读取的文件路径。');
        setMarkdownErrorMessage(null);
        setMdtInfoMessage(null);
        setIsLoading(false);
        setHasResolvedInitialState(true);
        return;
      }

      setIsLoading(true);
      setErrorMessage(null);
      setMarkdownErrorMessage(null);
      setMdtInfoMessage(null);
      setHasResolvedInitialState(false);

      try {
        const [rawContent, fileInfo] = await Promise.all([
          readTextFile(filePath),
          isMdtDocument
            ? invoke<FileInfo>('get_file_info', { path: filePath }).catch(() => null)
            : Promise.resolve(null),
        ]);
        if (!isActive) {
          return;
        }

        if (isMdtDocument) {
          const ensuredContent = ensureMdtContent(rawContent, {
            filePath,
            defaultCreatedAt: fileInfo?.created || new Date().toISOString(),
          });
          setContent(ensuredContent.content);
          setOriginalContent(ensuredContent.changed ? rawContent : ensuredContent.content);
          setMdtInfoMessage(
            ensuredContent.changed
              ? 'MDT 元数据已补全，请保存后写入文件。'
              : null,
          );
        } else {
          setContent(rawContent);
          setOriginalContent(rawContent);
        }
        setErrorMessage(null);
        setMarkdownErrorMessage(null);
        setHasResolvedInitialState(true);
      } catch (error) {
        if (!isActive) {
          return;
        }

        setContent('');
        setOriginalContent('');
        setErrorMessage(`读取文本失败：${String(error)}`);
        setMarkdownErrorMessage(null);
        setMdtInfoMessage(null);
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
  }, [filePath, initialContent, initialOriginalContent, isMdtDocument]);

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
    if (isLoading || !hasResolvedInitialState) {
      return;
    }

    if (!filePath) {
      flashSaveMessage('未绑定文件路径');
      return;
    }

    try {
      await writeTextFile(filePath, content);
      setOriginalContent(content);
      setErrorMessage(null);
      setMdtInfoMessage(null);
      flashSaveMessage(SAVE_SUCCESS_MESSAGE);
    } catch (error) {
      setErrorMessage(`保存失败：${String(error)}`);
      flashSaveMessage(SAVE_FAILURE_MESSAGE);
    }
  };

  const handleOpenMdtFrontmatter = () => {
    setFrontmatterEditRequestKey((value) => value + 1);
  };

  const handleAddMdtReference = ({ targetPath, mode }: MdtRelatedFilePickResult) => {
    if (!filePath || !projectPath) {
      flashSaveMessage('当前文档缺少项目上下文，暂时无法插入文件引用');
      return;
    }

    const nextKey = insertRequestCounterRef.current + 1;
    insertRequestCounterRef.current = nextKey;
    const inlineReference = buildMdtInlineReference(
      filePath,
      targetPath,
      mode === 'image' ? 'image' : 'link',
    );

    setInsertRequest(
      mode === 'image'
        ? {
          key: nextKey,
          type: 'image',
          image: {
            src: inlineReference.reference,
            altText: inlineReference.label,
            title: inlineReference.label,
          },
        }
        : {
          key: nextKey,
          type: 'markdown',
          markdown: inlineReference.markdown,
        },
    );
    setIsMdtFilePickerOpen(false);
  };

  useEffect(() => {
    if (!isActive) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }

      if (!(event.ctrlKey || event.metaKey) || event.shiftKey || event.altKey) {
        return;
      }

      if (event.key.toLowerCase() !== 's') {
        return;
      }

      event.preventDefault();
      void handleSave();
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleSave, isActive]);

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

    if (mdtInfoMessage) {
      return mdtInfoMessage;
    }

    return `${content.length} 字符`;
  }, [content.length, errorMessage, isLoading, mdtInfoMessage, saveMessage]);

  const statusIcon = useMemo(() => {
    if (isLoading) {
      return <Loader2 className="h-3.5 w-3.5 animate-spin" />;
    }

    if (errorMessage) {
      return <AlertCircle className="h-3.5 w-3.5" />;
    }

    if (saveMessage === SAVE_SUCCESS_MESSAGE) {
      return <CheckCircle2 className="h-3.5 w-3.5" />;
    }

    return null;
  }, [errorMessage, isLoading, saveMessage]);

  const markdownWarningText = useMemo(() => {
    if (!markdownErrorMessage) {
      return null;
    }

    return markdownViewMode === 'source'
      ? MARKDOWN_SOURCE_WARNING
      : MARKDOWN_RICH_TEXT_WARNING;
  }, [markdownErrorMessage, markdownViewMode]);

  return (
    <div className="flex h-full w-full min-w-0 flex-col bg-white dark:bg-gray-900">
      <div className="flex items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/80">
        {showTitleInToolbar ? (
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
        ) : (
          <div className="flex-1" />
        )}

        <div className="flex items-center gap-2">
          {isMdtDocument && (
            <>
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={handleOpenMdtFrontmatter}
                className="inline-flex items-center gap-1 rounded border border-gray-200 bg-white px-2 py-1 text-xs text-gray-700 shadow-sm transition-colors hover:bg-gray-100"
                title="编辑当前 MDT 的标题、时间、摘要等元数据"
              >
                <Settings2 className="h-3.5 w-3.5" />
                编辑 MDT 元数据
              </button>
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setIsMdtFilePickerOpen(true)}
                disabled={!projectPath || !filePath}
                className="inline-flex items-center gap-1 rounded border border-gray-200 bg-white px-2 py-1 text-xs text-gray-700 shadow-sm transition-colors hover:bg-gray-100 disabled:cursor-not-allowed disabled:bg-gray-100 disabled:text-gray-400"
                title={projectPath ? '从当前项目中选择文件或文件夹并加入 MDT' : '当前窗口没有可用的项目上下文'}
              >
                <FolderPlus className="h-3.5 w-3.5" />
                添加文件
              </button>
            </>
          )}

          {isMarkdownDocument ? (
            <div className="flex items-center rounded-md border border-gray-200 bg-white p-0.5 shadow-sm">
              <button
                type="button"
                onClick={() => setMarkdownViewMode('rich-text')}
                className={`inline-flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors ${
                  markdownViewMode === 'rich-text'
                    ? 'bg-blue-600 text-white'
                    : 'text-gray-600 hover:bg-gray-100 hover:text-gray-900'
                }`}
                title="切换到可视化编辑"
              >
                <Eye className="h-3.5 w-3.5" />
                可视化
              </button>
              <button
                type="button"
                onClick={() => setMarkdownViewMode('source')}
                className={`inline-flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors ${
                  markdownViewMode === 'source'
                    ? 'bg-blue-600 text-white'
                    : 'text-gray-600 hover:bg-gray-100 hover:text-gray-900'
                }`}
                title="切换到源码编辑"
              >
                <Code2 className="h-3.5 w-3.5" />
                源码
              </button>
            </div>
          ) : (
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
          )}

          <button
            onClick={handleSave}
            disabled={isLoading || !isDirty}
            className="flex items-center gap-1 rounded bg-blue-600 px-2 py-1 text-xs text-white transition-colors hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 dark:disabled:bg-gray-700"
            title="保存"
          >
            <Save className="h-3.5 w-3.5" />
            保存
          </button>

          {!isMarkdownDocument && (
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
                  {(
                    [
                      'python',
                      'javascript',
                      'typescript',
                      'html',
                      'css',
                      'json',
                      'rust',
                      'markdown',
                      'plaintext',
                    ] as EditorLanguage[]
                  ).map((item) => (
                    <button
                      key={item}
                      onClick={() => {
                        setLanguage(item);
                        dismissLanguageMenu();
                      }}
                      className={`w-full px-3 py-1.5 text-left text-xs transition-colors hover:bg-gray-100 dark:hover:bg-gray-700 ${
                        language === item
                          ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/30 dark:text-blue-300'
                          : 'text-gray-700 dark:text-gray-200'
                      }`}
                    >
                      {getLanguageName(item)}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {isMarkdownDocument && markdownWarningText && (
          <div
            className="hidden max-w-[320px] items-center gap-1 rounded-md border border-amber-200 bg-amber-50 px-2 py-1 text-xs text-amber-700 lg:flex"
            title={markdownErrorMessage ?? undefined}
          >
            <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{markdownWarningText}</span>
          </div>
        )}

        <div
          className={`flex min-w-[120px] items-center justify-end gap-1.5 text-xs ${
            errorMessage
              ? 'text-red-500'
              : saveMessage === SAVE_SUCCESS_MESSAGE
                ? 'text-green-600'
                : 'text-gray-500 dark:text-gray-400'
          }`}
        >
          {statusIcon}
          <span>{statusText}</span>
        </div>
      </div>

      {isMarkdownDocument && markdownWarningText && (
        <div className="border-b border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 lg:hidden">
          {markdownWarningText}
        </div>
      )}

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
              <p className="text-sm font-medium text-gray-800 dark:text-gray-100">
                无法打开这个文本文件
              </p>
              <p className="mt-2 break-all text-xs text-gray-500 dark:text-gray-400">
                {errorMessage}
              </p>
            </div>
          </div>
        ) : isMarkdownDocument ? (
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center text-sm text-gray-400">
                <div className="flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  正在加载 Markdown 编辑器...
                </div>
              </div>
            }
          >
            <MarkdownEditorSurface
              editorId={markdownEditorId}
              documentPath={filePath}
              projectPath={projectPath}
              markdown={content}
              viewMode={markdownViewMode}
              showFrontmatterControls={isMdtDocument}
              frontmatterEditRequestKey={frontmatterEditRequestKey}
              insertRequest={insertRequest ?? undefined}
              onChange={handleMarkdownChange}
              onErrorChange={handleMarkdownErrorChange}
              onSave={handleSave}
            />
          </Suspense>
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

      {showLanguageMenu && !isMarkdownDocument && (
        <button
          type="button"
          className="fixed inset-0 z-40 cursor-default"
          onClick={dismissLanguageMenu}
          aria-label="关闭语言菜单"
        />
      )}

      {isMdtDocument && projectPath && filePath && (
        <MdtRelatedFilePickerDialog
          isOpen={isMdtFilePickerOpen}
          projectPath={projectPath}
          currentFilePath={filePath}
          onClose={() => setIsMdtFilePickerOpen(false)}
          onPick={handleAddMdtReference}
        />
      )}
    </div>
  );
}
