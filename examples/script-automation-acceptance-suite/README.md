# Script Automation Acceptance Suite

This is a development and release-acceptance fixture for R11. It is not installed,
trusted, or enabled by default. It uses no capabilities and does not access project
files, network resources, or external programs.

## Commands

- `global-probe`: verifies that a global command receives no project context and can
  use the nonce-bound SDK state and surface bridge.
- `either-probe`: reports whether Nexora supplied a project context.
- `long-probe`: checks the cancellation marker once per second. Cancel it from Task
  Center and verify that the run becomes `cancelled` with no remaining Python process.
- `retry-probe`: intentionally fails. The idempotent command must create a second
  attempt after the normal five-second backoff, then end in `failed`.
- `non-idempotent-probe`: runs for a bounded interval. Use only in a disposable test
  data directory when validating abnormal-exit recovery: a forced app termination
  while it is running must produce `attention` after restart, never an automatic replay.

## Install And Run

1. In Script Developer Workbench, validate this directory, explicitly trust it, install
   it, and add it to a disposable Profile.
2. Run `global-probe` and `either-probe` from the Workbench or Task Center.
3. Open the contributed surface. Its buttons use the sandboxed `window.nexora` bridge;
   no Tauri API is exposed to the page.
4. Bind `file.changed` with a stable source dedupe key, emit it twice, and confirm that
   only one run is created. A new key must create a new run.
5. Package it using a disposable Ed25519 signing key, then inspect and install the
   resulting `.pmc-pack` after trusting the test publisher. Do not add keys or output
   packages to this repository.

## Command-Line Package Check

```powershell
$out = Join-Path $env:TEMP "nexora-r11-acceptance"
New-Item -ItemType Directory -Force $out | Out-Null
cargo run --manifest-path src-tauri/Cargo.toml --bin nexora-component-pack -- keygen "$out\test-publisher.json"
cargo run --manifest-path src-tauri/Cargo.toml --bin nexora-component-pack -- pack examples/script-automation-acceptance-suite "$out\automation-acceptance.pmc-pack" --key "$out\test-publisher.json" --publisher-id nexora.r11.acceptance --publisher-name "Nexora R11 Acceptance"
```

The package should be reported as signed but untrusted until the disposable publisher
is explicitly trusted. This is expected and verifies the distribution boundary.
