use crate::{ContractError, ContractErrorCode, ContractResult};
use semver::{Version, VersionReq};
use std::path::{Component, Path};

pub(crate) fn validate_stable_id(value: &str, path: &str) -> ContractResult<()> {
    validate_identifier(value, path, true, ContractErrorCode::InvalidStableId)
}

pub(crate) fn validate_local_id(value: &str, path: &str) -> ContractResult<()> {
    validate_identifier(value, path, false, ContractErrorCode::InvalidLocalId)
}

fn validate_identifier(
    value: &str,
    path: &str,
    require_namespace: bool,
    code: ContractErrorCode,
) -> ContractResult<()> {
    if value.len() < 2 || value.len() > 128 || (require_namespace && !value.contains('.')) {
        return Err(ContractError::new(
            code,
            path,
            format!("无效标识符: {value}"),
        ));
    }
    let invalid = if require_namespace {
        value.split('.').any(|segment| {
            segment.is_empty()
                || !segment
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || segment.ends_with('-')
        })
    } else {
        value.contains('.')
            || !value
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.ends_with('-')
    };
    if invalid {
        return Err(ContractError::new(
            code,
            path,
            format!("无效标识符: {value}"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_version(value: &str, path: &str) -> ContractResult<()> {
    Version::parse(value).map(|_| ()).map_err(|error| {
        ContractError::new(
            ContractErrorCode::InvalidVersion,
            path,
            format!("版本号 {value} 无效: {error}"),
        )
    })
}

pub(crate) fn validate_version_requirement(value: &str, path: &str) -> ContractResult<()> {
    VersionReq::parse(value).map(|_| ()).map_err(|error| {
        ContractError::new(
            ContractErrorCode::InvalidVersionRequirement,
            path,
            format!("版本约束 {value} 无效: {error}"),
        )
    })
}

pub(crate) fn validate_api_version(value: &str, path: &str) -> ContractResult<()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ContractError::new(
            ContractErrorCode::InvalidVersion,
            path,
            format!("API 版本必须是非负整数字符串: {value}"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_relative_path(value: &str, path: &str) -> ContractResult<()> {
    let parsed = Path::new(value);
    let invalid = value.is_empty()
        || value.contains('\\')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        });
    if invalid {
        return Err(ContractError::new(
            ContractErrorCode::InvalidRelativePath,
            path,
            format!("路径必须是使用 / 的包内相对路径: {value}"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_digest(value: &str, path: &str) -> ContractResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ContractError::new(
            ContractErrorCode::InvalidDigest,
            path,
            "摘要必须是 64 位小写十六进制字符串",
        ));
    }
    Ok(())
}
