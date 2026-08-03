use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

pub type ResourceCleanupFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'static>>;
pub type ResourceCleanup = Box<dyn FnOnce() -> ResourceCleanupFuture + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    TokioTask,
    NativeThread,
    ChildProcess,
    NetworkListener,
    Database,
    Watcher,
    GlobalShortcut,
    EventSubscription,
    TemporaryPath,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDiagnostic {
    pub id: String,
    pub module_id: String,
    pub kind: ResourceKind,
    pub label: String,
    pub created_at: i64,
    pub details: BTreeMap<String, String>,
    pub sequence: u64,
}

struct ResourceEntry {
    diagnostic: ResourceDiagnostic,
    cleanup: Option<ResourceCleanup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCleanupReport {
    pub released: usize,
    pub timed_out: Vec<String>,
    pub failures: Vec<ResourceCleanupFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCleanupFailure {
    pub resource_id: String,
    pub message: String,
}

#[derive(Default)]
pub struct ResourceRegistry {
    entries: Mutex<Vec<ResourceEntry>>,
    next_sequence: AtomicU64,
}

impl ResourceRegistry {
    pub fn register(
        &self,
        module_id: impl Into<String>,
        kind: ResourceKind,
        label: impl Into<String>,
        details: BTreeMap<String, String>,
        cleanup: ResourceCleanup,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let diagnostic = ResourceDiagnostic {
            id: id.clone(),
            module_id: module_id.into(),
            kind,
            label: label.into(),
            created_at: Utc::now().timestamp_millis(),
            details,
            sequence,
        };
        self.entries
            .lock()
            .expect("resource registry mutex poisoned")
            .push(ResourceEntry {
                diagnostic,
                cleanup: Some(cleanup),
            });
        id
    }

    pub fn diagnostics_for(&self, module_id: &str) -> Vec<ResourceDiagnostic> {
        let mut resources = self
            .entries
            .lock()
            .expect("resource registry mutex poisoned")
            .iter()
            .filter(|entry| entry.diagnostic.module_id == module_id)
            .map(|entry| entry.diagnostic.clone())
            .collect::<Vec<_>>();
        resources.sort_by_key(|resource| resource.sequence);
        resources
    }

    pub fn count_for(&self, module_id: &str) -> usize {
        self.entries
            .lock()
            .expect("resource registry mutex poisoned")
            .iter()
            .filter(|entry| entry.diagnostic.module_id == module_id)
            .count()
    }

    pub fn total_count(&self) -> usize {
        self.entries
            .lock()
            .expect("resource registry mutex poisoned")
            .len()
    }

    pub async fn cleanup_module(
        &self,
        module_id: &str,
        timeout: Duration,
    ) -> ResourceCleanupReport {
        let mut selected = {
            let mut entries = self
                .entries
                .lock()
                .expect("resource registry mutex poisoned");
            let mut selected = Vec::new();
            let mut retained = Vec::with_capacity(entries.len());
            for entry in entries.drain(..) {
                if entry.diagnostic.module_id == module_id {
                    selected.push(entry);
                } else {
                    retained.push(entry);
                }
            }
            *entries = retained;
            selected
        };

        selected.sort_by_key(|entry| std::cmp::Reverse(entry.diagnostic.sequence));
        self.cleanup_entries(selected, timeout).await
    }

    pub async fn cleanup_all(&self, timeout: Duration) -> ResourceCleanupReport {
        let mut selected = {
            let mut entries = self
                .entries
                .lock()
                .expect("resource registry mutex poisoned");
            std::mem::take(&mut *entries)
        };
        selected.sort_by_key(|entry| std::cmp::Reverse(entry.diagnostic.sequence));
        self.cleanup_entries(selected, timeout).await
    }

    async fn cleanup_entries(
        &self,
        entries: Vec<ResourceEntry>,
        timeout: Duration,
    ) -> ResourceCleanupReport {
        let mut report = ResourceCleanupReport::default();
        for mut entry in entries {
            let Some(cleanup) = entry.cleanup.take() else {
                continue;
            };
            match tokio::time::timeout(timeout, cleanup()).await {
                Ok(Ok(())) => report.released += 1,
                Ok(Err(message)) => report.failures.push(ResourceCleanupFailure {
                    resource_id: entry.diagnostic.id,
                    message,
                }),
                Err(_) => report.timed_out.push(entry.diagnostic.id),
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    #[tokio::test]
    async fn cleans_resources_in_reverse_registration_order() {
        let registry = ResourceRegistry::default();
        let order = Arc::new(StdMutex::new(Vec::new()));
        for value in [1, 2, 3] {
            let order = order.clone();
            registry.register(
                "test.module",
                ResourceKind::Other,
                format!("resource-{value}"),
                BTreeMap::new(),
                Box::new(move || {
                    Box::pin(async move {
                        order.lock().unwrap().push(value);
                        Ok(())
                    })
                }),
            );
        }

        let report = registry
            .cleanup_module("test.module", Duration::from_secs(1))
            .await;
        assert_eq!(report.released, 3);
        assert_eq!(*order.lock().unwrap(), vec![3, 2, 1]);
        assert_eq!(registry.total_count(), 0);
    }

    #[tokio::test]
    async fn reports_cleanup_timeout_without_leaving_registry_entries() {
        let registry = ResourceRegistry::default();
        registry.register(
            "test.module",
            ResourceKind::Other,
            "slow",
            BTreeMap::new(),
            Box::new(|| Box::pin(async { std::future::pending::<Result<(), String>>().await })),
        );

        let report = registry
            .cleanup_module("test.module", Duration::from_millis(10))
            .await;
        assert_eq!(report.timed_out.len(), 1);
        assert_eq!(registry.total_count(), 0);
    }
}
