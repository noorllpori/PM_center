import { invoke } from "@tauri-apps/api/core";
import type { ExternalFileDragResult } from "../types/externalDrag";

export async function startExternalFileDrag(
  paths: string[],
): Promise<ExternalFileDragResult> {
  return invoke("start_external_file_drag", { paths });
}

export function supportsExternalFileDrag(): boolean {
  return (
    typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent)
  );
}
