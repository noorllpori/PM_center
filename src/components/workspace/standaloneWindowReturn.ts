import type { TextEditorTransferPayload } from '../text-editor/textEditorWindowTransfer';

export const STANDALONE_RETURN_TO_WORKSPACE_EVENT = 'pm-center:standalone-return-to-workspace';

export type StandaloneReturnFileType = 'image' | 'video' | 'text';

export interface StandaloneReturnToWorkspacePayload {
  projectPath: string;
  filePath: string;
  fileType: StandaloneReturnFileType;
  textEditorSnapshot?: TextEditorTransferPayload;
}
