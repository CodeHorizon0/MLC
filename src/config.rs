use std::path::{Path, PathBuf};

use crate::util::parse_script_input_source;

#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub base_dir: PathBuf,
    pub python_enabled: bool,
    pub lua_enabled: bool,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            base_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            python_enabled: true,
            lua_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    Lua,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::Lua => "lua",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ScriptInput {
    Code(String),
    Path(PathBuf),
}

impl ScriptInput {
    pub fn from_source(input: impl AsRef<str>, base_dir: impl AsRef<Path>) -> Self {
        parse_script_input_source(base_dir.as_ref(), input.as_ref())
    }
}


pub trait ScriptInputSource {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput;
}

impl ScriptInputSource for ScriptInput {
    fn into_script_input(self, _base_dir: &Path) -> ScriptInput {
        self
    }
}

impl ScriptInputSource for &str {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput {
        ScriptInput::from_source(self, base_dir)
    }
}

impl ScriptInputSource for String {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput {
        ScriptInput::from_source(self, base_dir)
    }
}

impl<'a> ScriptInputSource for &'a String {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput {
        ScriptInput::from_source(self, base_dir)
    }
}

impl ScriptInputSource for std::path::PathBuf {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput {
        let candidate = if self.is_absolute() {
            self
        } else {
            base_dir.join(self)
        };
        ScriptInput::Path(candidate)
    }
}

impl<'a> ScriptInputSource for &'a std::path::Path {
    fn into_script_input(self, base_dir: &Path) -> ScriptInput {
        let candidate = if self.is_absolute() {
            self.to_path_buf()
        } else {
            base_dir.join(self)
        };
        ScriptInput::Path(candidate)
    }
}

pub struct RunRequest {
    pub language: Language,
    pub input: ScriptInput,
    pub working_dir: Option<PathBuf>,
}

impl RunRequest {
    pub fn code(language: Language, code: impl Into<String>) -> Self {
        Self {
            language,
            input: ScriptInput::Code(code.into()),
            working_dir: None,
        }
    }

    pub fn path(language: Language, path: impl Into<PathBuf>) -> Self {
        Self {
            language,
            input: ScriptInput::Path(path.into()),
            working_dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunReport {
    pub language: Language,
    pub entry_path: PathBuf,
    pub discovered_modules: Vec<String>,
}

