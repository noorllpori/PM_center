import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
  Archive,
  Download,
  File,
  FileImage,
  FileVideo,
  FolderOpen,
  SquareArrowOutUpRight,
  RefreshCw,
  X,
} from 'lucide-react';
import type { LanTransfer, LanTransferProgress } from '../../stores/lanCollaborationStore';

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return bytes === 0 ? '0 B' : '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function TransferIcon({ mimeType }: { mimeType: string | null }) {
  if (mimeType?.startsWith('image/')) return <FileImage className="h-5 w-5" />;
  if (mimeType?.startsWith('video/')) return <FileVideo className="h-5 w-5" />;
  if (mimeType?.includes('zip') || mimeType?.includes('compressed') || mimeType?.includes('archive')) {
    return <Archive className="h-5 w-5" />;
  }
  return <File className="h-5 w-5" />;
}

function statusLabel(transfer: LanTransfer) {
  const autoReceivingImage = transfer.direction === 'incoming'
    && transfer.status === 'pending'
    && transfer.totalBytes <= 100 * 1024 * 1024
    && transfer.mimeType?.startsWith('image/');
  if (autoReceivingImage) return '正在自动接收';
  switch (transfer.status) {
    case 'waiting': return '等待对方接收';
    case 'pending': return '待接收';
    case 'transferring': return transfer.direction === 'incoming' ? '正在接收' : '正在发送';
    case 'completed': return transfer.direction === 'incoming' ? '已接收' : '已发送';
    case 'rejected': return '已拒绝';
    case 'failed': return '传输失败';
    default: return transfer.status;
  }
}

export function LanTransferCard({
  transfer,
  progress,
  avatar,
  busy = false,
  onAccept,
  onReject,
}: {
  transfer: LanTransfer;
  progress?: LanTransferProgress;
  avatar?: React.ReactNode;
  busy?: boolean;
  onAccept: (transfer: LanTransfer) => void;
  onReject: (transfer: LanTransfer) => void;
}) {
  const outgoing = transfer.direction === 'outgoing';
  const localPath = transfer.receivedPath || (outgoing ? transfer.sourcePath : null);
  const transferredBytes = transfer.status === 'completed'
    ? transfer.totalBytes
    : progress?.transferredBytes || 0;
  const progressPercent = transfer.totalBytes > 0
    ? Math.min(100, Math.round((transferredBytes / transfer.totalBytes) * 100))
    : transfer.status === 'completed' ? 100 : 0;
  const autoReceivingImage = !outgoing
    && transfer.status === 'pending'
    && transfer.totalBytes <= 100 * 1024 * 1024
    && transfer.mimeType?.startsWith('image/');
  const canRespond = !outgoing
    && !autoReceivingImage
    && (transfer.status === 'pending' || transfer.status === 'failed');

  return (
    <div className={`flex gap-2.5 ${outgoing ? 'justify-end' : 'justify-start'}`}>
      {!outgoing ? avatar : null}
      <div className={`w-full max-w-[min(78%,520px)] ${outgoing ? 'items-end' : 'items-start'}`}>
        {!outgoing ? <p className="mb-1 px-1 text-[11px] text-gray-500">{transfer.fromName}</p> : null}
        <div className="overflow-hidden rounded-md border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
          {transfer.mimeType?.startsWith('image/') && localPath ? (
            <img
              src={convertFileSrc(localPath)}
              alt=""
              className="max-h-52 w-full bg-gray-100 object-contain dark:bg-gray-950"
            />
          ) : null}
          <div className="flex items-start gap-3 p-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-300">
              <TransferIcon mimeType={transfer.mimeType} />
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium" title={transfer.displayName}>{transfer.displayName}</p>
              <p className="mt-0.5 text-xs text-gray-500">
                {formatBytes(transfer.totalBytes)} · {statusLabel(transfer)}
                {transfer.status === 'transferring' && progress?.bytesPerSecond
                  ? ` · ${formatBytes(progress.bytesPerSecond)}/s`
                  : ''}
              </p>
              {transfer.error ? <p className="mt-1 line-clamp-2 text-xs text-red-500">{transfer.error}</p> : null}
            </div>
          </div>
          {transfer.status === 'transferring' ? (
            <div className="h-1 bg-gray-100 dark:bg-gray-800">
              <div className="h-full bg-blue-500 transition-[width]" style={{ width: `${progressPercent}%` }} />
            </div>
          ) : null}
          {canRespond ? (
            <div className="flex justify-end gap-2 border-t border-gray-100 px-3 py-2 dark:border-gray-800">
              <button type="button" disabled={busy} onClick={() => onReject(transfer)} className="inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-50 dark:text-gray-300 dark:hover:bg-gray-800">
                <X className="h-3.5 w-3.5" />拒绝
              </button>
              <button type="button" disabled={busy} onClick={() => onAccept(transfer)} className="inline-flex items-center gap-1.5 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50">
                {transfer.status === 'failed' ? <RefreshCw className="h-3.5 w-3.5" /> : <Download className="h-3.5 w-3.5" />}
                {transfer.status === 'failed' ? '重新接收' : '接收'}
              </button>
            </div>
          ) : localPath && transfer.status === 'completed' ? (
            <div className="flex justify-end gap-1 border-t border-gray-100 px-3 py-2 dark:border-gray-800">
              <button type="button" onClick={() => void invoke('show_in_folder', { path: localPath })} className="inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800">
                <FolderOpen className="h-3.5 w-3.5" />所在目录
              </button>
              <button type="button" onClick={() => void invoke('open_file', { path: localPath })} className="inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs text-blue-600 hover:bg-blue-50 dark:text-blue-300 dark:hover:bg-blue-950/30">
                <SquareArrowOutUpRight className="h-3.5 w-3.5" />打开文件
              </button>
            </div>
          ) : null}
        </div>
        <p className={`mt-1 px-1 text-[10px] text-gray-400 ${outgoing ? 'text-right' : ''}`}>
          {new Date(transfer.createdAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}
        </p>
      </div>
    </div>
  );
}
