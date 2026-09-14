"""Dream memory consolidation job.

The scheduler kernel fires this script when a sender stays silent for the configured
interval. The script builds the transcript from the service's conversation history,
merges it with the latest Dream memory through the dream agent defined inline below,
persists the result, and clears the history. Host capabilities are called through
`zihuan_sdk.JobSdk`.
"""

from zihuan_sdk import Host, JobSdk

JOB_MANIFEST = {
    "task_name": "Dream",
    "entry": "run_job",
    "description": "用户静默后合并对话历史与上一次 Dream 记忆，生成新的长期记忆。",
}

DREAM_AGENT_YAML = """\
id: dream
name: Dream
description: Dream memory consolidation agent.
inputs:
- name: previous_memory
  data_type: String
  description: Previous Dream memory
  required: false
- name: transcript
  data_type: String
  description: Conversation transcript
  required: true
outputs:
- name: memory
  data_type: String
  description: Consolidated memory
  required: true
system_prompt: |-
  You are the Dream memory consolidation agent. Produce concise long-term memories in English. Do not address the user. Use the available node graph tools synchronously when they are relevant to consolidating the memory.
user_prompt: |-
  Combine the previous Dream memory with this conversation. Record durable facts, preferences, relationships, emotions, and emotional continuity. Do not invent information.

  Previous Dream memory:
  {previous_memory}

  Current conversation:
  {transcript}
output_mode: text
include_graph_tools: true
"""


def run_job(request):
    sdk = JobSdk(Host())
    agent_id = request["agent_id"]
    sender_id = request["sender_id"]

    messages = sdk.load_history(sender_id)
    lines = []
    for message in messages:
        role = message.get("role")
        if role not in ("user", "assistant"):
            continue
        text = message.get("text") or ""
        lines.append(f"{'用户' if role == 'user' else 'Bot'}: {text}")
    transcript = "\n".join(lines)
    chars = sum(len(line) for line in lines)

    previous = sdk.latest_dream_memory(agent_id, sender_id) or ""

    result = sdk.run_subagent(DREAM_AGENT_YAML, previous_memory=previous, transcript=transcript)

    sdk.insert_dream_memory(agent_id, sender_id, chars, result)
    sdk.clear_history(sender_id)

    return {"ok": True, "result": "Dream 记忆已生成"}
