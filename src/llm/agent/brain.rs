use std::sync::Arc;

use crate::bot_adapter::models::{MessageEvent, message::{Message as MsgEnum, PlainTextMessage, ReplyMessage}};
use crate::llm::{LLMBase, Message, MessageRole};
use crate::llm::agent::Agent;
use crate::llm::agent::{
	chat_history_agent::ChatHistoryAgent,
	code_writer_agent::CodeWriterAgent,
	math_agent::MathAgent,
	nl_reply_agent::NaturalLanguageReplyAgent,
};
use serde_json::json;

/// BrainAgent: a reasoning agent that routes incoming events to specialized agents
/// based on intent heuristics. It uses an LLM for NL tasks and simple rules for
/// other tool selection.
pub struct BrainAgent {
	/// Reasoning-capable LLM (can be the same as chat LLM)
	llm: Arc<dyn LLMBase + Send + Sync>,
	/// Tools wrapped as agents
	nl_reply: NaturalLanguageReplyAgent,
	code_writer: CodeWriterAgent,
	chat_history: ChatHistoryAgent,
	math: MathAgent,
}

impl BrainAgent {
	/// Construct a BrainAgent. The provided `llm` will be used for tools that
	/// require an LLM (NL reply, code writer).
	pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
		Self {
			nl_reply: NaturalLanguageReplyAgent::new(llm.clone()),
			code_writer: CodeWriterAgent::new(llm.clone()),
			chat_history: ChatHistoryAgent::new(),
			math: MathAgent::new(),
			llm,
		}
	}

	fn aggregate_text(event: &MessageEvent) -> String {
		let mut parts: Vec<String> = Vec::new();
		for m in &event.message_list {
			match m {
				MsgEnum::PlainText(PlainTextMessage { text }) => parts.push(text.clone()),
				MsgEnum::At(_) => {
					// Ignore explicit @ content from text aggregation; routing leverages it elsewhere
				}
				MsgEnum::Reply(ReplyMessage { id, .. }) => {
					parts.push(format!("[reply:{}]", id));
				}
			}
		}
		parts.join(" ")
	}

	fn has_reply(event: &MessageEvent) -> Option<i64> {
		for m in &event.message_list {
			if let MsgEnum::Reply(ReplyMessage { id, .. }) = m { return Some(*id); }
		}
		None
	}

	fn looks_like_math(expr: &str) -> bool {
		// crude signal: contains digits and arithmetic operators
		let has_digit = expr.chars().any(|c| c.is_ascii_digit());
		let has_op = expr.contains('+') || expr.contains('-') || expr.contains('*') || expr.contains('/') || expr.contains('×') || expr.contains('÷');
		let keywords = ["计算", "加", "减", "乘", "除", "sum", "add", "minus", "multiply", "divide"]; // zh/en
		has_digit && has_op || keywords.iter().any(|k| expr.contains(k))
	}

	fn looks_like_code(text: &str) -> bool {
		let keywords = [
			"写代码", "实现", "函数", "脚本", "代码", "generate code", "implement", "class", "function",
		];
		let fenced = text.contains("```");
		fenced || keywords.iter().any(|k| text.to_lowercase().contains(&k.to_lowercase()))
	}
}

impl Agent for BrainAgent {
	type Output = Message;

	fn name(&self) -> &'static str { "brain_agent" }

	fn on_event(&self, event: &MessageEvent) -> Self::Output {
		let text = Self::aggregate_text(event);

		// Prioritize explicit reply -> chat history
		if let Some(reply_id) = Self::has_reply(event) {
			// Use chat_history agent to fetch context, then provide a NL reply summarizing it
			let hist_msg = self.chat_history.on_agent_input(json!({"message_id": reply_id.to_string()}));
			let summary = hist_msg.content.unwrap_or_else(|| "No history found.".to_string());
			return Message { role: MessageRole::Assistant, content: Some(format!("引用消息({reply_id})的记录: {summary}")), tool_calls: Vec::new() };
		}

		// Math intent
		if Self::looks_like_math(&text) {
			return self.math.on_agent_input(json!({"text": text}));
		}

		// Code generation intent
		if Self::looks_like_code(&text) {
			return self.code_writer.on_agent_input(json!({"task": text}));
		}

		// Default: NL reply
		self.nl_reply.on_agent_input(json!({"prompt": text}))
	}

	fn on_agent_input(&self, input: serde_json::Value) -> Self::Output {
		let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
		let prompt = if text.is_empty() {
			input.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string()
		} else { text };
		self.nl_reply.on_agent_input(json!({"prompt": prompt}))
	}
}

