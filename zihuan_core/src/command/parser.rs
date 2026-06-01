/// Result of parsing a potential slash-command from raw user input.
///
/// ## Purpose
///
/// Parsing a command message involves recognising the leading `/`, extracting
/// the command name, consuming up to `accepted_arg_count` positional arguments,
/// and preserving any leftover text as passthrough for the LLM.
///
/// ## Design
///
/// - `command_name` is always lowercased for case-insensitive matching.
/// - `args` consumes at most `accepted_arg_count` whitespace-delimited tokens.
/// - `passthrough_text` is the remainder joined back with spaces — it is
///   `None` when there is no text beyond the consumed arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub command_name: String,
    pub args: Vec<String>,
    pub passthrough_text: Option<String>,
}

/// Parse a raw message into a `ParsedCommand` if it starts with `/`.
///
/// `accepted_arg_count` controls how many whitespace-delimited tokens after
/// the command name are consumed as positional arguments. All remaining text
/// is preserved as `passthrough_text`. A value of 0 means the entire remainder
/// is passthrough (parameterless commands like `/new`, `/help`).
pub fn parse_command(raw_input: &str, accepted_arg_count: u8) -> Option<ParsedCommand> {
    let trimmed = raw_input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let body = &trimmed[1..];
    let mut tokens = body.split_whitespace();
    let command_name = tokens.next()?.to_lowercase();

    let mut args: Vec<String> = Vec::with_capacity(accepted_arg_count as usize);
    for _ in 0..accepted_arg_count {
        match tokens.next() {
            Some(t) => args.push(t.to_string()),
            None => break,
        }
    }

    let remainder: Vec<&str> = tokens.collect();
    let passthrough_text = if remainder.is_empty() {
        None
    } else {
        Some(remainder.join(" "))
    };

    Some(ParsedCommand {
        command_name,
        args,
        passthrough_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_command_returns_none() {
        assert_eq!(parse_command("hello world", 0), None);
        assert_eq!(parse_command("  hello", 0), None);
    }

    #[test]
    fn zero_args_no_passthrough() {
        let parsed = parse_command("/new", 0).unwrap();
        assert_eq!(parsed.command_name, "new");
        assert!(parsed.args.is_empty());
        assert_eq!(parsed.passthrough_text, None);
    }

    #[test]
    fn zero_args_with_passthrough() {
        let parsed = parse_command("/new 你好世界", 0).unwrap();
        assert!(parsed.args.is_empty());
        assert_eq!(parsed.passthrough_text.as_deref(), Some("你好世界"));
    }

    #[test]
    fn one_arg_no_passthrough() {
        let parsed = parse_command("/task abc123", 1).unwrap();
        assert_eq!(parsed.args, vec!["abc123"]);
        assert_eq!(parsed.passthrough_text, None);
    }

    #[test]
    fn one_arg_with_passthrough() {
        let parsed = parse_command("/task abc123 告诉我这是什么", 1).unwrap();
        assert_eq!(parsed.command_name, "task");
        assert_eq!(parsed.args, vec!["abc123"]);
        assert_eq!(parsed.passthrough_text.as_deref(), Some("告诉我这是什么"));
    }

    #[test]
    fn two_args_with_passthrough() {
        let parsed = parse_command("/cmd a b and the rest here", 2).unwrap();
        assert_eq!(parsed.args, vec!["a", "b"]);
        assert_eq!(
            parsed.passthrough_text.as_deref(),
            Some("and the rest here")
        );
    }

    #[test]
    fn args_exceed_available_tokens() {
        let parsed = parse_command("/cmd only", 3).unwrap();
        assert_eq!(parsed.args, vec!["only"]);
        assert_eq!(parsed.passthrough_text, None);
    }

    #[test]
    fn parse_case_insensitive() {
        let parsed = parse_command("/NeW", 0).unwrap();
        assert_eq!(parsed.command_name, "new");
    }

    #[test]
    fn parse_with_leading_whitespace() {
        let parsed = parse_command("  /help  ", 0).unwrap();
        assert_eq!(parsed.command_name, "help");
        assert_eq!(parsed.passthrough_text, None);
    }
}
