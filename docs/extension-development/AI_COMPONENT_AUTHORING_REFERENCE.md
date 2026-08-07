# Nexora AI Component Authoring Contract

目标：把本文直接作为开发者 AI 的上下文，用于生成只依赖 Nexora 安装版公开合同的组件。

## 0. Mandatory Version Rule

- Default target: `stable-2.8.5` only.
- Do not use `experimental`, `planned-r14.5` or `reserved-r17` unless the user explicitly names that status and target version.
- If a non-stable interface is requested, start the output with `Required Nexora interface status: <status>` and provide a stable fallback.
- Never invent Tauri commands, React components, Zustand/Redux stores, DOM hooks, SQLite tables, TreeCache functions or internal filesystem paths.

## 1. Product Vocabulary

Use only:

- 组件: installable extension unit.
- 方案: versioned assembly selecting components, UI and bindings.
- 项目: user business directory managed by Nexora.

`Module`, `enabledModules` and `moduleSettings` are legacy read-only compatibility concepts. Never generate a new Module Manifest.

## 2. Stable Manifest Template

```json
{
  "schemaVersion": 1,
  "id": "com.example.component",
  "name": "Example Component",
  "description": "One clear sentence.",
  "version": "0.1.0",
  "apiVersion": "1",
  "runtime": "python-action",
  "role": "feature",
  "category": "automation",
  "tags": ["example"],
  "distribution": "local",
  "uiMode": "headless",
  "platforms": ["windows-x64"],
  "entry": "main.py",
  "capabilities": [],
  "requiresComponents": [],
  "optionalComponents": [],
  "contributes": {"automationCommands": []},
  "resources": {"maxMemoryMb": 256, "maxParallelism": 1, "timeoutMs": 120000},
  "publisher": "Developer Name"
}
```

Allowed categories: `workspace`, `file-handler`, `service`, `automation`, `appearance`, `integration`, `data`.

## 3. Stable Command Template

```json
{
  "id": "com.example.component.command",
  "command": "command",
  "name": "Command",
  "description": "What it does.",
  "contextRequirement": "project-required",
  "executionSemantics": "pure",
  "requiredCapability": "project.files.read",
  "capabilityOperation": "read",
  "inputSchema": {"type": "object"},
  "outputSchema": {"type": "object"},
  "maxAttempts": 1,
  "maxParallelism": 1,
  "timeoutMs": 30000
}
```

Rules:

- One command has at most one `requiredCapability` in 2.8.5. Split workflows into multiple commands when permissions differ.
- `pure` may be retried safely; `idempotent` requires a stable idempotency strategy; `non-idempotent` must not be automatically replayed.
- Use JSON Schema for every public input and output.
- Only commands listed in `automationCommands` are callable through Component Bridge.

## 4. Stable Python SDK Signatures

```python
read_request() -> Request
log(message) -> None
progress(value: float, message="") -> None
is_cancelled(context: Context) -> bool
raise_if_cancelled(context: Context) -> None
write_result(value) -> None
write_error(message) -> None

call_component(component_id, command, payload=None, *, capability=None, timeout_ms=None)
get_state(key, default=None, *, scope="global")
set_state(key, value, *, scope="global")
delete_state(key, *, scope="global")
emit_surface_event(surface_id, event, payload=None)

get_project_context() -> dict
list_project_files(relative_path="", *, cursor=0, limit=100) -> dict
stat_project_file(relative_path) -> dict
resolve_project_file(relative_path, *, access="read") -> dict
mutate_project_files(mutations: list[dict]) -> dict
get_project_metadata(key, default=None)
set_project_metadata(key, value)

put_blob(name, data, *, scope="global", kind="state") -> dict
open_blob(name, *, scope="global", kind="state") -> dict
list_blobs(*, scope="global", kind="state") -> dict
delete_blob(name, *, scope="global", kind="state") -> dict
get_storage_directory(*, scope="global", kind="state") -> dict
```

Always call `read_request()` before any Bridge function. Always handle `NexoraBridgeError` and cancellation.

## 5. Capability Matrix

| Operation | Required stable Capability |
| --- | --- |
| List/stat/resolve project files for read | `project.files.read` |
| Create/copy/move/rename/delete project files | `project.files.write` |
| Read/write component project metadata | `project.metadata.read` / `project.metadata.write` |
| Read/list or write/delete Blob | `project.storage.read` / `project.storage.write` |
| Obtain component-owned physical directory | `project.storage.direct` |
| Call BlenderIO `inspect` | `project.files.read` |
| Read arbitrary user-selected external file | `filesystem.external.read` |
| Start a process | `process.spawn` |
| HTTP request | `network.http.request` |

Do not request a broad Capability for convenience.

## 6. Project And Storage Rules

- Project paths passed to project APIs are relative.
- Reject absolute paths and `..`; never access `.pm_center` directly.
- Use `mutate_project_files()` so Nexora can process conflicts, Watcher and TreeCache updates.
- Use `state` for durable user data and `cache` for rebuildable data.
- Blob Bridge payload is limited to 512 KiB per write.
- Only use `get_storage_directory()` after declaring `project.storage.direct`.
- Never concatenate another component ID into a storage path.

## 7. Component Bridge Rules

Before generating `call_component()`:

1. Add the target to `requiresComponents` or `optionalComponents` with a SemVer requirement.
2. Confirm the target command is documented as public.
3. Pass the exact Capability required by the target command.
4. Handle `component_not_installed`, `dependency_not_active`, `dependency_version_mismatch`, `component_command_not_exported`, `capability_required`, timeout and cancellation errors.

Stable BlenderIO call:

```json
{"id":"pmc.blendio","versionRequirement":"^1.0"}
```

```python
resolved = resolve_project_file("scene.blend")
result = call_component(
    "pmc.blendio",
    "inspect",
    {"path": resolved["path"]},
    capability="project.files.read",
    timeout_ms=120000,
)
```

No other BlenderIO command is stable unless added to the human interface reference.

## 8. UI And Runtime Matrix

| Need | Stable choice |
| --- | --- |
| Headless one-shot task | `python-action` |
| Long-lived Python service | `python-worker` |
| Existing CLI or isolated native service | `native-process` |
| Performance DLL | `native-library` through isolated component host |
| Custom HTML/CSS/JS page | `scriptSurfaces` sandbox |
| Open a file extension | `fileHandlers` |
| User-editable settings | `settingsSections` |

Sandbox JavaScript cannot access host DOM, Tauri IPC, local files or undeclared network resources. Hosted React surfaces and project-manager-level ABI are `reserved-r17`.

## 9. Forbidden Output

Reject or rewrite any design that:

- imports Nexora internal TypeScript/Rust modules;
- invokes a Tauri command by string;
- reads a Nexora SQLite database;
- modifies TreeCache or Watcher state directly;
- hardcodes app-data, install, `.pm_center/components` or another component path;
- generates `enabledModules` for a new component;
- claims Python is an OS sandbox;
- claims a planned or reserved API is available in 2.8.5.

## 10. Complete Generation Flow

1. Restate target Nexora status and runtime.
2. Choose a stable component ID and SemVer.
3. List commands and assign one Capability to each.
4. Declare dependencies and compatible versions.
5. Write `component.json` with input/output Schema.
6. Write the runtime entry using only SDK calls above.
7. Add cancellation, structured errors and deterministic output.
8. Add README with installation, trust, test and uninstall behavior.
9. Validate in Nexora developer workbench.
10. Trust the development directory, run each command, test missing dependency and denied permission.
11. Generate a signing key outside the component directory and create `.pmc-pack`.
12. Test the package on a clean Windows user/device.

## 11. Self-check

- [ ] Uses only `stable-2.8.5` unless an explicit target says otherwise.
- [ ] Contains no Module Manifest or internal API.
- [ ] Every command has Schema, semantics, timeout and minimal Capability.
- [ ] Every component call has a declared dependency.
- [ ] Project files are accessed through SDK paths and mutations.
- [ ] Durable data and cache are separated.
- [ ] Errors, cancellation and missing project context are handled.
- [ ] No credentials, absolute paths or signing keys are packaged.
- [ ] README explains trust and Python's Windows-user authority.
