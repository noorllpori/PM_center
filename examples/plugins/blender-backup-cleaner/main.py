import re
from pathlib import Path

from pmc_plugin import (
    confirm,
    get_interaction_response,
    refresh,
    result,
    run,
    toast,
)

CONFIRM_REQUEST_ID = "confirm-delete-blender-backups"
BACKUP_PATTERN = re.compile(r".+\.blend\d+$", re.IGNORECASE)


def resolve_project_root(request):
    project_path = request.get("projectPath")
    if project_path:
        target = Path(project_path)
        if target.is_dir():
            return target
        return target.parent
    return Path(".").resolve()


def list_backup_files(project_root):
    if not project_root.exists():
        return []

    return sorted(
        [
            path
            for path in project_root.rglob("*")
            if path.is_file() and BACKUP_PATTERN.fullmatch(path.name)
        ],
        key=lambda path: str(path.relative_to(project_root)).lower(),
    )


def emit_confirm(project_root, backup_files):
    items = [str(path.relative_to(project_root)) for path in backup_files]
    message = (
        f"将在当前活动项目中删除以下 {len(backup_files)} 个 Blender 备份文件：\n"
        f"{project_root}"
    )
    confirm(
        message,
        request_id=CONFIRM_REQUEST_ID,
        title="清理 Blender 备份文件",
        confirm_text="确认删除",
        cancel_text="取消",
        data={
            "projectRoot": str(project_root),
            "items": items,
            "paths": [str(path) for path in backup_files],
        },
    )


def delete_confirmed_files(response_data):
    deleted = []
    missing = []
    failed = []

    for path_str in response_data.get("paths", []):
        path = Path(path_str)
        if not path.exists():
            missing.append(str(path))
            continue

        try:
            path.unlink()
            deleted.append(str(path))
        except Exception as exc:
            failed.append(
                {
                    "path": str(path),
                    "error": str(exc),
                }
            )

    return deleted, missing, failed


def handle(request):
    project_root = resolve_project_root(request)
    backup_files = list_backup_files(project_root)

    if not backup_files:
        message = "当前活动项目中没有发现 Blender 备份文件。"
        toast(message, title="Blender Backup Cleaner", tone="info")
        result(
            {
                "deletedCount": 0,
                "message": message,
                "projectRoot": str(project_root),
            }
        )
        return

    interaction = get_interaction_response(request, CONFIRM_REQUEST_ID)
    if not interaction or not interaction.get("approved"):
        emit_confirm(project_root, backup_files)
        return

    response_data = interaction.get("data") or {}
    deleted, missing, failed = delete_confirmed_files(response_data)

    if deleted:
        refresh(scope="project", path=str(project_root))

    if failed:
        toast(
            f"已删除 {len(deleted)} 个备份文件，失败 {len(failed)} 个。",
            title="Blender Backup Cleaner",
            tone="warning",
        )
    else:
        toast(
            f"已删除 {len(deleted)} 个 Blender 备份文件。",
            title="Blender Backup Cleaner",
            tone="success",
        )

    result(
        {
            "projectRoot": str(project_root),
            "deletedCount": len(deleted),
            "missingCount": len(missing),
            "failedCount": len(failed),
            "deleted": deleted,
            "missing": missing,
            "failed": failed,
        }
    )


if __name__ == "__main__":
    run(handle)
