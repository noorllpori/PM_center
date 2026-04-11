import type { EditorLanguage } from '../../stores/windowStore';

export type MarkdownViewMode = 'rich-text' | 'source';

export interface TextEditorTransferPayload {
  filePath: string;
  title: string;
  content: string;
  originalContent: string;
  language: EditorLanguage;
  isDirty: boolean;
  markdownViewMode?: MarkdownViewMode;
}

export interface TextEditorDetachReadyPayload {
  targetLabel: string;
}

export interface TextEditorDetachAckPayload {
  targetLabel: string;
}

export function createTextDetachTransferId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createTextDetachReadyEvent(transferId: string) {
  return `pm-center:text-detach-ready:${transferId}`;
}

export function createTextDetachPayloadEvent(transferId: string) {
  return `pm-center:text-detach-payload:${transferId}`;
}

export function createTextDetachAckEvent(transferId: string) {
  return `pm-center:text-detach-ack:${transferId}`;
}
