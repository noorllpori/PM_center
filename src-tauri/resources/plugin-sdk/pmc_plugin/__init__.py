import argparse
import json
from pathlib import Path


def load_request(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


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
