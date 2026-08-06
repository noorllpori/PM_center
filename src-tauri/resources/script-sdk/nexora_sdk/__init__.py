"""Nexora script component SDK v1.

The SDK only describes the structured component protocol. Host capabilities are
still approved and enforced by Nexora before a component command starts.
"""

from __future__ import annotations

import json
import os
import socket
import sys
from dataclasses import dataclass
from typing import Any, Dict


_current_context: Context | None = None


@dataclass(frozen=True)
class Context:
    run_id: str
    operation_id: str
    attempt: int
    profile_id: str
    profile_revision: int
    project_path: str | None
    trigger: Dict[str, Any]
    component: Dict[str, Any]
    cancellation_file: str | None
    bridge_protocol: str | None
    bridge_endpoint: str | None
    bridge_token: str | None


@dataclass(frozen=True)
class Request:
    command: str
    payload: Any
    context: Context
    raw: Dict[str, Any]


def read_request() -> Request:
    global _current_context
    raw = json.loads(sys.stdin.readline().lstrip("\ufeff"))
    automation = raw.get("input", {}).get("nexora", {})
    bridge = automation.get("bridge") or {}
    payload = raw.get("input", {}).get("payload")
    context = Context(
        run_id=str(automation.get("runId", "")),
        operation_id=str(automation.get("operationId", raw.get("operationId", ""))),
        attempt=int(automation.get("attempt", 1)),
        profile_id=str(automation.get("profileId", "")),
        profile_revision=int(automation.get("profileRevision", 0)),
        project_path=automation.get("projectPath"),
        trigger=dict(automation.get("trigger") or {}),
        component=dict(automation.get("component") or {}),
        cancellation_file=raw.get("cancellationFile"),
        bridge_protocol=bridge.get("protocol"),
        bridge_endpoint=bridge.get("endpoint"),
        bridge_token=bridge.get("token"),
    )
    _current_context = context
    return Request(
        command=str(raw.get("command", "")),
        payload=payload,
        context=context,
        raw=raw,
    )


def log(message: Any) -> None:
    print(str(message), file=sys.stderr, flush=True)


def progress(value: float, message: Any = "") -> None:
    normalized = max(0.0, min(1.0, float(value)))
    print(
        json.dumps(
            {
                "type": "progress",
                "operationId": _current_context.operation_id if _current_context else "",
                "progress": normalized,
                "message": str(message),
            },
            ensure_ascii=False,
        ),
        flush=True,
    )


def is_cancelled(context: Context) -> bool:
    return bool(context.cancellation_file and os.path.isfile(context.cancellation_file))


def raise_if_cancelled(context: Context) -> None:
    if is_cancelled(context):
        raise InterruptedError("Nexora cancelled this operation")


def _bridge_request(operation: str, payload: Dict[str, Any], timeout: float = 30.0) -> Any:
    context = _current_context
    if not context or not context.bridge_endpoint or not context.bridge_token:
        raise RuntimeError("Nexora SDK bridge is unavailable for this request")
    host, separator, port = context.bridge_endpoint.rpartition(":")
    if not separator or not host or not port.isdigit():
        raise RuntimeError("Nexora SDK bridge endpoint is invalid")
    request = {
        "protocol": context.bridge_protocol or "nexora.automation.bridge.v1",
        "token": context.bridge_token,
        "operation": operation,
        "payload": payload,
    }
    with socket.create_connection((host, int(port)), timeout=max(1.0, timeout)) as client:
        client.settimeout(max(1.0, timeout))
        stream = client.makefile("rwb")
        stream.write(json.dumps(request, ensure_ascii=False).encode("utf-8") + b"\n")
        stream.flush()
        line = stream.readline()
    if not line:
        raise RuntimeError("Nexora SDK bridge closed without a response")
    response = json.loads(line.decode("utf-8"))
    if not response.get("ok"):
        raise RuntimeError(str(response.get("error") or "Nexora SDK bridge request failed"))
    return response.get("result")


def call_component(
    component_id: str,
    command: str,
    payload: Any = None,
    *,
    capability: str | None = None,
    timeout_ms: int | None = None,
) -> Any:
    """Call a command on a dependency declared by this component's manifest."""
    request: Dict[str, Any] = {
        "componentId": component_id,
        "command": command,
        "input": payload,
    }
    if capability:
        request["capability"] = capability
    if timeout_ms is not None:
        request["timeoutMs"] = max(100, int(timeout_ms))
    response = _bridge_request(
        "component.invoke",
        request,
        timeout=(request.get("timeoutMs", 30000) / 1000.0) + 5.0,
    )
    return response.get("output") if isinstance(response, dict) else response


def get_state(key: str, default: Any = None, *, scope: str = "global") -> Any:
    value = _bridge_request("state.get", {"key": key, "scope": scope})
    return default if value is None else value


def set_state(key: str, value: Any, *, scope: str = "global") -> Any:
    return _bridge_request("state.set", {"key": key, "value": value, "scope": scope})


def delete_state(key: str, *, scope: str = "global") -> Any:
    return _bridge_request("state.delete", {"key": key, "scope": scope})


def emit_surface_event(surface_id: str, event: str, payload: Any = None) -> Any:
    return _bridge_request(
        "surface.emit",
        {"surfaceId": surface_id, "event": event, "payload": payload},
    )


def write_result(value: Any) -> None:
    print(json.dumps({"ok": True, "result": value}, ensure_ascii=False), flush=True)


def write_error(message: Any) -> None:
    print(json.dumps({"ok": False, "error": str(message)}, ensure_ascii=False), flush=True)
