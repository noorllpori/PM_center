from __future__ import annotations

from nexora_sdk import (
    get_project_context,
    get_storage_directory,
    list_project_files,
    put_blob,
    read_request,
    set_state,
    write_error,
    write_result,
)


def inspect_project() -> None:
    context = get_project_context()
    page = list_project_files(limit=50)
    summary = {
        "project": context,
        "fileCountOnFirstPage": len(page.get("entries", [])),
        "nextCursor": page.get("nextCursor"),
    }
    # The JSON state API is stable and component-namespaced.
    set_state("lastProjectInspection", summary, scope="project")
    write_result(summary)


def write_project_blob(payload: object) -> None:
    values = payload if isinstance(payload, dict) else {}
    name = str(values.get("name", "note.txt")).strip()
    text = str(values.get("text", ""))
    write_result(put_blob(name, text, scope="project", kind="state"))


def resolve_storage_directory() -> None:
    write_result(get_storage_directory(scope="project", kind="cache"))


def main() -> None:
    request = read_request()
    if request.command == "inspect-project":
        inspect_project()
    elif request.command == "write-project-blob":
        write_project_blob(request.payload)
    elif request.command == "resolve-storage-directory":
        resolve_storage_directory()
    else:
        write_error(f"unsupported command: {request.command}")


if __name__ == "__main__":
    main()
