import argparse
import json
from pathlib import Path


def load_request(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def get_settings(request):
    return request.get("settingsValues", {}) or {}


def get_setting(request, key, default=None):
    return get_settings(request).get(key, default)


def get_interaction_responses(request):
    return request.get("interactionResponses", []) or []


def get_interaction_response(request, request_id):
    for response in get_interaction_responses(request):
        if response.get("requestId") == request_id:
            return response
    return None


def is_confirmed(request, request_id):
    response = get_interaction_response(request, request_id)
    return bool(response and response.get("approved"))


def emit(event_type, **payload):
    message = {"type": event_type, **payload}
    print(f"@pmc {json.dumps(message, ensure_ascii=False)}", flush=True)


def progress(value):
    emit("progress", value=max(0, min(100, int(value))))


def toast(message, title=None, tone="info"):
    emit("toast", title=title, message=message, tone=tone)


def refresh(scope="project", path=None):
    emit("refresh", scope=scope, path=path)


def result(data):
    emit("result", data=data)


def error(message):
    emit("error", message=message)


def confirm(
    message,
    request_id,
    title=None,
    confirm_text="确认",
    cancel_text="取消",
    data=None,
):
    emit(
        "confirm",
        title=title,
        message=message,
        requestId=request_id,
        confirmText=confirm_text,
        cancelText=cancel_text,
        data=data,
    )


def run(handler):
    parser = argparse.ArgumentParser()
    parser.add_argument("--pmc-request", required=True)
    args = parser.parse_args()
    request = load_request(args.pmc_request)

    try:
        handler(request)
    except Exception as exc:
        error(str(exc))
        raise
