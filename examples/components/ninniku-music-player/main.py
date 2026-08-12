"""Directory-backed library actions for the Ninniku music player example.

The sandbox page never receives a directory handle. This trusted Python action
uses the public File Operations component after Nexora has granted the exact
directory and read capabilities requested by the current command.
"""

from __future__ import annotations

from pathlib import PureWindowsPath
from time import time
from typing import Any

from nexora_sdk import (
    NexoraBridgeError,
    call_file_operation,
    emit_surface_event,
    external_location,
    get_state,
    log,
    raise_if_cancelled,
    read_request,
    set_state,
    write_error,
    write_result,
)


SURFACE_ID = "com.ninniku.music-player.surface"
LIBRARIES_STATE_KEY = "musicLibraries"
AUDIO_EXTENSIONS = ("mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "wma", "webm")
SEARCH_LIMIT_PER_EXTENSION = 250
MAX_TRACKS_PER_LIBRARY = 1_000
# File Operations streams are bound to this component and its directory grant.
# They avoid the 4 MiB single-read limit without ever exposing a local path to
# the isolated page. The page still materializes a Blob for WebView playback,
# so retain a bounded size until a native media-stream bridge is available.
STREAM_CHUNK_BYTES = 512 * 1024
MAX_INLINE_AUDIO_BYTES = 64 * 1024 * 1024


def operation_output(response: Any) -> dict[str, Any]:
    if isinstance(response, dict) and isinstance(response.get("output"), dict):
        return response["output"]
    return response if isinstance(response, dict) else {}


def track_name(file_name: str) -> tuple[str, str]:
    stem = file_name.rsplit(".", 1)[0] if "." in file_name else file_name
    if " - " in stem:
        artist, title = stem.split(" - ", 1)
        if artist.strip() and title.strip():
            return artist.strip(), title.strip()
    return "音乐文件夹", stem or file_name


def stored_libraries() -> list[dict[str, Any]]:
    value = get_state(LIBRARIES_STATE_KEY, [])
    return [item for item in value if isinstance(item, dict)] if isinstance(value, list) else []


def save_libraries(libraries: list[dict[str, Any]]) -> None:
    set_state(LIBRARIES_STATE_KEY, libraries)


def publish(event: str, payload: dict[str, Any]) -> None:
    emit_surface_event(SURFACE_ID, event, payload)


def scan_library(library: dict[str, Any], context: Any) -> dict[str, Any]:
    grant_id = str(library.get("grantId") or "")
    root_path = str(library.get("rootPath") or "")
    if not grant_id or not root_path:
        raise ValueError("音乐目录记录缺少 grantId 或 rootPath")

    tracks: list[dict[str, Any]] = []
    seen_paths: set[str] = set()
    for extension in AUDIO_EXTENSIONS:
        raise_if_cancelled(context)
        result = call_file_operation(
            "entry.search",
            {
                "location": external_location(root_path, grant_id),
                "extension": extension,
                "kind": "file",
                "limit": SEARCH_LIMIT_PER_EXTENSION,
            },
            capability="filesystem.external.read",
            timeout_ms=60_000,
        )
        for entry in operation_output(result).get("entries", []):
            if not isinstance(entry, dict):
                continue
            path = str(entry.get("path") or "")
            name = str(entry.get("name") or "")
            path_key = path.lower()
            if not path or not name or path_key in seen_paths:
                continue
            seen_paths.add(path_key)
            artist, title = track_name(name)
            tracks.append(
                {
                    "id": f"{grant_id}:{path_key}",
                    "libraryId": grant_id,
                    "path": path,
                    "name": name,
                    "title": title,
                    "artist": artist,
                    "extension": str(entry.get("extension") or extension).lower(),
                    "sizeBytes": int(entry.get("sizeBytes") or 0),
                }
            )
            if len(tracks) >= MAX_TRACKS_PER_LIBRARY:
                break
        if len(tracks) >= MAX_TRACKS_PER_LIBRARY:
            break

    updated = dict(library)
    updated["tracks"] = sorted(tracks, key=lambda item: (item["artist"].lower(), item["title"].lower()))
    updated["trackCount"] = len(updated["tracks"])
    updated["updatedAt"] = int(time() * 1000)
    updated["truncated"] = len(tracks) >= MAX_TRACKS_PER_LIBRARY
    return updated


def choose_library() -> dict[str, Any]:
    grant_response = call_file_operation(
        "external.grant-directory",
        {
            "title": "选择包含音乐文件的文件夹",
            "access": "read",
            "lifetime": "persistent",
        },
        capability="filesystem.dialog.open",
        timeout_ms=120_000,
    )
    grant = operation_output(grant_response)
    grant_id = str(grant.get("id") or "")
    root_path = str(grant.get("rootPath") or "")
    if not grant_id or not root_path:
        raise ValueError("Nexora 未返回有效的目录授权")

    libraries = [item for item in stored_libraries() if item.get("grantId") != grant_id]
    source = {
        "id": grant_id,
        "grantId": grant_id,
        "rootPath": root_path,
        "name": PureWindowsPath(root_path).name or root_path,
    }
    source["tracks"] = []
    source["trackCount"] = 0
    source["updatedAt"] = int(time() * 1000)
    libraries.append(source)
    save_libraries(libraries)
    publish(
        "library-state",
        {
            "libraries": libraries,
            "refreshGrantId": grant_id,
            "message": "已添加音乐文件夹，正在读取曲目...",
        },
    )
    return {"library": source, "libraries": libraries}


def refresh_libraries(context: Any, grant_id: str | None) -> dict[str, Any]:
    refreshed: list[dict[str, Any]] = []
    warnings: list[str] = []
    for library in stored_libraries():
        if grant_id and library.get("grantId") != grant_id:
            refreshed.append(library)
            continue
        try:
            refreshed.append(scan_library(library, context))
        except NexoraBridgeError as error:
            log(f"scan failed for {library.get('rootPath')}: {error.code} {error}")
            warnings.append(f"{library.get('name') or library.get('rootPath')}: {error}")
            refreshed.append(library)
    save_libraries(refreshed)
    publish("library-state", {"libraries": refreshed, "warnings": warnings, "message": "音乐目录已刷新"})
    return {"libraries": refreshed, "warnings": warnings}


def remove_library(grant_id: str) -> dict[str, Any]:
    libraries = stored_libraries()
    if not any(item.get("grantId") == grant_id for item in libraries):
        raise ValueError("未找到要移除的音乐目录")
    call_file_operation(
        "external.revoke-grant",
        {"grantId": grant_id},
        capability="filesystem.external.read",
    )
    libraries = [item for item in libraries if item.get("grantId") != grant_id]
    save_libraries(libraries)
    publish("library-state", {"libraries": libraries, "message": "已移除音乐文件夹并撤销授权"})
    return {"libraries": libraries}


def load_track(track_id: str, context: Any) -> dict[str, Any]:
    selected: dict[str, Any] | None = None
    source: dict[str, Any] | None = None
    for library in stored_libraries():
        for track in library.get("tracks", []):
            if isinstance(track, dict) and track.get("id") == track_id:
                selected, source = track, library
                break
        if selected:
            break
    if not selected or not source:
        raise ValueError("音乐文件不在当前已授权目录中")
    size = int(selected.get("sizeBytes") or 0)
    if size > MAX_INLINE_AUDIO_BYTES:
        raise ValueError(
            f"此示例当前只能在隔离页面中播放不超过 {MAX_INLINE_AUDIO_BYTES // 1024 // 1024} MB 的单曲；"
            "更大的媒体文件需要后续的原生媒体流桥。"
        )
    opened = operation_output(call_file_operation(
        "stream.open-read",
        {"location": external_location(str(selected["path"]), str(source["grantId"]))},
        capability="filesystem.external.read",
        timeout_ms=60_000,
    ))
    stream_id = str(opened.get("streamId") or "")
    actual_size = int(opened.get("sizeBytes") or size)
    if not stream_id:
        raise ValueError("Nexora 未返回有效的音频读取流")
    if actual_size > MAX_INLINE_AUDIO_BYTES:
        # Abort the handle before returning so it cannot linger until its TTL.
        call_file_operation("stream.abort", {"streamId": stream_id}, capability="filesystem.external.read")
        raise ValueError(
            f"此示例当前只能在隔离页面中播放不超过 {MAX_INLINE_AUDIO_BYTES // 1024 // 1024} MB 的单曲；"
            "更大的媒体文件需要后续的原生媒体流桥。"
        )

    bytes_read = 0
    publish(
        "track-content-start",
        {
            "trackId": track_id,
            "sizeBytes": actual_size,
            "extension": selected.get("extension"),
            "name": selected.get("name"),
        },
    )
    try:
        while True:
            raise_if_cancelled(context)
            page = operation_output(call_file_operation(
                "stream.read",
                {"streamId": stream_id, "length": STREAM_CHUNK_BYTES},
                capability="filesystem.external.read",
                timeout_ms=60_000,
            ))
            data_base64 = page.get("dataBase64")
            if not isinstance(data_base64, str):
                raise ValueError("Nexora 返回了无效的音频流分块")
            chunk_bytes = int(page.get("bytesRead") or 0)
            bytes_read += chunk_bytes
            publish(
                "track-content-chunk",
                {
                    "trackId": track_id,
                    "dataBase64": data_base64,
                    "bytesRead": chunk_bytes,
                    "offset": int(page.get("offset") or bytes_read),
                },
            )
            if page.get("eof", False):
                break
        publish("track-content-complete", {"trackId": track_id, "bytesRead": bytes_read})
        return {"trackId": track_id, "bytesRead": bytes_read}
    finally:
        # stream.abort is also the read-stream close operation. Never let a
        # close error mask an earlier read or cancellation failure.
        try:
            call_file_operation("stream.abort", {"streamId": stream_id}, capability="filesystem.external.read")
        except NexoraBridgeError as error:
            log(f"close stream {stream_id} failed: {error.code} {error}")


def main() -> None:
    request = read_request()
    try:
        if request.command == "restore-libraries":
            libraries = stored_libraries()
            publish("library-state", {"libraries": libraries})
            write_result({"libraries": libraries})
        elif request.command == "add-library-folder":
            write_result(choose_library())
        elif request.command == "refresh-libraries":
            grant_id = request.payload.get("grantId") if isinstance(request.payload, dict) else None
            write_result(refresh_libraries(request.context, str(grant_id) if grant_id else None))
        elif request.command == "remove-library-folder":
            grant_id = request.payload.get("grantId") if isinstance(request.payload, dict) else None
            if not isinstance(grant_id, str) or not grant_id:
                raise ValueError("缺少 grantId")
            write_result(remove_library(grant_id))
        elif request.command == "load-library-track":
            track_id = request.payload.get("trackId") if isinstance(request.payload, dict) else None
            if not isinstance(track_id, str) or not track_id:
                raise ValueError("缺少 trackId")
            write_result(load_track(track_id, request.context))
        else:
            write_error(f"unsupported command: {request.command}")
    except (NexoraBridgeError, InterruptedError, ValueError) as error:
        log(f"{request.command} failed: {error}")
        write_error(error)


if __name__ == "__main__":
    main()
