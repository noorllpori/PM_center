from __future__ import annotations

import time
from typing import Any

from nexora_sdk import (
    emit_surface_event,
    log,
    progress,
    raise_if_cancelled,
    read_request,
    set_state,
    write_error,
    write_result,
)


SURFACE_ID = "nexora.example.automation-acceptance.surface"


def payload_object(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def bounded_seconds(payload: dict[str, Any], default: int) -> int:
    value = payload.get("seconds", default)
    return min(60, max(1, int(value)))


def main() -> None:
    request = read_request()
    payload = payload_object(request.payload)
    log(f"command={request.command} run={request.context.run_id}")

    if request.command == "global-probe":
        # The host must remove any project context before Python receives this command.
        if request.context.project_path:
            write_error("global command unexpectedly received a project context")
            return
        progress(0.5, "global context confirmed")
        result = {"kind": "global", "runId": request.context.run_id, "projectPath": None}
        set_state("lastProbe", result)
        emit_surface_event(SURFACE_ID, "probe.completed", result)
        write_result(result)
        return

    if request.command == "either-probe":
        progress(0.5, "either context confirmed")
        result = {
            "kind": "either",
            "runId": request.context.run_id,
            "projectPath": request.context.project_path,
        }
        set_state("lastProbe", result)
        emit_surface_event(SURFACE_ID, "probe.completed", result)
        write_result(result)
        return

    if request.command in {"long-probe", "non-idempotent-probe"}:
        seconds = bounded_seconds(payload, 20)
        for elapsed in range(seconds):
            raise_if_cancelled(request.context)
            progress(elapsed / seconds, f"running {elapsed + 1}/{seconds}")
            time.sleep(1)
        raise_if_cancelled(request.context)
        write_result(
            {
                "kind": request.command,
                "runId": request.context.run_id,
                "seconds": seconds,
                "completed": True,
            }
        )
        return

    if request.command == "retry-probe":
        # The runtime should create a second attempt after its normal backoff.
        write_error("intentional idempotent failure for retry verification")
        return

    write_error(f"unsupported command: {request.command}")


if __name__ == "__main__":
    main()
