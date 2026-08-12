use super::{ComponentRuntimeError, ComponentRuntimeErrorCode};
use pmc_platform::{
    parse_presentation_template, ComponentManifestV1, ComponentRuntime, PageTemplateContribution,
    PresentationTemplateDocumentV1, PresentationTemplateKind, ShellTemplateContribution,
    TemplateSlotDefinition, ThemePresetContribution,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_TEMPLATE_DOCUMENT_BYTES: u64 = 512 * 1024;
const REQUIRED_SHELL_NODES: [&str; 5] = [
    "pm-surface-host",
    "pm-overlay-host",
    "pm-window-controls",
    "pm-window-drag-region",
    "pm-recovery-entry",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplatePreviewRequest {
    pub component_id: String,
    pub template_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplatePreview {
    pub component_id: String,
    pub template_id: String,
    pub component_name: String,
    pub name: String,
    pub kind: PresentationTemplateKind,
    pub version: String,
    pub base_html: Option<String>,
    /// Scoped CSS produced by the presentation compiler. The raw stylesheet
    /// never needs to be injected into the host document.
    pub compiled_styles: Option<String>,
    /// Kept for compatibility with the diagnostics preview introduced before
    /// interface templates; normal shell rendering uses compiledStyles.
    pub styles: Option<String>,
    pub regions: Vec<String>,
    pub slots: Vec<TemplateSlotDefinition>,
    pub options_schema: Option<Value>,
    pub semantic_version: Option<String>,
    /// Pseudo-elements are supported, but can create visual layers above a
    /// component surface. Keep this visible to template authors instead of
    /// rejecting otherwise valid static CSS.
    pub css_warnings: Vec<String>,
    pub content_digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceTemplateDiagnostic {
    pub code: String,
    pub severity: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceTemplateLayoutValidation {
    pub valid: bool,
    pub diagnostics: Vec<InterfaceTemplateDiagnostic>,
}

/// External presentation components are data-only. Validate every referenced
/// template while it is still in staging so invalid markup cannot reach a
/// profile or replace the current Shell.
pub fn validate_presentation_component(
    root: &Path,
    manifest: &ComponentManifestV1,
) -> Result<(), ComponentRuntimeError> {
    let has_presentation = !manifest.contributes.shell_templates.is_empty()
        || !manifest.contributes.page_templates.is_empty()
        || !manifest.contributes.theme_presets.is_empty();
    if !has_presentation {
        return Ok(());
    }
    if manifest.runtime != ComponentRuntime::DataPack {
        return Err(template_error("外部表现模板组件必须使用 data-pack 运行时"));
    }
    if !manifest.capabilities.is_empty() {
        return Err(template_error("外部表现模板组件不能申请能力权限"));
    }
    for template in &manifest.contributes.shell_templates {
        validate_shell_template(root, template)?;
    }
    for template in &manifest.contributes.page_templates {
        validate_page_template(root, template)?;
    }
    for template in &manifest.contributes.theme_presets {
        validate_theme_template(root, template)?;
    }
    Ok(())
}

/// Loads only already-validated, package-local presentation resources. This is
/// deliberately separate from component invocation: callers receive static
/// markup for a sandboxed preview, never a path or a capability-bearing API.
pub fn load_presentation_template_preview(
    root: &Path,
    manifest: &ComponentManifestV1,
    template_id: &str,
) -> Result<PresentationTemplatePreview, ComponentRuntimeError> {
    let template_id = template_id.trim();
    if template_id.is_empty() {
        return Err(template_error("表现模板预览缺少 templateId"));
    }
    let (kind, name, version, regions, descriptor_value, is_shell) = if let Some(template) =
        manifest
            .contributes
            .shell_templates
            .iter()
            .find(|template| template.id == template_id)
    {
        (
            PresentationTemplateKind::Shell,
            template.name.clone(),
            template.version.clone(),
            Vec::new(),
            template.extensions.get("templatePath"),
            true,
        )
    } else if let Some(template) = manifest
        .contributes
        .page_templates
        .iter()
        .find(|template| template.id == template_id)
    {
        (
            PresentationTemplateKind::Page,
            template.name.clone(),
            template.version.clone(),
            template.regions.clone(),
            template.extensions.get("templatePath"),
            false,
        )
    } else if let Some(template) = manifest
        .contributes
        .theme_presets
        .iter()
        .find(|template| template.id == template_id)
    {
        (
            PresentationTemplateKind::Theme,
            template.name.clone(),
            template.version.clone(),
            Vec::new(),
            template.extensions.get("templatePath"),
            false,
        )
    } else {
        return Err(template_error(format!(
            "组件没有声明表现模板: {template_id}"
        )));
    };
    let descriptor = read_descriptor(root, descriptor_value)?;
    validate_descriptor(&descriptor, template_id, &version, kind)?;
    let base_html = match descriptor.base_html.as_deref() {
        Some(path) => {
            let html = read_limited_text(&resolve_template_path(root, path)?)?;
            validate_html(&html, is_shell, &descriptor.slots)?;
            Some(html)
        }
        None if kind == PresentationTemplateKind::Theme => None,
        None => return Err(template_error("页面或 Shell 模板缺少 baseHtml")),
    };
    let styles = match descriptor.styles.as_deref() {
        Some(path) => {
            let css = read_limited_text(&resolve_template_path(root, path)?)?;
            validate_css(&css)?;
            Some(css)
        }
        None => None,
    };
    let compiled_styles = styles
        .as_deref()
        .map(|css| compile_scoped_styles(css, template_id))
        .transpose()?;
    let css_warnings = styles
        .as_deref()
        .map(template_css_warnings)
        .unwrap_or_default();
    let content_digest = blake3::hash(
        format!(
            "{}\n{}\n{}\n{}",
            manifest.id,
            template_id,
            base_html.as_deref().unwrap_or_default(),
            styles.as_deref().unwrap_or_default()
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    Ok(PresentationTemplatePreview {
        component_id: manifest.id.clone(),
        template_id: template_id.into(),
        component_name: manifest.name.clone(),
        name,
        kind,
        version,
        base_html,
        compiled_styles,
        styles,
        regions,
        slots: descriptor.slots,
        options_schema: descriptor.options_schema,
        semantic_version: descriptor.semantic_version,
        css_warnings,
        content_digest,
    })
}

fn validate_shell_template(
    root: &Path,
    contribution: &ShellTemplateContribution,
) -> Result<(), ComponentRuntimeError> {
    if contribution.adapter.is_some() {
        return Err(template_error("外部 Shell 模板不能声明宿主 adapter"));
    }
    let descriptor = read_descriptor(root, contribution.extensions.get("templatePath"))?;
    validate_descriptor(
        &descriptor,
        contribution.id.as_str(),
        contribution.version.as_str(),
        PresentationTemplateKind::Shell,
    )?;
    let html = read_template_text(
        root,
        descriptor.base_html.as_deref(),
        "Shell 模板缺少 baseHtml",
    )?;
    validate_html(&html, true, &descriptor.slots)?;
    validate_styles(root, descriptor.styles.as_deref())?;
    Ok(())
}

fn validate_page_template(
    root: &Path,
    contribution: &PageTemplateContribution,
) -> Result<(), ComponentRuntimeError> {
    let descriptor = read_descriptor(root, contribution.extensions.get("templatePath"))?;
    validate_descriptor(
        &descriptor,
        contribution.id.as_str(),
        contribution.version.as_str(),
        PresentationTemplateKind::Page,
    )?;
    let html = read_template_text(
        root,
        descriptor.base_html.as_deref(),
        "页面模板缺少 baseHtml",
    )?;
    validate_html(&html, false, &descriptor.slots)?;
    if contribution
        .regions
        .iter()
        .any(|region| !descriptor.regions.iter().any(|value| value == region))
    {
        return Err(template_error(
            "页面模板 descriptor 未声明组件贡献所需的全部 region",
        ));
    }
    validate_styles(root, descriptor.styles.as_deref())?;
    Ok(())
}

fn validate_theme_template(
    root: &Path,
    contribution: &ThemePresetContribution,
) -> Result<(), ComponentRuntimeError> {
    let descriptor = read_descriptor(root, contribution.extensions.get("templatePath"))?;
    validate_descriptor(
        &descriptor,
        contribution.id.as_str(),
        contribution.version.as_str(),
        PresentationTemplateKind::Theme,
    )?;
    if descriptor.base_html.is_some() {
        return Err(template_error("主题模板不能携带 baseHtml"));
    }
    validate_styles(root, descriptor.styles.as_deref())
}

fn read_descriptor(
    root: &Path,
    value: Option<&Value>,
) -> Result<PresentationTemplateDocumentV1, ComponentRuntimeError> {
    let path = value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| template_error("表现模板贡献缺少 templatePath"))?;
    let descriptor_path = resolve_template_path(root, path)?;
    let source = read_limited_text(&descriptor_path)?;
    parse_presentation_template(&source)
        .map_err(|error| template_error(format!("template.json 无效: {error}")))
}

fn validate_descriptor(
    descriptor: &PresentationTemplateDocumentV1,
    id: &str,
    version: &str,
    expected_kind: PresentationTemplateKind,
) -> Result<(), ComponentRuntimeError> {
    if descriptor.id != id || descriptor.version != version || descriptor.kind != expected_kind {
        return Err(template_error(
            "template.json 的 id、version 或 kind 与组件贡献不匹配",
        ));
    }
    Ok(())
}

fn read_template_text(
    root: &Path,
    value: Option<&str>,
    missing_message: &str,
) -> Result<String, ComponentRuntimeError> {
    let path = value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| template_error(missing_message))?;
    read_limited_text(&resolve_template_path(root, path)?)
}

fn validate_styles(root: &Path, value: Option<&str>) -> Result<(), ComponentRuntimeError> {
    let Some(path) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let css = read_limited_text(&resolve_template_path(root, path)?)?;
    validate_css(&css)
}

fn read_limited_text(path: &Path) -> Result<String, ComponentRuntimeError> {
    let metadata =
        fs::metadata(path).map_err(|error| template_error(format!("模板资源不可用: {error}")))?;
    if !metadata.is_file() {
        return Err(template_error("模板资源必须是普通文件"));
    }
    if metadata.len() > MAX_TEMPLATE_DOCUMENT_BYTES {
        return Err(template_error("单个模板文档不能超过 512 KiB"));
    }
    fs::read_to_string(path)
        .map_err(|error| template_error(format!("模板资源不是有效 UTF-8: {error}")))
}

fn resolve_template_path(root: &Path, relative: &str) -> Result<PathBuf, ComponentRuntimeError> {
    let path = Path::new(relative);
    if relative.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(template_error("模板资源路径不安全"));
    }
    let resolved = root.join(path);
    let root = fs::canonicalize(root)
        .map_err(|error| template_error(format!("模板包目录不可用: {error}")))?;
    let resolved = fs::canonicalize(&resolved)
        .map_err(|error| template_error(format!("模板资源不可用: {error}")))?;
    if !resolved.starts_with(&root) {
        return Err(template_error("模板资源路径越界"));
    }
    Ok(resolved)
}

fn validate_html(
    source: &str,
    is_shell: bool,
    slots: &[TemplateSlotDefinition],
) -> Result<(), ComponentRuntimeError> {
    let lowered = source.to_ascii_lowercase();
    for forbidden in [
        "<script",
        "<iframe",
        "<object",
        "<embed",
        "<base",
        "javascript:",
        "vbscript:",
        "file:",
        "data:",
        "window.__tauri__",
        "window.__nexora__",
        "eval(",
        "import(",
    ] {
        if lowered.contains(forbidden) {
            return Err(template_error(format!(
                "模板 HTML 包含禁止内容: {forbidden}"
            )));
        }
    }
    if source
        .split('<')
        .skip(1)
        .any(|tag| tag.trim_start().to_ascii_lowercase().starts_with("link"))
    {
        return Err(template_error("模板 HTML 不允许通过 link 加载外部资源"));
    }
    if contains_inline_event_handler(&lowered) {
        return Err(template_error("模板 HTML 不允许内联事件处理器"));
    }
    if contains_external_url(&lowered) {
        return Err(template_error("模板 HTML 不允许远程资源"));
    }
    if is_shell {
        // The persistent HostUtilityBar owns recovery access and window
        // controls. Older templates remain valid through their historical
        // required nodes; new typed templates declare their own slot tree.
        if slots.is_empty() {
            for node in REQUIRED_SHELL_NODES {
                if !lowered.contains(&format!("<{node}")) {
                    return Err(template_error(format!("Shell 模板缺少强制节点 <{node}>")));
                }
            }
        } else {
            let names = template_slot_names(source);
            for slot in slots {
                if !names.iter().any(|name| name == &slot.id) {
                    return Err(template_error(format!(
                        "模板插槽 {} 未在 baseHtml 中声明 <nexora-slot>",
                        slot.id
                    )));
                }
            }
        }
    }
    Ok(())
}

fn template_slot_names(source: &str) -> Vec<String> {
    let expression = regex::Regex::new(
        r#"(?is)<nexora-slot\b[^>]*\bname\s*=\s*[\"']\s*([a-z][a-z0-9-]*)\s*[\"'][^>]*>"#,
    )
    .expect("template slot regex must compile");
    expression
        .captures_iter(source)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .collect()
}

fn contains_inline_event_handler(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes
        .windows(3)
        .any(|window| window[0].is_ascii_whitespace() && window[1] == b'o' && window[2] == b'n')
}

fn contains_external_url(source: &str) -> bool {
    source.contains("http://") || source.contains("https://") || source.contains("//")
}

fn validate_css(source: &str) -> Result<(), ComponentRuntimeError> {
    let lowered = source.to_ascii_lowercase();
    for forbidden in [
        "@import",
        "expression(",
        "javascript:",
        "vbscript:",
        "file:",
        "data:",
        "http://",
        "https://",
        "url(//",
    ] {
        if lowered.contains(forbidden) {
            return Err(template_error(format!(
                "模板 CSS 包含禁止内容: {forbidden}"
            )));
        }
    }
    Ok(())
}

/// Templates are data-only packages. Scope every ordinary rule beneath the
/// template host so a package cannot restyle the Nexora utility bar, dialogs,
/// or another template. Nested conditional rules keep their at-rule and have
/// their contents compiled recursively.
fn compile_scoped_styles(source: &str, template_id: &str) -> Result<String, ComponentRuntimeError> {
    let scope = format!(r#"[data-nexora-interface-template="{}"]"#, template_id);
    compile_css_block(source, &scope)
}

fn compile_css_block(source: &str, scope: &str) -> Result<String, ComponentRuntimeError> {
    let mut output = String::with_capacity(source.len() + 128);
    let mut cursor = 0usize;
    while cursor < source.len() {
        let Some(open_relative) = source[cursor..].find('{') else {
            output.push_str(&source[cursor..]);
            break;
        };
        let open = cursor + open_relative;
        let selector = source[cursor..open].trim();
        let mut depth = 1usize;
        let mut index = open + 1;
        while index < source.len() && depth > 0 {
            match source.as_bytes()[index] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            index += 1;
        }
        if depth != 0 {
            return Err(template_error("模板 CSS 大括号未闭合"));
        }
        let body = &source[open + 1..index - 1];
        if selector.trim_start().starts_with('@') {
            let directive = selector.trim_start().to_ascii_lowercase();
            if directive.starts_with("@media")
                || directive.starts_with("@supports")
                || directive.starts_with("@container")
                || directive.starts_with("@layer")
            {
                output.push_str(selector);
                output.push('{');
                output.push_str(&compile_css_block(body, scope)?);
                output.push('}');
            } else {
                return Err(template_error(
                    "模板 CSS 只允许 @media、@supports、@container 或 @layer 嵌套规则",
                ));
            }
        } else {
            let selectors = selector
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| scope_css_selector(value, scope))
                .collect::<Result<Vec<_>, _>>()?;
            if selectors.is_empty() {
                return Err(template_error("模板 CSS 规则缺少选择器"));
            }
            output.push_str(&selectors.join(", "));
            output.push('{');
            output.push_str(body);
            output.push('}');
        }
        cursor = index;
    }
    Ok(output)
}

fn scope_css_selector(selector: &str, scope: &str) -> Result<String, ComponentRuntimeError> {
    let lowered = selector.to_ascii_lowercase();
    if selector == "*"
        || lowered.starts_with("html")
        || lowered.starts_with("body")
        || lowered.starts_with(":root")
    {
        return Err(template_error("模板 CSS 不能使用全局根选择器"));
    }
    Ok(format!("{scope} {selector}"))
}

fn template_css_warnings(source: &str) -> Vec<String> {
    let expression = regex::Regex::new(r"::[a-zA-Z][a-zA-Z0-9_-]*")
        .expect("pseudo-element regex must compile");
    let pseudo_elements = expression
        .find_iter(source)
        .map(|entry| entry.as_str().to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    if pseudo_elements.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "模板 CSS 使用伪元素（{}）。已允许并继续受模板范围隔离；请检查 z-index、覆盖范围、点击命中和可访问性。",
        pseudo_elements.into_iter().collect::<Vec<_>>().join("、")
    )]
}

fn template_error(message: impl Into<String>) -> ComponentRuntimeError {
    ComponentRuntimeError::new(ComponentRuntimeErrorCode::ComponentPackageInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rejects_scripted_or_remote_html() {
        assert!(validate_html("<pm-surface-host /><script>alert(1)</script>", false, &[]).is_err());
        assert!(validate_html("<img src=\"https://example.com/image.png\">", false, &[]).is_err());
        assert!(validate_html("<button onclick=\"run()\">Run</button>", false, &[]).is_err());
    }

    #[test]
    fn requires_recovery_nodes_for_shells() {
        assert!(validate_html("<pm-surface-host />", true, &[]).is_err());
        assert!(validate_html(
            "<pm-surface-host /><pm-overlay-host /><pm-window-controls /><pm-window-drag-region /><pm-recovery-entry />",
            true, &[],
        )
        .is_ok());
    }

    #[test]
    fn rejects_external_css() {
        assert!(validate_css("@import url(https://example.com/theme.css);").is_err());
        assert!(validate_css("main { background: url(../assets/hero.png); }").is_ok());
    }

    #[test]
    fn compiles_styles_under_template_scope() {
        let compiled = compile_scoped_styles(
            ".shell, .shell__main { color: red; } @media (max-width: 600px) { .shell { display: block; } }",
            "example.shell",
        )
        .unwrap();
        assert!(compiled.contains("[data-nexora-interface-template=\"example.shell\"] .shell"));
        assert!(compiled.contains("@media"));
        assert!(compile_scoped_styles("body { color: red; }", "example.shell").is_err());
    }

    #[test]
    fn permits_and_reports_pseudo_elements() {
        let compiled = compile_scoped_styles(
            ".pane::before { content: ''; } .pane::after { pointer-events: none; }",
            "example.shell",
        )
        .unwrap();
        assert!(compiled.contains("[data-nexora-interface-template=\"example.shell\"] .pane::before"));
        let warnings = template_css_warnings(".pane::before { content: ''; } .pane::after { content: ''; }");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("::before"));
        assert!(warnings[0].contains("::after"));
    }

    #[test]
    fn local_example_templates_pass_staging_validation() {
        let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri must have a workspace parent")
            .join("examples");
        for name in ["presentation-template-studio", "blank-home-template"] {
            let root = examples.join(name);
            let manifest = pmc_platform::parse_component_manifest(
                &fs::read_to_string(root.join("component.json")).unwrap(),
            )
            .unwrap();
            validate_presentation_component(&root, &manifest).unwrap();
        }
    }
}
