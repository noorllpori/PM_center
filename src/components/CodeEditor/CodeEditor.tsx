import { useEffect, useRef, useCallback } from 'react';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { indentOnInput, syntaxHighlighting, defaultHighlightStyle, bracketMatching } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { python } from '@codemirror/lang-python';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { rust } from '@codemirror/lang-rust';
import { markdown } from '@codemirror/lang-markdown';
import type { EditorLanguage } from '../../stores/windowStore';

interface CodeEditorProps {
  initialContent?: string;
  language?: EditorLanguage;
  theme?: 'light' | 'dark';
  readOnly?: boolean;
  onChange?: (content: string) => void;
  onSave?: () => void;
  className?: string;
}

// 语言映射
const languageExtensions: Record<EditorLanguage, () => Extension> = {
  python: () => python(),
  javascript: () => javascript(),
  typescript: () => javascript({ typescript: true }),
  html: () => html(),
  css: () => css(),
  json: () => json(),
  rust: () => rust(),
  markdown: () => markdown(),
  plaintext: () => [],
};

export function CodeEditor({
  initialContent = '',
  language = 'plaintext',
  theme = 'light',
  readOnly = false,
  onChange,
  onSave,
  className = '',
}: CodeEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  // 保存回调的 ref，避免重新创建编辑器
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  
  useEffect(() => {
    onChangeRef.current = onChange;
    onSaveRef.current = onSave;
  }, [onChange, onSave]);

  // 创建编辑器
  const createEditor = useCallback(() => {
    if (!editorRef.current) return;

    const extensions: Extension[] = [
      // 基础功能
      lineNumbers(),
      highlightActiveLineGutter(),
      history(),
      closeBrackets(),
      bracketMatching(),
      indentOnInput(),
      
      // 按键映射
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...closeBracketsKeymap,
        {
          key: 'Ctrl-s',
          preventDefault: true,
          run: () => {
            onSaveRef.current?.();
            return true;
          },
        },
      ]),
      
      // 语法高亮
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      
      // 主题
      theme === 'dark' ? oneDark : [],
      
      // 语言支持
      languageExtensions[language]?.() || [],
      
      // 变更监听
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current?.(update.state.doc.toString());
        }
      }),
      
      // 只读
      readOnly ? EditorView.editable.of(false) : [],
      
      // 基础样式
      EditorView.theme({
        '&': {
          fontSize: '14px',
        },
        '.cm-content': {
          fontFamily: '"JetBrains Mono", "Fira Code", "Consolas", monospace',
          padding: '10px',
        },
        '.cm-gutters': {
          backgroundColor: theme === 'dark' ? '#1e1e1e' : '#f5f5f5',
          borderRight: 'none',
        },
        '.cm-activeLineGutter': {
          backgroundColor: theme === 'dark' ? '#2a2a2a' : '#e8e8e8',
        },
        '.cm-activeLine': {
          backgroundColor: theme === 'dark' ? '#2a2a2a40' : '#e8e8e840',
        },
      }),
    ];

    const state = EditorState.create({
      doc: initialContent,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: editorRef.current,
    });

    viewRef.current = view;
  }, [theme, language, readOnly, initialContent]);

  // 初始化编辑器
  useEffect(() => {
    createEditor();
    
    return () => {
      viewRef.current?.destroy();
      viewRef.current = null;
    };
  }, [createEditor]);

  // 更新内容（外部控制）
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    
    const currentContent = view.state.doc.toString();
    if (initialContent !== currentContent) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: initialContent },
      });
    }
  }, [initialContent]);

  // 更新语言
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    
    // 重新创建编辑器以应用新语言
    view.destroy();
    createEditor();
  }, [language, createEditor]);

  return (
    <div
      ref={editorRef}
      className={`w-full h-full overflow-hidden ${className}`}
      style={{ fontFamily: 'monospace' }}
    />
  );
}

// 语言选择器组件
interface LanguageSelectorProps {
  value: EditorLanguage;
  onChange: (lang: EditorLanguage) => void;
  className?: string;
}

export function LanguageSelector({ value, onChange, className = '' }: LanguageSelectorProps) {
  const languages: EditorLanguage[] = [
    'python',
    'javascript',
    'typescript',
    'html',
    'css',
    'json',
    'rust',
    'markdown',
    'plaintext',
  ];

  const getDisplayName = (lang: EditorLanguage): string => {
    const names: Record<EditorLanguage, string> = {
      python: 'Python',
      javascript: 'JavaScript',
      typescript: 'TypeScript',
      html: 'HTML',
      css: 'CSS',
      json: 'JSON',
      rust: 'Rust',
      markdown: 'Markdown',
      plaintext: 'Plain Text',
    };
    return names[lang];
  };

  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as EditorLanguage)}
      className={`px-2 py-1 text-xs bg-gray-100 dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded ${className}`}
    >
      {languages.map((lang) => (
        <option key={lang} value={lang}>
          {getDisplayName(lang)}
        </option>
      ))}
    </select>
  );
}
