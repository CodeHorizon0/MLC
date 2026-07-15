use std::collections::BTreeMap;
use std::ffi::{CString, OsStr};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::config::ScriptInput;
use crate::error::{KernelError, KernelResult};

use chrono::{SecondsFormat, Utc};

pub fn normalize_abs(path: impl AsRef<Path>) -> KernelResult<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn canonical_if_possible(path: impl AsRef<Path>) -> KernelResult<PathBuf> {
    let path = normalize_abs(path)?;
    Ok(fs::canonicalize(&path).unwrap_or(path))
}

pub fn path_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

pub fn read_to_string(path: impl AsRef<Path>) -> KernelResult<String> {
    Ok(fs::read_to_string(path)?)
}

pub fn write_string(path: impl AsRef<Path>, content: &str) -> KernelResult<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

pub fn to_cstring(s: &str) -> KernelResult<CString> {
    CString::new(s.as_bytes()).map_err(|_| KernelError::InvalidInput("source contains interior NUL byte".into()))
}

pub fn parse_script_input_source(base_dir: &Path, raw: &str) -> ScriptInput {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ScriptInput::Code(raw.to_string());
    }

    let candidate = Path::new(trimmed);
    let path_like = candidate.is_absolute()
        || candidate.exists()
        || base_dir.join(candidate).exists()
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with(r".\")
        || trimmed.starts_with(r"..\")
        || trimmed.ends_with(".py")
        || trimmed.ends_with(".lua");

    if path_like {
        ScriptInput::Path(candidate.to_path_buf())
    } else {
        ScriptInput::Code(raw.to_string())
    }
}

pub fn path_to_module_name(base_dir: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(base_dir).ok()?;
    let mut parts = Vec::new();
    for comp in rel.components() {
        if let Component::Normal(os) = comp {
            parts.push(os_to_string(os)?);
        }
    }
    if parts.is_empty() {
        return None;
    }

    if let Some(last) = parts.last_mut() {
        if let Some(stripped) = last.strip_suffix(".py") {
            *last = stripped.to_string();
        } else if let Some(stripped) = last.strip_suffix(".lua") {
            *last = stripped.to_string();
        }
    }

    if parts.last().map(|s| s.as_str()) == Some("__init__") {
        parts.pop();
    }

    let filtered: Vec<String> = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();

    if filtered.is_empty() {
        None
    } else {
        Some(filtered.join("."))
    }
}

pub fn module_to_candidates(root: &Path, module_name: &str, extension: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let rel = module_name.replace('.', "/");
    out.push(root.join(format!("{}{}", rel, extension)));
    out.push(root.join(rel).join(format!("__init__{}", extension)));
    out
}

pub fn unique_push(map: &mut BTreeMap<String, PathBuf>, key: String, value: PathBuf) {
    map.entry(key).or_insert(value);
}

pub fn os_to_string(os: &OsStr) -> Option<String> {
    os.to_str().map(ToOwned::to_owned)
}

pub fn candidate_roots(start: &Path, base_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut cur = Some(start.to_path_buf());
    while let Some(path) = cur {
        if roots.last() != Some(&path) {
            roots.push(path.clone());
        }
        cur = path.parent().map(|p| p.to_path_buf());
    }
    if roots.last() != Some(&base_dir.to_path_buf()) {
        roots.push(base_dir.to_path_buf());
    }
    roots
}

pub fn resolve_module_file(root: &Path, module_name: &str, extension: &str) -> Option<PathBuf> {
    for candidate in module_to_candidates(root, module_name, extension) {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn is_probably_package_dir(dir: &Path) -> bool {
    dir.join("__init__.py").exists() || dir.join("__init__.lua").exists()
}


pub fn utc_now_rfc3339_nanos() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}
