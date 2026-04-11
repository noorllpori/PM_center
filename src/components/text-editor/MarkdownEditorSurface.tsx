import { useEffect, useMemo, useRef } from 'react';
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
  GenericJsxEditor,
  headingsPlugin,
  InsertCodeBlock,
  InsertTable,
  InsertThematicBreak,
  jsxPlugin,
  linkDialogPlugin,
  linkPlugin,
  ListsToggle,
  listsPlugin,
  markdownShortcutPlugin,
  MDXEditor,
  type MDXEditorMethods,
  quotePlugin,
  Separator,
  tablePlugin,
  thematicBreakPlugin,
  toolbarPlugin,
  UndoRedo,
  type JsxComponentDescriptor,
} from '@mdxeditor/editor';
import '@mdxeditor/editor/style.css';
import './markdownEditor.css';
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

interface MarkdownEditorSurfaceProps {
  markdown: string;
  viewMode: MarkdownViewMode;
  onChange: (markdown: string, initialNormalize: boolean) => void;
  onSave?: () => void;
  onErrorChange?: (errorMessage: string | null) => void;
}

export function MarkdownEditorSurface({
  markdown,
  viewMode,
  onChange,
  onSave,
  onErrorChange,
}: MarkdownEditorSurfaceProps) {
  const editorRef = useRef<MDXEditorMethods>(null);
  const lastMarkdownRef = useRef(markdown);

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

  const plugins = useMemo(() => {
    const nextPlugins = [
      headingsPlugin(),
      listsPlugin(),
      quotePlugin(),
      thematicBreakPlugin(),
      markdownShortcutPlugin(),
      linkPlugin(),
      linkDialogPlugin(),
      tablePlugin(),
      codeBlockPlugin({
        defaultCodeBlockLanguage: 'txt',
      }),
      codeMirrorPlugin({
        codeBlockLanguages: CODE_BLOCK_LANGUAGES,
        codeMirrorExtensions: [codeMirrorMarkdown()],
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
  }, [viewMode]);

  return (
    <div
      className="pm-markdown-editor-shell light-theme h-full min-h-0 bg-white"
      onKeyDownCapture={handleKeyDownCapture}
    >
      <MDXEditor
        key={viewMode}
        ref={editorRef}
        className="pm-markdown-editor h-full min-h-0"
        contentEditableClassName="pm-markdown-editor__content"
        markdown={markdown}
        plugins={plugins}
        spellCheck
        onChange={(nextMarkdown, initialMarkdownNormalize) => {
          lastMarkdownRef.current = nextMarkdown;
          onErrorChange?.(null);
          onChange(nextMarkdown, initialMarkdownNormalize);
        }}
        onError={({ error }) => {
          onErrorChange?.(error);
        }}
      />
    </div>
  );
}
