"""Dream memory consolidation job.

The scheduler kernel fires this script when a sender stays silent for the configured
interval. The script builds the transcript from the service's conversation history,
merges it with the sender's previous Dream memory (one record per sender in the agent's
memory store) through the dream agent defined inline below, writes the consolidated
memory back, and clears the history. Host capabilities are called through
`zihuan_sdk.JobSdk`.
"""

from zihuan_sdk import Host, JobSdk

JOB_MANIFEST = {
    "task_name": "Dream",
    "entry": "run_job",
    "description": "用户一段时间后没有与Agent对话，则会自动总结对话并生成相关记忆",
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
    sender_id = request["sender_id"]
    memory_key = f"dream:{sender_id}"

    messages = sdk.load_history(sender_id)
    lines = []
    for message in messages:
        role = message.get("role")
        if role not in ("user", "assistant"):
            continue
        text = message.get("text") or ""
        lines.append(f"{'用户' if role == 'user' else 'Bot'}: {text}")
    transcript = "\n".join(lines)

    previous = ""
    for record in sdk.list_memory(limit=100, sender_id=sender_id).get("items", []):
        if record.get("key") == memory_key:
            previous = record.get("value") or ""
            break

    if not transcript.strip() and not previous.strip():
        # Nothing to consolidate: never fabricate a memory from an empty transcript
        # (e.g. a pending task whose conversation cache was wiped by a restart).
        return {"ok": True, "result": "无对话内容，跳过 Dream 记忆生成"}

    result = sdk.run_subagent(DREAM_AGENT_YAML, previous_memory=previous, transcript=transcript)

    sdk.upsert_memory(key=memory_key, value=result, sender_id_list=[sender_id])
    sdk.clear_history(sender_id)

    return {"ok": True, "result": "Dream 记忆已生成"}
