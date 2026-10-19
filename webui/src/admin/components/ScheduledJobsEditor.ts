import { computed } from "vue";

import type { QqChatScheduledJobFormItem } from "../model";
import type { SchedulerJobStatus } from "../../api/types";

export const EVENT_OPTIONS = [{ value: "sender_silence", label: "用户一段时间不发送消息" }];
export const INTERVAL_UNIT_OPTIONS = [
  { value: "minute", label: "分" },
  { value: "hour", label: "时" },
  { value: "day", label: "天" },
];

export interface ScheduledJobsEditorProps {
  jobs: QqChatScheduledJobFormItem[];
  availableJobs: SchedulerJobStatus[];
  rdbConfigured: boolean;
}

export interface ScheduledJobsEditorEmit {
  (event: "add"): void;
  (event: "remove", index: number): void;
}

export function useScheduledJobsEditor(
  props: ScheduledJobsEditorProps,
  _emit: ScheduledJobsEditorEmit,
) {
  /** Every registered scheduler job, plus any already-configured task name that is not in
   *  the catalog (e.g. its script was removed) so the select can still display it. */
  const taskOptions = computed(() => {
    const options: Array<{ value: string; label: string; description: string | undefined }> =
      props.availableJobs
        .filter((job) => job.task_name.trim().length > 0)
        .map((job) => ({
          value: job.task_name,
          label: job.task_name,
          description: job.description,
        }));
    const known = new Set(options.map((option) => option.value));
    for (const job of props.jobs) {
      const name = job.task_name.trim();
      if (name && !known.has(name)) {
        options.push({ value: name, label: name, description: undefined });
      }
    }
    return options;
  });

  return { EVENT_OPTIONS, INTERVAL_UNIT_OPTIONS, taskOptions };
}
