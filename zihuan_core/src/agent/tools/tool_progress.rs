use std::cell::RefCell;

thread_local! {
    static TOOL_PROGRESS_SCOPE_STACK: RefCell<Vec<ToolProgressScopeState>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone)]
struct ToolProgressScopeState {
    call_content: String,
    consumed: bool,
}

pub(crate) struct ToolProgressScopeGuard;

impl ToolProgressScopeGuard {
    pub(crate) fn enter(call_content: &str) -> Self {
        TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
            stack.borrow_mut().push(ToolProgressScopeState {
                call_content: call_content.to_string(),
                consumed: false,
            });
        });
        Self
    }
}

impl Drop for ToolProgressScopeGuard {
    fn drop(&mut self) {
        TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub fn consume_tool_progress_notification(call_content: &str) -> bool {
    let trimmed = call_content.trim();
    if trimmed.is_empty() {
        return false;
    }

    TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(scope) = stack.last_mut() else {
            return true;
        };
        if scope.call_content.trim() != trimmed {
            return true;
        }
        if scope.consumed {
            return false;
        }
        scope.consumed = true;
        true
    })
}

pub fn current_task_progress_message(call_content: &str) -> Option<String> {
    let trimmed = call_content.trim();
    if trimmed.is_empty() || !consume_tool_progress_notification(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}
