use super::capability_gateway::CapabilityComponentRegistration;
use super::module_manager::{
    LifecycleFuture, ModuleContext, ModuleHealth, ModuleHealthLevel, ModuleLifecycle,
    RegisteredModule,
};
use super::resource_registry::ResourceKind;
use pmc_platform::{
    Capability, ExtensionFields, ModuleContributions, ModuleDataPolicy, ModuleDependency,
    ModuleManifestV1, ModuleScope,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const DIAGNOSTIC_BASE_ID: &str = "diagnostic.runtime-base";
pub const DIAGNOSTIC_WORKER_ID: &str = "diagnostic.runtime-worker";
pub const DIAGNOSTIC_FAILING_ID: &str = "diagnostic.runtime-failing";
pub const DIAGNOSTIC_SLOW_STOP_ID: &str = "diagnostic.runtime-slow-stop";

#[derive(Default)]
pub struct DiagnosticControls {
    fail_start: AtomicBool,
    fail_health: AtomicBool,
    slow_stop: AtomicBool,
    active_tasks: AtomicUsize,
}

impl DiagnosticControls {
    pub fn configure(&self, module_id: &str, point: &str, enabled: bool) -> Result<(), String> {
        match (module_id, point) {
            (DIAGNOSTIC_FAILING_ID, "start") => {
                self.fail_start.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            (DIAGNOSTIC_FAILING_ID, "health") => {
                self.fail_health.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            (DIAGNOSTIC_SLOW_STOP_ID, "stop-timeout") => {
                self.slow_stop.store(enabled, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(format!("模块 {module_id} 不支持故障点 {point}")),
        }
    }

    pub fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::SeqCst)
    }

    pub fn failure_injections(&self) -> BTreeMap<String, bool> {
        BTreeMap::from([
            (
                format!("{DIAGNOSTIC_FAILING_ID}:start"),
                self.fail_start.load(Ordering::SeqCst),
            ),
            (
                format!("{DIAGNOSTIC_FAILING_ID}:health"),
                self.fail_health.load(Ordering::SeqCst),
            ),
            (
                format!("{DIAGNOSTIC_SLOW_STOP_ID}:stop-timeout"),
                self.slow_stop.load(Ordering::SeqCst),
            ),
        ])
    }
}

#[derive(Clone, Copy)]
enum DiagnosticKind {
    Base,
    Worker,
    Failing,
    SlowStop,
}

struct DiagnosticLifecycle {
    kind: DiagnosticKind,
    controls: Arc<DiagnosticControls>,
}

impl DiagnosticLifecycle {
    fn new(kind: DiagnosticKind, controls: Arc<DiagnosticControls>) -> Self {
        Self { kind, controls }
    }

    fn label(&self) -> &'static str {
        match self.kind {
            DiagnosticKind::Base => "基础心跳任务",
            DiagnosticKind::Worker => "依赖工作任务",
            DiagnosticKind::Failing => "故障注入任务",
            DiagnosticKind::SlowStop => "慢停止任务",
        }
    }
}

impl ModuleLifecycle for DiagnosticLifecycle {
    fn start<'a>(&'a self, context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::Failing)
                && self.controls.fail_start.load(Ordering::SeqCst)
            {
                return Err("已注入启动失败，用于验证事务回滚".into());
            }

            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
            let active_tasks = self.controls.clone();
            active_tasks.active_tasks.fetch_add(1, Ordering::SeqCst);
            let task = tokio::spawn(async move {
                let _ = stop_rx.await;
                active_tasks.active_tasks.fetch_sub(1, Ordering::SeqCst);
            });
            let mut details = BTreeMap::new();
            details.insert("runtime".into(), "tokio".into());
            details.insert("purpose".into(), "R2 lifecycle verification".into());
            context.resources.register(
                context.module_id,
                ResourceKind::TokioTask,
                self.label(),
                details,
                Box::new(move || {
                    Box::pin(async move {
                        let _ = stop_tx.send(());
                        task.await
                            .map_err(|error| format!("等待诊断任务退出失败: {error}"))?;
                        Ok(())
                    })
                }),
            );
            Ok(())
        })
    }

    fn stop<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ()> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::SlowStop)
                && self.controls.slow_stop.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
            Ok(())
        })
    }

    fn health<'a>(&'a self, _context: ModuleContext) -> LifecycleFuture<'a, ModuleHealth> {
        Box::pin(async move {
            if matches!(self.kind, DiagnosticKind::Failing)
                && self.controls.fail_health.load(Ordering::SeqCst)
            {
                return Ok(ModuleHealth {
                    level: ModuleHealthLevel::Unhealthy,
                    message: "已注入健康检查失败".into(),
                    checked_at: Some(chrono::Utc::now().timestamp_millis()),
                });
            }
            Ok(ModuleHealth::healthy("诊断资源运行正常"))
        })
    }

    fn start_timeout(&self) -> Duration {
        Duration::from_secs(2)
    }

    fn stop_timeout(&self) -> Duration {
        if matches!(self.kind, DiagnosticKind::SlowStop) {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(2)
        }
    }
}

pub fn diagnostic_modules(controls: Arc<DiagnosticControls>) -> Vec<RegisteredModule> {
    vec![
        diagnostic_module(
            DIAGNOSTIC_BASE_ID,
            "运行时基础模块",
            "用于验证基础启停和资源释放。",
            DiagnosticKind::Base,
            &[],
            &[],
            &[Capability::AppSettingsRead],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_WORKER_ID,
            "依赖工作模块",
            "依赖基础模块，用于验证拓扑启动和反向停止。",
            DiagnosticKind::Worker,
            &[(DIAGNOSTIC_BASE_ID, "*")],
            &[(DIAGNOSTIC_SLOW_STOP_ID, "*")],
            &[Capability::ProjectFilesRead],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_FAILING_ID,
            "故障注入模块",
            "可注入启动或健康检查失败，用于验证回滚。",
            DiagnosticKind::Failing,
            &[(DIAGNOSTIC_BASE_ID, "*")],
            &[],
            &[Capability::ProjectFilesWrite],
            controls.clone(),
        ),
        diagnostic_module(
            DIAGNOSTIC_SLOW_STOP_ID,
            "停止超时模块",
            "可注入停止超时，用于验证强制资源释放。",
            DiagnosticKind::SlowStop,
            &[],
            &[],
            &[Capability::FilesystemExternalWrite],
            controls,
        ),
    ]
}

fn diagnostic_module(
    id: &str,
    name: &str,
    description: &str,
    kind: DiagnosticKind,
    required: &[(&str, &str)],
    optional: &[(&str, &str)],
    capabilities: &[Capability],
    controls: Arc<DiagnosticControls>,
) -> RegisteredModule {
    let mut extensions = ExtensionFields::new();
    extensions.insert("diagnostic".into(), Value::Bool(true));
    RegisteredModule {
        manifest: ModuleManifestV1 {
            schema_version: 1,
            id: id.into(),
            name: name.into(),
            description: description.into(),
            version: "1.0.0".into(),
            api_version: "1".into(),
            scope: ModuleScope::Global,
            builtin: true,
            requires_modules: required
                .iter()
                .map(|(id, requirement)| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: (*requirement).into(),
                })
                .collect(),
            optional_modules: optional
                .iter()
                .map(|(id, requirement)| ModuleDependency {
                    id: (*id).into(),
                    version_requirement: (*requirement).into(),
                })
                .collect(),
            conflicts: Vec::new(),
            capabilities: capabilities.to_vec(),
            background_services: vec!["diagnostic-heartbeat".into()],
            contributes: ModuleContributions::default(),
            data_policy: ModuleDataPolicy::default(),
            extensions,
        },
        lifecycle: Arc::new(DiagnosticLifecycle::new(kind, controls)),
        diagnostic: true,
    }
}

pub fn diagnostic_components() -> Vec<CapabilityComponentRegistration> {
    vec![
        CapabilityComponentRegistration {
            id: "diagnostic.component-base".into(),
            name: "基础设置读取组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_BASE_ID.into(),
            capabilities: vec![Capability::AppSettingsRead],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-worker".into(),
            name: "项目只读诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_WORKER_ID.into(),
            capabilities: vec![Capability::ProjectFilesRead],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-failing".into(),
            name: "项目写入诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_FAILING_ID.into(),
            capabilities: vec![Capability::ProjectFilesWrite],
        },
        CapabilityComponentRegistration {
            id: "diagnostic.component-slow-stop".into(),
            name: "外部暂存诊断组件".into(),
            version: "1.0.0".into(),
            module_id: DIAGNOSTIC_SLOW_STOP_ID.into(),
            capabilities: vec![Capability::FilesystemExternalWrite],
        },
    ]
}
