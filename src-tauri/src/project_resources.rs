use crate::db::Database;
use crate::{cache_manager, tree_cache, watcher};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

pub const PROJECT_RESOURCES_MODULE_ID: &str = "builtin.project-resources";
pub const PROJECT_RESOURCES_DISABLED_CODE: &str = "PROJECT_RESOURCES_MODULE_DISABLED";
pub const PROJECT_NOT_OPEN_CODE: &str = "PROJECT_RESOURCES_PROJECT_NOT_OPEN";

#[derive(Debug, Clone)]
pub struct ProjectRegistration {
    pub project_path: String,
    pub exclude_patterns: Vec<String>,
}

#[derive(Default)]
struct ProjectLifecycleRegistry {
    enforced: bool,
    enabled: bool,
    restore_pending: bool,
    projects: HashMap<String, ProjectRegistration>,
    active_project_key: Option<String>,
}

impl ProjectLifecycleRegistry {
    fn initialize(&mut self) {
        self.enforced = true;
        self.enabled = false;
        self.restore_pending = true;
        self.projects.clear();
        self.active_project_key = None;
    }

    fn ensure_module_enabled(&self) -> Result<(), String> {
        if !self.enforced || self.enabled {
            Ok(())
        } else if self.restore_pending {
            Err("PROJECT_RESOURCES_MODULE_STARTING: 项目资源模块正在启动".into())
        } else {
            Err(format!(
                "{PROJECT_RESOURCES_DISABLED_CODE}: 项目资源模块已停用，请先在后台模块中重新启用"
            ))
        }
    }

    fn ensure_project_access(&self, project_path: &str) -> Result<(), String> {
        self.ensure_module_enabled()?;
        if !self.enforced {
            return Ok(());
        }
        let project_key = tree_cache::normalize_path_key(project_path);
        if self.projects.contains_key(&project_key) {
            Ok(())
        } else {
            Err(format!("{PROJECT_NOT_OPEN_CODE}: 项目未打开或资源已经释放"))
        }
    }

    fn register_project(
        &mut self,
        project_path: &str,
        exclude_patterns: &[String],
    ) -> Result<bool, String> {
        self.ensure_module_enabled()?;
        let project_key = tree_cache::normalize_path_key(project_path);
        let existed = self.projects.contains_key(&project_key);
        self.projects.insert(
            project_key,
            ProjectRegistration {
                project_path: project_path.to_string(),
                exclude_patterns: exclude_patterns.to_vec(),
            },
        );
        Ok(existed)
    }

    fn unregister_project(&mut self, project_path: &str) -> bool {
        let project_key = tree_cache::normalize_path_key(project_path);
        if self.active_project_key.as_deref() == Some(project_key.as_str()) {
            self.active_project_key = None;
        }
        self.projects.remove(&project_key).is_some()
    }

    fn mark_active(&mut self, project_path: &str) -> Result<(), String> {
        self.ensure_project_access(project_path)?;
        self.active_project_key = Some(tree_cache::normalize_path_key(project_path));
        Ok(())
    }

    fn clear_active(&mut self) {
        self.active_project_key = None;
    }

    fn registrations(&self) -> Vec<ProjectRegistration> {
        self.projects.values().cloned().collect()
    }

    fn active_registration(&self) -> Option<ProjectRegistration> {
        self.active_project_key
            .as_ref()
            .and_then(|key| self.projects.get(key))
            .cloned()
    }
}

lazy_static::lazy_static! {
    static ref PROJECT_LIFECYCLE: StdMutex<ProjectLifecycleRegistry> =
        StdMutex::new(ProjectLifecycleRegistry::default());
}

#[derive(Default)]
pub struct ProjectDatabaseStateInner {
    databases: HashMap<String, Database>,
}

pub type ProjectDatabaseState = Arc<Mutex<ProjectDatabaseStateInner>>;

#[derive(Debug, Clone)]
pub struct ProjectResourceHealthSnapshot {
    pub registered_projects: usize,
    pub database_count: usize,
    pub tree_cache_count: usize,
    pub active_project: Option<String>,
    pub watcher_running: bool,
    pub watcher_worker_running: bool,
    pub active_maintenance: usize,
}

pub fn new_database_state() -> ProjectDatabaseState {
    Arc::new(Mutex::new(ProjectDatabaseStateInner::default()))
}

pub fn initialize_lifecycle_control() {
    if let Ok(mut registry) = PROJECT_LIFECYCLE.lock() {
        registry.initialize();
    }
}

pub fn set_module_enabled(enabled: bool) {
    if let Ok(mut registry) = PROJECT_LIFECYCLE.lock() {
        registry.enabled = enabled;
        registry.restore_pending = false;
    }
}

pub fn set_initial_desired_enabled(desired_enabled: bool) {
    if let Ok(mut registry) = PROJECT_LIFECYCLE.lock() {
        registry.enabled = false;
        registry.restore_pending = desired_enabled;
    }
}

pub fn is_module_enabled() -> bool {
    PROJECT_LIFECYCLE
        .lock()
        .map(|registry| !registry.enforced || registry.enabled)
        .unwrap_or(false)
}

pub fn ensure_module_enabled() -> Result<(), String> {
    PROJECT_LIFECYCLE
        .lock()
        .map_err(|error| error.to_string())?
        .ensure_module_enabled()
}

pub async fn wait_for_module_enabled(timeout: std::time::Duration) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let (enabled, pending) = PROJECT_LIFECYCLE
            .lock()
            .map(|registry| {
                (
                    !registry.enforced || registry.enabled,
                    registry.enforced && registry.restore_pending,
                )
            })
            .map_err(|error| error.to_string())?;
        if enabled {
            return Ok(());
        }
        if !pending || tokio::time::Instant::now() >= deadline {
            return ensure_module_enabled();
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

pub fn ensure_project_access(project_path: &str) -> Result<(), String> {
    PROJECT_LIFECYCLE
        .lock()
        .map_err(|error| error.to_string())?
        .ensure_project_access(project_path)
}

pub fn register_project(project_path: &str, exclude_patterns: &[String]) -> Result<bool, String> {
    PROJECT_LIFECYCLE
        .lock()
        .map_err(|error| error.to_string())?
        .register_project(project_path, exclude_patterns)
}

pub fn unregister_project(project_path: &str) -> bool {
    PROJECT_LIFECYCLE
        .lock()
        .map(|mut registry| registry.unregister_project(project_path))
        .unwrap_or(false)
}

pub fn mark_active_project(project_path: &str) -> Result<(), String> {
    PROJECT_LIFECYCLE
        .lock()
        .map_err(|error| error.to_string())?
        .mark_active(project_path)
}

pub fn clear_active_project() {
    if let Ok(mut registry) = PROJECT_LIFECYCLE.lock() {
        registry.clear_active();
    }
}

pub fn registrations() -> Vec<ProjectRegistration> {
    PROJECT_LIFECYCLE
        .lock()
        .map(|registry| registry.registrations())
        .unwrap_or_default()
}

pub fn active_registration() -> Option<ProjectRegistration> {
    PROJECT_LIFECYCLE
        .lock()
        .ok()
        .and_then(|registry| registry.active_registration())
}

pub fn registration_count() -> usize {
    PROJECT_LIFECYCLE
        .lock()
        .map(|registry| registry.projects.len())
        .unwrap_or_default()
}

pub async fn get_or_create_database(
    state: &ProjectDatabaseState,
    project_path: &str,
) -> Result<Database, String> {
    ensure_project_access(project_path)?;
    let project_key = tree_cache::normalize_path_key(project_path);
    let mut state = state.lock().await;
    ensure_project_access(project_path)?;
    if let Some(database) = state.databases.get(&project_key) {
        return Ok(database.clone());
    }

    let database = Database::new(project_path).map_err(|error| error.to_string())?;
    state.databases.insert(project_key, database.clone());
    Ok(database)
}

pub async fn release_project_database(state: &ProjectDatabaseState, project_path: &str) -> bool {
    let project_key = tree_cache::normalize_path_key(project_path);
    state.lock().await.databases.remove(&project_key).is_some()
}

pub async fn release_all_databases(state: &ProjectDatabaseState) -> usize {
    let mut state = state.lock().await;
    let count = state.databases.len();
    state.databases.clear();
    count
}

pub async fn database_count(state: &ProjectDatabaseState) -> usize {
    state.lock().await.databases.len()
}

pub async fn release_project_handles(
    state: &ProjectDatabaseState,
    project_path: &str,
) -> Result<(), String> {
    watcher::clear_active_project_if_matches(project_path);
    cache_manager::cancel_project_cache_maintenance(project_path)?;
    let maintenance_idle = cache_manager::wait_for_project_cache_maintenance(
        project_path,
        std::time::Duration::from_secs(5),
    )
    .await;
    tree_cache::mark_project_tree_dirty(project_path);
    release_project_database(state, project_path).await;
    tree_cache::release_project_cache(project_path)?;
    if maintenance_idle {
        Ok(())
    } else {
        Err("等待项目缓存维护任务停止超时，数据库连接已释放".into())
    }
}

pub async fn release_all_handles(state: &ProjectDatabaseState) -> Result<(), String> {
    watcher::stop_active_project();
    cache_manager::cancel_all_cache_maintenance()?;
    let maintenance_idle =
        cache_manager::wait_for_all_cache_maintenance(std::time::Duration::from_secs(7)).await;
    tree_cache::mark_registered_trees_dirty();
    release_all_databases(state).await;
    tree_cache::release_all_project_caches()?;
    if maintenance_idle {
        Ok(())
    } else {
        Err("等待缓存维护任务停止超时，项目数据库与索引连接已释放".into())
    }
}

pub async fn restore_registered_handles(state: &ProjectDatabaseState) -> Vec<String> {
    let mut errors = Vec::new();
    for registration in registrations() {
        if !Path::new(&registration.project_path).is_dir() {
            errors.push(format!("项目目录不存在：{}", registration.project_path));
            continue;
        }
        if let Err(error) = get_or_create_database(state, &registration.project_path).await {
            errors.push(format!(
                "恢复项目数据库失败：{}: {error}",
                registration.project_path
            ));
            continue;
        }
        if let Err(error) = tree_cache::get_or_create_project_cache(&registration.project_path) {
            errors.push(format!(
                "恢复目录索引失败：{}: {error}",
                registration.project_path
            ));
        }
    }

    if let Some(active) = active_registration() {
        if let Err(error) = watcher::set_active_project(
            &active.project_path,
            &match get_or_create_database(state, &active.project_path).await {
                Ok(database) => database,
                Err(error) => {
                    errors.push(format!(
                        "恢复活动项目数据库失败：{}: {error}",
                        active.project_path
                    ));
                    return errors;
                }
            },
            &active.exclude_patterns,
        ) {
            errors.push(format!(
                "恢复项目监听失败：{}: {error}",
                active.project_path
            ));
        }
    }

    errors
}

pub async fn health_snapshot(state: &ProjectDatabaseState) -> ProjectResourceHealthSnapshot {
    let watcher = watcher::runtime_status();
    ProjectResourceHealthSnapshot {
        registered_projects: registration_count(),
        database_count: database_count(state).await,
        tree_cache_count: tree_cache::project_cache_count(),
        active_project: watcher.active_project,
        watcher_running: watcher.watcher_running,
        watcher_worker_running: watcher.worker_running,
        active_maintenance: cache_manager::active_cache_maintenance_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn registry_preserves_projects_while_the_module_is_disabled() {
        let mut registry = ProjectLifecycleRegistry {
            enforced: true,
            enabled: true,
            restore_pending: false,
            ..ProjectLifecycleRegistry::default()
        };
        let project_path = if cfg!(windows) {
            "C:\\Project\\Demo"
        } else {
            "/project/demo"
        };
        assert!(!registry
            .register_project(project_path, &["cache/**".into()])
            .unwrap());
        registry.mark_active(project_path).unwrap();

        registry.enabled = false;
        assert!(registry.ensure_project_access(project_path).is_err());
        assert_eq!(registry.registrations().len(), 1);
        assert!(registry.active_registration().is_some());

        registry.enabled = true;
        assert!(registry.ensure_project_access(project_path).is_ok());
        assert!(registry.unregister_project(project_path));
        assert!(registry.active_registration().is_none());
    }

    #[tokio::test]
    async fn closing_a_project_releases_database_cache_and_watcher_without_deleting_data() {
        let root =
            std::env::temp_dir().join(format!("pm-center-project-resources-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join(".pm_center")).unwrap();
        let marker = root.join(".pm_center").join("keep-me.txt");
        fs::write(&marker, b"preserved").unwrap();
        let project_path = root.to_string_lossy().to_string();
        let state = new_database_state();

        register_project(&project_path, &[]).unwrap();
        let database = get_or_create_database(&state, &project_path).await.unwrap();
        let cache = tree_cache::get_or_create_project_cache(&project_path).unwrap();
        watcher::set_active_project(&project_path, &database, &[]).unwrap();
        mark_active_project(&project_path).unwrap();
        drop(cache);
        drop(database);

        assert_eq!(database_count(&state).await, 1);
        assert!(tree_cache::has_project_cache(&project_path));
        assert_eq!(
            watcher::get_active_project_path().as_deref(),
            Some(project_path.as_str())
        );

        unregister_project(&project_path);
        release_project_handles(&state, &project_path)
            .await
            .unwrap();

        assert_eq!(database_count(&state).await, 0);
        assert!(!tree_cache::has_project_cache(&project_path));
        assert!(watcher::get_active_project_path().is_none());
        assert_eq!(fs::read(&marker).unwrap(), b"preserved");

        fs::remove_dir_all(root).unwrap();
    }
}
