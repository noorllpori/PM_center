from __future__ import annotations

from nexora_sdk import (
    call_component,
    emit_surface_event,
    log,
    progress,
    raise_if_cancelled,
    read_request,
    resolve_project_file,
    set_state,
    stat_project_file,
    write_error,
    write_result,
)


def main() -> None:
    request = read_request()
    if request.command != "inspect":
        write_error(f"unsupported command: {request.command}")
        return

    if not request.context.project_path:
        write_error("project context is required")
        return

    payload = request.payload if isinstance(request.payload, dict) else {}
    blend_path = str(payload.get("blendPath", "")).strip()
    if not blend_path.lower().endswith(".blend"):
        write_error("blendPath must point to a .blend file")
        return

    file_info = stat_project_file(blend_path)
    if file_info.get("isDirectory"):
        write_error("blendPath must point to a file")
        return
    resolved = resolve_project_file(blend_path)
    candidate = str(resolved["path"])

    raise_if_cancelled(request.context)
    progress(0.35, "validated project file")
    log(f"run={request.context.run_id}")
    log(f"project={request.context.project_path}")
    log(f"blend={candidate}")
    blendio = call_component(
        "pmc.blendio",
        "inspect",
        {"path": candidate},
        capability="project.files.read",
        timeout_ms=120_000,
    )
    raise_if_cancelled(request.context)
    progress(0.9, "BlenderIO inspection completed")
    summary = {
        "file": blend_path,
        "sizeBytes": file_info.get("sizeBytes", 0),
        "runId": request.context.run_id,
    }
    set_state("lastAudit", summary, scope="project")
    emit_surface_event(
        "nexora.example.blend-audit.surface",
        "audit.completed",
        summary,
    )
    write_result(
        {
            **summary,
            "blendio": blendio,
            "message": "依赖合同、Capability 和 BlenderIO 调用均已完成。",
        }
    )


if __name__ == "__main__":
    main()
