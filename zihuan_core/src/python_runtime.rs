use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PythonRuntimeKind {
    #[default]
    UvProject,
    #[serde(alias = "venv_python")]
    ProjectVenv,
    CustomExecutable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PythonRuntimeConfig {
    #[serde(default)]
    pub kind: PythonRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
}

impl Default for PythonRuntimeConfig {
    fn default() -> Self {
        Self {
            kind: PythonRuntimeKind::UvProject,
            executable_path: None,
        }
    }
}

impl From<PythonRuntimeKind> for PythonRuntimeConfig {
    fn from(kind: PythonRuntimeKind) -> Self {
        Self { kind, executable_path: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_venv_python_name_is_accepted() {
        let kind: PythonRuntimeKind =
            serde_json::from_str("\"venv_python\"").expect("legacy Python runtime kind should deserialize");
        assert_eq!(kind, PythonRuntimeKind::ProjectVenv);
    }
}
