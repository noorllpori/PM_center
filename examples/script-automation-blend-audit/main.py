from __future__ import annotations

from pathlib import Path

from nexora_sdk import (
    call_component,
    emit_surface_event,
    log,
    progress,
    raise_if_cancelled,
    read_request,
    set_state,
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

    project_root = Path(request.context.project_path).resolve()
    candidate = Path(blend_path)
    if not candidate.is_absolute():
        candidate = project_root / candidate
    candidate = candidate.resolve()

    try:
        candidate.relative_to(project_root)
    except ValueError:
        write_error("blendPath must remain inside the current project")
        return

    if not candidate.is_file():
        write_error(f"file does not exist: {candidate}")
        return

    raise_if_cancelled(request.context)
    progress(0.35, "validated project file")
    log(f"run={request.context.run_id}")
    log(f"project={project_root}")
    log(f"blend={candidate}")
    blendio = call_component(
        "pmc.blendio",
        "inspect",
        {"path": str(candidate)},
        capability="project.files.read",
        timeout_ms=120_000,
    )
    raise_if_cancelled(request.context)
    progress(0.9, "BlenderIO inspection completed")
    summary = {
        "file": str(candidate),
        "sizeBytes": candidate.stat().st_size,
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
