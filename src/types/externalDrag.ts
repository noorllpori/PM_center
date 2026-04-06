export type ExternalFileDragStatus = "dropped" | "cancelled" | "unsupported";

export interface ExternalFileDragResult {
  status: ExternalFileDragStatus;
}
