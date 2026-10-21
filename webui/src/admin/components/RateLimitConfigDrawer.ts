import { defaultQqChatMessageRateLimitRule, type ServiceFormState } from "../model";

export interface RateLimitConfigDrawerProps {
  form: ServiceFormState;
}

export function useRateLimitConfigDrawer(props: RateLimitConfigDrawerProps) {
  function addGroupRule() {
    props.form.message_rate_limit_groups.push({ group_id: "", ...defaultQqChatMessageRateLimitRule() });
  }

  function addUserRule() {
    props.form.message_rate_limit_users.push({ sender_id: "", ...defaultQqChatMessageRateLimitRule() });
  }

  return { addGroupRule, addUserRule };
}
