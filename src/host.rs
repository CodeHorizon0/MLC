use std::fs;
use std::path::PathBuf;
use std::sync::Once;

use pyo3::prelude::*;
use pyo3::types::PyModule;

use crate::error::KernelResult;

#[pyfunction]
fn log(level: &str, message: &str) {
    eprintln!("[host:{level}] {message}");
}

#[pyfunction]
fn read_text(path: PathBuf) -> PyResult<String> {
    fs::read_to_string(path).map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))
}

#[pyfunction]
fn write_text(path: PathBuf, content: &str) -> PyResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))?;
    }
    fs::write(path, content).map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))?;
    Ok(())
}

#[pyfunction]
fn cwd() -> PyResult<String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|err| pyo3::exceptions::PyOSError::new_err(err.to_string()))
}

#[pymodule]
pub fn host(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(log, m)?)?;
    m.add_function(wrap_pyfunction!(read_text, m)?)?;
    m.add_function(wrap_pyfunction!(write_text, m)?)?;
    m.add_function(wrap_pyfunction!(cwd, m)?)?;
    Ok(())
}

pub fn register_python_host_module() -> KernelResult<()> {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        pyo3::append_to_inittab!(host);
    });
    Ok(())
}

pub fn build_python_manifest_json(manifest: &[(String, PathBuf)]) -> KernelResult<String> {
    let mut map = std::collections::BTreeMap::<String, String>::new();
    for (name, path) in manifest {
        map.insert(name.clone(), path.to_string_lossy().to_string());
    }
    Ok(serde_json::to_string(&map)?)
}
