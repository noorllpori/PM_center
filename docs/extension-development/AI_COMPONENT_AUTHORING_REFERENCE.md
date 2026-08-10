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
  "requiredCapabilities": ["project.files.read"],
  "capabilityOperation": "read",
  "inputSchema": {"type": "object"},
  "outputSchema": {"type": "object"},
  "maxAttempts": 1,
  "maxParallelism": 1,
  "timeoutMs": 30000
}
```

Rules:

- Use `requiredCapabilities` for every Capability the command can require. `requiredCapability` is accepted only as a compatibility alias for a one-item list.
- Nexora requests only the Capability entries actually needed by the concrete file-operation source, target and action. Do not omit a possible Capability from the Manifest merely because a particular run might not use it.
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

call_component(component_id, command, payload=None, *, capability=None, capabilities=None, timeout_ms=None)
project_location(path, *, project_id=None) -> dict
external_location(path, grant_id) -> dict
call_file_operation(command, payload=None, *, capability=None, capabilities=None, timeout_ms=None)
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

For project-to-external export, pass both `project.files.read` and `filesystem.external.write`. A component must first request a user-selected directory through `external.select` or `external.grant-directory`; it may only use the returned `grantId` for its own external locations.

Do not request a broad Capability for convenience.

When Nexora presents a Capability request, the user may choose `allowOnce`,
`allowSession`, `allowAlways`, or deny it. A command that needs multiple
Capabilities waits for each required approval in the same run. Components must
handle a denial or cancellation as a normal outcome; they must not retry a
non-idempotent operation solely to ask again.

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
3. Pass `capability` for one permission, or `capabilities` for every permission the concrete target call needs.
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

### File Operations With Multiple Capabilities

Use the structured `FileLocation` payload. Project paths stay relative; an external
path must include a grant returned by `external.select` or
`external.grant-directory`. The following exports a project file to an already
authorized external directory. The explicit list is required because one operation
reads the project and writes outside it.

```python
result = call_file_operation(
    "external.export",
    {
        "source": {"space": "project", "path": "assets/shot.png"},
        "target": {
            "space": "external",
            "grantId": selected_directory["grantId"],
            "path": r"D:\\Media\\shot.png",
        },
        "conflict": "rename",
    },
    capabilities=["project.files.read", "filesystem.external.write"],
)
```

Do not request broad external access pre-emptively. Ask for the directory through
`external.select`, then pass only the concrete capabilities needed by that command.

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
| Insert an isolated page into a supported host location | `uiExtensions` plus a `scriptSurfaces` entry and a dependency on that host |
| Replace a supported workspace page | `uiExtensions` with `mode: "replace"` on a declared surface point |

Sandbox JavaScript cannot access host DOM, Tauri IPC, local files or undeclared network resources. Hosted React surfaces and project-manager-level ABI are `reserved-r17`.

### UI Extension Contract

`nexora.project-manager` publishes stable points for `project-toolbar`, `project-sidebar`, `file-details`, `file-context-menu`, `project-home-widgets`, `project-status-bar`, and `project-workspace`. The component that contributes an extension must declare `nexora.project-manager` in `requiresComponents` or `optionalComponents`, own the referenced script surface, and use the point ID exactly as documented. The Profile controls enablement and order. A surface receives only project context, relative selection, theme, language and dimensions through the nonce bridge; it cannot receive a Store, DOM element or Tauri API.

```json
{
  "requiresComponents": [{"id": "nexora.project-manager", "versionRequirement": "^1.0"}],
  "contributes": {
    "scriptSurfaces": [{"id": "com.example.note.panel", "name": "Note panel", "entry": "ui/index.html", "allowedCommands": ["save-note"]}],
    "uiExtensions": [{
      "id": "com.example.note.file-details",
      "targetComponentId": "nexora.project-manager",
      "targetPointId": "nexora.project-manager.file-details",
      "surfaceId": "com.example.note.panel",
      "mode": "insert",
      "order": 100
    }]
  }
}
```

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
3. List commands and declare every Capability each command may require.
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
- [ ] Cross-space file calls use `FileLocation`, user-selected `grantId`, and the complete `capabilities` list.
- [ ] UI extensions name a stable target point, own their surface and have a target component dependency.
- [ ] Every component call has a declared dependency.
- [ ] Project files are accessed through SDK paths and mutations.
- [ ] Durable data and cache are separated.
- [ ] Errors, cancellation and missing project context are handled.
- [ ] No credentials, absolute paths or signing keys are packaged.
- [ ] README explains trust and Python's Windows-user authority.
