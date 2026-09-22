use serde_json::json;
use zihuan_core::agent::tools::Tool;

use crate::tools::workspace_tools::{AskUserTool, DEFAULT_TOOL_ASK_USER};

/// Purpose: Verify the ask_user tool normalizes the suggested answers the
/// dashboard renders as one-click choices, advertises the options field to the
/// model, and rejects an empty question.
///
/// Test Data: One call passes options with surrounding whitespace, a blank
/// entry, and a duplicate; another passes only blank options; a third passes an
/// empty question. Expects trimmed, de-duplicated options, an omitted options
/// list when nothing usable remains, a schema declaring `options`, and an error
/// result for the empty question.
#[test]
fn ask_user_normalizes_options_and_rejects_empty_question() {
    let tool = AskUserTool;

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_ASK_USER);
    let parameters = tool.spec().parameters();
    assert_eq!(parameters["properties"]["options"]["type"], "array");
    assert!(parameters["properties"]["options"]["description"]
        .as_str()
        .is_some_and(|description| description.contains("one-click")));

    let output = tool.execute_with_outcome(
        "",
        &json!({
            "question": "  Which flavor?  ",
            "details": "  pick one  ",
            "options": ["  yes  ", "", "no", "yes"]
        }),
    );
    let request = output.ask_user.expect("ask_user request");
    assert_eq!(request.question, "Which flavor?");
    assert_eq!(request.details.as_deref(), Some("pick one"));
    assert_eq!(request.options, Some(vec!["yes".to_string(), "no".to_string()]));

    let blank = tool.execute_with_outcome("", &json!({"question": "q", "options": ["  ", ""]}));
    assert_eq!(blank.ask_user.expect("ask_user request").options, None);

    let invalid = tool.execute_with_outcome("", &json!({"question": "   "}));
    assert!(invalid.ask_user.is_none());
    assert!(invalid.result.contains("question must not be empty"));
}
