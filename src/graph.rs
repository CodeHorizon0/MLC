use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use tempfile::NamedTempFile;

use crate::config::{Language, ScriptInput};
use crate::error::{KernelError, KernelResult};
use crate::util::{canonical_if_possible, normalize_abs, path_exists, path_to_module_name, resolve_module_file, unique_push, write_string};

#[derive(Debug)]
pub struct DependencyGraph {
    pub language: Language,
    pub entry_path: PathBuf,
    pub entry_module: Option<String>,
    pub base_dir: PathBuf,
    pub modules: BTreeMap<String, PathBuf>,
    pub entry_is_code: bool,
    #[allow(dead_code)]
    temp_guard: Option<NamedTempFile>,
}

impl DependencyGraph {
    pub fn discovered_module_names(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
}


fn resolve_entry_path(base_dir: &Path, path: &Path) -> KernelResult<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    Ok(canonical_if_possible(candidate)?)
}

pub fn build_graph(language: Language, input: ScriptInput, base_dir: &Path) -> KernelResult<DependencyGraph> {
    match language {
        Language::Python => build_python_graph(input, base_dir),
        Language::Lua => build_lua_graph(input, base_dir),
        Language::Js => build_js_graph(input, base_dir),
    }
}

fn build_python_graph(input: ScriptInput, base_dir: &Path) -> KernelResult<DependencyGraph> {
    let base_dir = normalize_abs(base_dir)?;
    let (entry_path, entry_module, entry_is_code, owned_temp) = match input {
        ScriptInput::Path(path) => {
            let abs = resolve_entry_path(&base_dir, &path)?;
            if !path_exists(&abs) {
                return Err(KernelError::EntryPathNotFound { path: abs });
            }
            let module = path_to_module_name(&base_dir, &abs);
            (abs, module, false, None)
        }
        ScriptInput::Code(code) => {
            let tmp = NamedTempFile::new_in(&base_dir)?;
            write_string(tmp.path(), &code)?;
            let abs = canonical_if_possible(tmp.path())?;
            let module = path_to_module_name(&base_dir, &abs).or_else(|| Some("__kernel_entry__".to_string()));
            (abs, module, true, Some(tmp))
        }
    };

    let mut modules = BTreeMap::<String, PathBuf>::new();
    let mut visited = BTreeSet::<PathBuf>::new();
    let mut queue = VecDeque::<PathBuf>::new();
    queue.push_back(entry_path.clone());

    while let Some(file) = queue.pop_front() {
        let file = canonical_if_possible(&file)?;
        if !visited.insert(file.clone()) {
            continue;
        }

        let module_name = path_to_module_name(&base_dir, &file).unwrap_or_else(|| {
            file.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module")
                .to_string()
        });
        unique_push(&mut modules, module_name.clone(), file.clone());

        let source = fs::read_to_string(&file)?;
        for imported in python_imports_from_source(&source) {
            let local_candidates = resolve_python_import_candidates(&file, &base_dir, &module_name, &imported);
            for candidate in local_candidates {
                if candidate.exists() {
                    queue.push_back(candidate);
                    break;
                }
            }
        }
    }

    Ok(DependencyGraph {
        language: Language::Python,
        entry_path,
        entry_module,
        base_dir,
        modules,
        entry_is_code,
        temp_guard: owned_temp,
    })
}

fn build_js_graph(input: ScriptInput, base_dir: &Path) -> KernelResult<DependencyGraph> {
    let base_dir = normalize_abs(base_dir)?;
    let (entry_path, entry_module, entry_is_code, owned_temp) = match input {
        ScriptInput::Path(path) => {
            let abs = resolve_entry_path(&base_dir, &path)?;
            if !path_exists(&abs) {
                return Err(KernelError::EntryPathNotFound { path: abs });
            }
            let module = path_to_module_name(&base_dir, &abs);
            (abs, module, false, None)
        }
        ScriptInput::Code(code) => {
            let tmp = NamedTempFile::new_in(&base_dir)?;
            write_string(tmp.path(), &code)?;
            let abs = canonical_if_possible(tmp.path())?;
            let module = Some("__kernel_entry__".to_string());
            (abs, module, true, Some(tmp))
        }
    };

    let mut modules = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(entry_path.clone());

    while let Some(file) = queue.pop_front() {
        let file = canonical_if_possible(&file)?;
        if !visited.insert(file.clone()) {
            continue;
        }
        let name = path_to_module_name(&base_dir, &file).unwrap_or_else(|| file.file_stem().unwrap().to_string_lossy().to_string());
        unique_push(&mut modules, name, file.clone());
        let source = fs::read_to_string(&file)?;
        for imported in js_imports_from_source(&source) {
            for candidate in resolve_js_import_candidates(&file, &imported) {
                if candidate.exists() {
                    queue.push_back(candidate);
                    break;
                }
            }
        }
    }

    Ok(DependencyGraph {
        language: Language::Js,
        entry_path,
        entry_module,
        base_dir,
        modules,
        entry_is_code,
        temp_guard: owned_temp,
    })
}

fn js_imports_from_source(source: &str) -> Vec<String> {
    let re = Regex::new(r#"(?:import\s+.*?from\s+['\"](.+?)['\"]|require\(['\"](.+?)['\"]\))"#).unwrap();
    re.captures_iter(source).filter_map(|c| c.get(1).or_else(|| c.get(2)).map(|v| v.as_str().to_string())).collect()
}

fn resolve_js_import_candidates(file: &Path, module: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let path = if module.starts_with("./") || module.starts_with("../") {
        file.parent().unwrap_or(Path::new(".")).join(module)
    } else {
        PathBuf::from(module)
    };
    out.push(path.clone());
    out.push(path.with_extension("js"));
    out.push(path.join("index.js"));
    out
}

fn build_lua_graph(input: ScriptInput, base_dir: &Path) -> KernelResult<DependencyGraph> {
    let base_dir = normalize_abs(base_dir)?;
    let (entry_path, entry_module, entry_is_code, owned_temp) = match input {
        ScriptInput::Path(path) => {
            let abs = resolve_entry_path(&base_dir, &path)?;
            if !path_exists(&abs) {
                return Err(KernelError::EntryPathNotFound { path: abs });
            }
            let module = path_to_module_name(&base_dir, &abs);
            (abs, module, false, None)
        }
        ScriptInput::Code(code) => {
            let tmp = NamedTempFile::new_in(&base_dir)?;
            write_string(tmp.path(), &code)?;
            let abs = canonical_if_possible(tmp.path())?;
            let module = path_to_module_name(&base_dir, &abs).or_else(|| Some("__kernel_entry__".to_string()));
            (abs, module, true, Some(tmp))
        }
    };

    let mut modules = BTreeMap::<String, PathBuf>::new();
    let mut visited = BTreeSet::<PathBuf>::new();
    let mut queue = VecDeque::<PathBuf>::new();
    queue.push_back(entry_path.clone());

    while let Some(file) = queue.pop_front() {
        let file = canonical_if_possible(&file)?;
        if !visited.insert(file.clone()) {
            continue;
        }

        let source = fs::read_to_string(&file)?;
        let module_name = path_to_module_name(&base_dir, &file).unwrap_or_else(|| {
            file.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module")
                .to_string()
        });
        unique_push(&mut modules, module_name.clone(), file.clone());

        for imported in lua_imports_from_source(&source) {
            let local_candidates = resolve_lua_import_candidates(&file, &base_dir, &imported);
            for candidate in local_candidates {
                if candidate.exists() {
                    queue.push_back(candidate);
                    break;
                }
            }
        }
    }

    Ok(DependencyGraph {
        language: Language::Lua,
        entry_path,
        entry_module,
        base_dir,
        modules,
        entry_is_code,
        temp_guard: owned_temp,
    })
}

fn python_imports_from_source(source: &str) -> Vec<String> {
    let import_re = Regex::new(r"^\s*import\s+(.+)$").unwrap();
    let from_re = Regex::new(r"^\s*from\s+([\.\w]+)\s+import\s+(.+)$").unwrap();
    let mut out = Vec::new();

    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = import_re.captures(line) {
            let list = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            for item in list.split(',') {
                let root = item.trim().split_whitespace().next().unwrap_or("");
                if !root.is_empty() {
                    out.push(root.to_string());
                }
            }
            continue;
        }

        if let Some(caps) = from_re.captures(line) {
            let base = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let imports = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            out.push(base.to_string());
            for item in imports.split(',') {
                let name = item.trim().split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    let mut combined = base.to_string();
                    if !combined.ends_with('.') && !combined.is_empty() {
                        combined.push('.');
                    }
                    combined.push_str(name);
                    out.push(combined);
                }
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

fn lua_imports_from_source(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let require_re = Regex::new(r#"require\s*\(\s*['\"]([^'\"]+)['\"]\s*\)"#).unwrap();
    let require_re_alt = Regex::new(r#"require\s+['\"]([^'\"]+)['\"]"#).unwrap();

    for caps in require_re.captures_iter(source) {
        if let Some(m) = caps.get(1) {
            out.push(m.as_str().to_string());
        }
    }
    for caps in require_re_alt.captures_iter(source) {
        if let Some(m) = caps.get(1) {
            out.push(m.as_str().to_string());
        }
    }

    out.sort();
    out.dedup();
    out
}

fn resolve_python_import_candidates(file: &Path, base_dir: &Path, current_module: &str, imported: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let current_dir = file.parent().unwrap_or(base_dir);

    if imported.starts_with('.') {
        let mut dots = 0usize;
        for ch in imported.chars() {
            if ch == '.' {
                dots += 1;
            } else {
                break;
            }
        }
        let remainder = imported[dots..].trim_start_matches('.');
        let mut prefix_parts: Vec<&str> = current_module.split('.').collect();
        if !prefix_parts.is_empty() {
            prefix_parts.pop();
        }
        for _ in 1..dots {
            if !prefix_parts.is_empty() {
                prefix_parts.pop();
            }
        }
        let mut resolved = prefix_parts.join(".");
        if !remainder.is_empty() {
            if !resolved.is_empty() {
                resolved.push('.');
            }
            resolved.push_str(remainder);
        }
        out.extend(resolve_python_by_name(base_dir, current_dir, &resolved));
        return out;
    }

    out.extend(resolve_python_by_name(base_dir, current_dir, imported));
    out
}

fn resolve_python_by_name(base_dir: &Path, current_dir: &Path, module_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if module_name.is_empty() {
        return out;
    }

    let roots = [current_dir.to_path_buf(), base_dir.to_path_buf()];
    for root in roots {
        if let Some(path) = resolve_module_file(&root, module_name, ".py") {
            out.push(path);
        }
    }

    out.sort();
    out.dedup();
    out
}

fn resolve_lua_import_candidates(file: &Path, base_dir: &Path, module_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let current_dir = file.parent().unwrap_or(base_dir);
    let roots = [current_dir.to_path_buf(), base_dir.to_path_buf()];

    for root in roots {
        if let Some(path) = resolve_module_file(&root, module_name, ".lua") {
            out.push(path);
        }
    }

    out.sort();
    out.dedup();
    out
}
