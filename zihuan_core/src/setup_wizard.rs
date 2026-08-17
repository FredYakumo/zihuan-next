use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SetupWizardState {
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub skipped: bool,
    pub completed_at: Option<String>,
    pub mode: Option<String>,
    pub last_step: Option<String>,
    pub last_error: Option<String>,
}

fn setup_wizard_state_path() -> PathBuf {
    crate::system_config::application_data_dir().join("setup_wizard_state.json")
}

fn setup_completed_flag_path() -> PathBuf {
    crate::system_config::application_data_dir().join(".setup_completed")
}

pub fn load_setup_wizard_state() -> Result<SetupWizardState> {
    let path = setup_wizard_state_path();
    if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|e| crate::string_error!("failed to read setup wizard state: {e}"))?;
        return serde_json::from_str(&content)
            .map_err(|e| crate::string_error!("failed to parse setup wizard state: {e}"));
    }

    // Fallback: migrate from old system_config.json section.
    if let Ok(old) = crate::system_config::load_section::<SetupWizardSection>() {
        let _ = save_setup_wizard_state(&old);
        return Ok(old);
    }

    Ok(SetupWizardState::default())
}

// Internal type used only for migrating from the old system_config section.
struct SetupWizardSection;

impl crate::system_config::SystemConfigSection for SetupWizardSection {
    const SECTION_KEY: &'static str = "setup_wizard";
    type Value = SetupWizardState;
}

pub fn save_setup_wizard_state(state: &SetupWizardState) -> Result<()> {
    let path = setup_wizard_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(state)
        .map_err(|e| crate::string_error!("failed to serialize setup wizard state: {e}"))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, &path)?;

    // Also create a flag file so other tools can quickly check completion.
    if state.completed || state.skipped {
        let flag = setup_completed_flag_path();
        if let Some(parent) = flag.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&flag, "");
    }

    Ok(())
}

pub fn clear_setup_wizard_state() -> Result<()> {
    let _ = fs::remove_file(setup_wizard_state_path());
    let _ = fs::remove_file(setup_completed_flag_path());
    Ok(())
}
