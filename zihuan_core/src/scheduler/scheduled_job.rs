//! Role-service configuration for scheduler-triggered jobs: which task to run and the
//! event that arms it. Shared by every role service type that can trigger jobs.

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::utils::time_unit::TimeUnit;

/// One job a role service schedules when its trigger event fires. The task name is the
/// registry identity of the script-defined job body that runs once the event holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJobConfig {
    #[serde(default)]
    pub task_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_scheduled_job_trigger")]
    pub trigger: JobTrigger,
}

/// The event that arms a scheduled job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobTrigger {
    /// The subject has not sent any message to the agent for the configured interval. Each
    /// new message from the subject re-arms the task, so only the final silence counts.
    SenderSilence {
        #[serde(default)]
        interval_value: usize,
        #[serde(default)]
        interval_unit: TimeUnit,
    },
}

impl ScheduledJobConfig {
    /// Filters a configured job list down to the entries that actually schedule: enabled,
    /// with a non-blank task name, a valid trigger, and no duplicate task name.
    pub fn resolved(jobs: &[Self]) -> Vec<Self> {
        let mut resolved = Vec::new();
        for job in jobs {
            let task_name = job.task_name.trim();
            if !job.enabled
                || task_name.is_empty()
                || job.trigger.delay_seconds().is_none()
                || resolved
                    .iter()
                    .any(|existing: &ScheduledJobConfig| existing.task_name == task_name)
            {
                continue;
            }
            resolved.push(Self {
                task_name: task_name.to_string(),
                enabled: true,
                trigger: job.trigger.clone(),
            });
        }
        resolved
    }
}

impl JobTrigger {
    /// The debounce delay this trigger schedules with, or `None` when no positive interval
    /// is configured. Only silence-style triggers carry one.
    pub fn delay_seconds(&self) -> Option<Duration> {
        match self {
            Self::SenderSilence { interval_value, interval_unit } => {
                if *interval_value == 0 {
                    return None;
                }
                interval_value
                    .checked_mul(interval_unit.seconds() as usize)
                    .map(|seconds| Duration::seconds(seconds as i64))
            }
        }
    }
}

fn default_scheduled_job_trigger() -> JobTrigger {
    JobTrigger::SenderSilence {
        interval_value: 0,
        interval_unit: TimeUnit::default(),
    }
}

fn default_true() -> bool {
    true
}
