import { useEffect, useMemo, useRef } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { readFile } from '@tauri-apps/plugin-fs';
import { markdown as codeMirrorMarkdown } from '@codemirror/lang-markdown';
import {
  BlockTypeSelect,
  BoldItalicUnderlineToggles,
  ChangeCodeMirrorLanguage,
  CodeToggle,
  codeBlockPlugin,
  codeMirrorPlugin,
  ConditionalContents,
  CreateLink,
  diffSourcePlugin,
  frontmatterPlugin,
  GenericJsxEditor,
  headingsPlugin,
  imagePlugin,
  InsertCodeBlock,
  InsertTable,
  InsertThematicBreak,
  insertFrontmatter$,
  insertImage$,
  insertMarkdown$,
  jsxPlugin,
  linkDialogPlugin,
  linkPlugin,
  ListsToggle,
  listsPlugin,
  markdownShortcutPlugin,
  MDXEditor,
  RemoteMDXEditorRealmProvider,
  remoteRealmPlugin,
  type MDXEditorMethods,
  quotePlugin,
  Separator,
  tablePlugin,
  thematicBreakPlugin,
  toolbarPlugin,
  UndoRedo,
  useRemoteMDXEditorRealm,
  type JsxComponentDescriptor,
} from '@mdxeditor/editor';
import '@mdxeditor/editor/style.css';
import './markdownEditor.css';
import { getImageExtension, getImageMimeType, isPsdExtension } from '../image-viewer/imageViewerUtils';
import type { MarkdownViewMode } from './textEditorWindowTransfer';

const CODE_BLOCK_LANGUAGES = {
  txt: 'Plain text',
  md: 'Markdown',
  html: 'HTML',
  css: 'CSS',
  json: 'JSON',
  js: 'JavaScript',
  jsx: 'JavaScript React',
  ts: 'TypeScript',
  tsx: 'TypeScript React',
  bash: 'Bash',
  shell: 'Shell',
  python: 'Python',
  rust: 'Rust',
  sql: 'SQL',
  yaml: 'YAML',
};

const GENERIC_JSX_DESCRIPTORS: JsxComponentDescriptor[] = [
  {
    name: '*',
    kind: 'flow',
    props: [],
    hasChildren: true,
    Editor: GenericJsxEditor,
  },
  {
    name: '*',
    kind: 'text',
    props: [],
    hasChildren: true,
    Editor: GenericJsxEditor,
  },
];

const EDITOR_TRANSLATIONS: Record<string, string> = {
  'frontmatterEditor.title': '编辑文档元数据',
  'frontmatterEditor.key': '字段',
  'frontmatterEditor.value': '值',
  'frontmatterEditor.addEntry': '添加字段',
  'dialogControls.save': '保存',
  'dialogControls.cancel': '取消',
};

interface PsdPreviewDocument {
  width?: number;
  height?: number;
  canvas?: HTMLCanvasElement;
  children?: PsdPreviewLayer[];
  imageResources?: {
    thumbnail?: HTMLCanvasElement;
  };
}

interface PsdPreviewLayer {
  canvas?: HTMLCanvasElement;
  children?: PsdPreviewLayer[];
  hidden?: boolean;
  opacity?: number;
  left?: number;
  top?: number;
}

export interface MarkdownEditorInsertRequest {
  key: number;
  type: 'markdown' | 'image';
  markdown?: string;
  image?: {
    src: string;
    altText: string;
    title?: string;
  };
}

interface MarkdownEditorSurfaceProps {
  editorId: string;
  documentPath?: string;
  projectPath?: string;
  markdown: string;
  viewMode: MarkdownViewMode;
  showFrontmatterControls?: boolean;
  frontmatterEditRequestKey?: number;
  insertRequest?: MarkdownEditorInsertRequest;
  onChange: (markdown: string, initialNormalize: boolean) => void;
  onSave?: () => void;
  onErrorChange?: (errorMessage: string | null) => void;
}

function isDirectBrowserSource(source: string) {
  return /^(asset:|blob:|data:|https?:|http:\/\/asset\.localhost)/i.test(source);
}

function isAbsolutePath(path: string) {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('\\\\') || path.startsWith('/');
}

function getParentPath(path: string) {
  const parts = path.split(/[\\/]/);
  parts.pop();
  return parts.join(path.includes('\\') ? '\\' : '/');
}

function getPathSeparator(path: string) {
  return path.includes('\\') ? '\\' : '/';
}

function joinPath(basePath: string, relativePath: string) {
  const separator = getPathSeparator(basePath);
  const baseParts = basePath.split(/[\\/]+/).filter(Boolean);
  const relativeParts = relativePath.split(/[\\/]+/).filter(Boolean);
  const absolutePrefix = /^[A-Za-z]:$/.test(baseParts[0] || '')
    ? `${baseParts[0]}${separator}`
    : basePath.startsWith('\\')
      ? `${separator}${separator}`
      : basePath.startsWith('/')
        ? separator
        : '';

  const stack = absolutePrefix ? baseParts.slice(1) : [...baseParts];

  for (const part of relativeParts) {
    if (part === '.') {
      continue;
    }
    if (part === '..') {
      stack.pop();
      continue;
    }
    stack.push(part);
  }

  return absolutePrefix
    ? `${absolutePrefix}${stack.join(separator)}`
    : stack.join(separator);
}

function stripQueryAndHash(source: string) {
  return source.split('#')[0]?.split('?')[0] || source;
}

function resolveDocumentRelativePath(source: string, documentPath?: string, projectPath?: string) {
  if (!source || isDirectBrowserSource(source)) {
    return source;
  }

  const normalizedSource = stripQueryAndHash(source);
  if (!normalizedSource) {
    return source;
  }

  if (isAbsolutePath(normalizedSource)) {
    if (normalizedSource.startsWith('/') && projectPath && !/^[A-Za-z]:[\\/]/.test(normalizedSource)) {
      return joinPath(projectPath, normalizedSource.replace(/^[\\/]+/, ''));
    }
    return normalizedSource;
  }

  if (!documentPath) {
    return normalizedSource;
  }

  return joinPath(getParentPath(documentPath), normalizedSource);
}

function toExactArrayBuffer(bytes: Uint8Array) {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
}

function canvasToBlob(canvas: HTMLCanvasElement) {
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) {
        resolve(blob);
        return;
      }

      reject(new Error('Unable to create PSD preview image.'));
    }, 'image/png');
  });
}

function stringifyEditorError(error: unknown) {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  if (error && typeof error === 'object') {
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }

  return String(error);
}

function drawPsdLayers(context: CanvasRenderingContext2D, layers: PsdPreviewLayer[]) {
  for (const layer of [...layers].reverse()) {
    if (layer.hidden) {
      continue;
    }

    if (layer.children?.length) {
      drawPsdLayers(context, layer.children);
      continue;
    }

    if (!layer.canvas) {
      continue;
    }

    context.save();
    context.globalAlpha = typeof layer.opacity === 'number' ? layer.opacity : 1;
    context.drawImage(layer.canvas, layer.left ?? 0, layer.top ?? 0);
    context.restore();
  }
}

function composePsdPreview(psd: PsdPreviewDocument) {
  if (!psd.width || !psd.height || !psd.children?.length) {
    return null;
  }

  const canvas = document.createElement('canvas');
  canvas.width = psd.width;
  canvas.height = psd.height;

  const context = canvas.getContext('2d');
  if (!context) {
    return null;
  }

  drawPsdLayers(context, psd.children);
  return canvas;
}

async function resolvePsdPreviewSource(source: string) {
  const bytes = await readFile(source);
  const { readPsd } = await import('ag-psd');
  const buffer = toExactArrayBuffer(bytes);
  let psd = readPsd(buffer, {
    skipLayerImageData: true,
  }) as PsdPreviewDocument;

  let previewCanvas: HTMLCanvasElement | null | undefined = psd.canvas ?? psd.imageResources?.thumbnail;
  if (!previewCanvas) {
    psd = readPsd(buffer) as PsdPreviewDocument;
    previewCanvas = psd.canvas ?? psd.imageResources?.thumbnail ?? composePsdPreview(psd);
  }

  if (!previewCanvas) {
    throw new Error('PSD preview is unavailable.');
  }

  const blob = await canvasToBlob(previewCanvas);
  return URL.createObjectURL(blob);
}

function MarkdownEditorBridge({
  editorId,
  showFrontmatterControls,
  frontmatterEditRequestKey = 0,
  insertRequest,
}: {
  editorId: string;
  showFrontmatterControls: boolean;
  frontmatterEditRequestKey?: number;
  insertRequest?: MarkdownEditorInsertRequest;
}) {
  const realm = useRemoteMDXEditorRealm(editorId);
  const handledFrontmatterRequestKeyRef = useRef(0);
  const handledInsertRequestKeyRef = useRef(0);

  useEffect(() => {
    if (!showFrontmatterControls || !realm || frontmatterEditRequestKey <= handledFrontmatterRequestKeyRef.current) {
      return;
    }

    realm.pub(insertFrontmatter$);
    handledFrontmatterRequestKeyRef.current = frontmatterEditRequestKey;
  }, [frontmatterEditRequestKey, realm, showFrontmatterControls]);

  useEffect(() => {
    if (!realm || !insertRequest || insertRequest.key <= handledInsertRequestKeyRef.current) {
      return;
    }

    if (insertRequest.type === 'image' && insertRequest.image) {
      realm.pub(insertImage$, insertRequest.image);
    } else if (insertRequest.type === 'markdown' && insertRequest.markdown) {
      realm.pub(insertMarkdown$, insertRequest.markdown);
    }

    handledInsertRequestKeyRef.current = insertRequest.key;
  }, [insertRequest, realm]);

  return null;
}

export function MarkdownEditorSurface({
  editorId,
  documentPath,
  projectPath,
  markdown,
  viewMode,
  showFrontmatterControls = false,
  frontmatterEditRequestKey = 0,
  insertRequest,
  onChange,
  onSave,
  onErrorChange,
}: MarkdownEditorSurfaceProps) {
  const editorRef = useRef<MDXEditorMethods>(null);
  const lastMarkdownRef = useRef(markdown);
  const translation = useMemo(
    () => (key: string, defaultValue: string) => EDITOR_TRANSLATIONS[key] ?? defaultValue,
    [],
  );

  useEffect(() => {
    if (markdown === lastMarkdownRef.current) {
      return;
    }

    editorRef.current?.setMarkdown(markdown);
    lastMarkdownRef.current = markdown;
  }, [markdown]);

  const handleKeyDownCapture = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
      event.preventDefault();
      onSave?.();
    }
  };

  const imagePreviewHandler = useMemo(
    () => async (imageSource: string) => {
      if (isDirectBrowserSource(imageSource)) {
        return imageSource;
      }

      const resolvedPath = resolveDocumentRelativePath(imageSource, documentPath, projectPath);
      if (!resolvedPath || isDirectBrowserSource(resolvedPath)) {
        return resolvedPath;
      }

      const extension = getImageExtension(resolvedPath);
      if (isPsdExtension(extension)) {
        return resolvePsdPreviewSource(resolvedPath);
      }

      if (extension === 'svg') {
        return convertFileSrc(resolvedPath);
      }

      if (!getImageMimeType(resolvedPath).startsWith('image/')) {
        return convertFileSrc(resolvedPath);
      }

      return convertFileSrc(resolvedPath);
    },
    [documentPath, projectPath],
  );

  const plugins = useMemo(() => {
    const nextPlugins = [
      headingsPlugin(),
      listsPlugin(),
      quotePlugin(),
      thematicBreakPlugin(),
      markdownShortcutPlugin(),
      linkPlugin(),
      linkDialogPlugin(),
      frontmatterPlugin(),
      imagePlugin({
        imagePreviewHandler,
      }),
      tablePlugin(),
      codeBlockPlugin({
        defaultCodeBlockLanguage: 'txt',
      }),
      codeMirrorPlugin({
        codeBlockLanguages: CODE_BLOCK_LANGUAGES,
        codeMirrorExtensions: [codeMirrorMarkdown()],
      }),
      remoteRealmPlugin({
        editorId,
      }),
      jsxPlugin({
        jsxComponentDescriptors: GENERIC_JSX_DESCRIPTORS,
        allowFragment: true,
      }),
      diffSourcePlugin({
        viewMode,
        codeMirrorExtensions: [codeMirrorMarkdown()],
      }),
    ];

    if (viewMode === 'rich-text') {
      nextPlugins.push(
        toolbarPlugin({
          toolbarContents: () => (
            <ConditionalContents
              options={[
                {
                  when: (editor) => editor?.editorType === 'codeblock',
                  contents: () => (
                    <>
                      <ChangeCodeMirrorLanguage />
                      <Separator />
                      <UndoRedo />
                    </>
                  ),
                },
                {
                  fallback: () => (
                    <>
                      <UndoRedo />
                      <Separator />
                      <BlockTypeSelect />
                      <Separator />
                      <BoldItalicUnderlineToggles />
                      <CodeToggle />
                      <Separator />
                      <ListsToggle />
                      <CreateLink />
                      <Separator />
                      <InsertCodeBlock />
                      <InsertTable />
                      <InsertThematicBreak />
                    </>
                  ),
                },
              ]}
            />
          ),
        }),
      );
    }

    return nextPlugins;
  }, [editorId, imagePreviewHandler, viewMode]);

  return (
    <RemoteMDXEditorRealmProvider>
      <div
        className="pm-markdown-editor-shell light-theme h-full min-h-0 bg-white"
        onKeyDownCapture={handleKeyDownCapture}
      >
        <MarkdownEditorBridge
          editorId={editorId}
          showFrontmatterControls={showFrontmatterControls}
          frontmatterEditRequestKey={frontmatterEditRequestKey}
          insertRequest={insertRequest}
        />
        <MDXEditor
          key={viewMode}
          ref={editorRef}
          className="pm-markdown-editor h-full min-h-0"
          contentEditableClassName="pm-markdown-editor__content"
          markdown={markdown}
          plugins={plugins}
          spellCheck
          translation={translation}
          onChange={(nextMarkdown, initialMarkdownNormalize) => {
            lastMarkdownRef.current = nextMarkdown;
            onErrorChange?.(null);
            onChange(nextMarkdown, initialMarkdownNormalize);
          }}
          onError={({ error }) => {
            onErrorChange?.(stringifyEditorError(error));
          }}
        />
      </div>
    </RemoteMDXEditorRealmProvider>
  );
}
