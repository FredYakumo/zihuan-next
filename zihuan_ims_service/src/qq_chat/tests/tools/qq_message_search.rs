use crate::tools::{SearchQqMessagesAccessContext, SearchQqMessagesTool, ToolNotificationTarget};
use zihuan_core::permission::EntityType;
use zihuan_core::storage::user_permission::{
    DEFAULT_ACCESS_LEVEL, PLANER_ACCESS_LEVEL, ROLE_SELF_ACCESS_LEVEL,
};

const SENDER_ID: &str = "旅行者";
const SENDER_NAME: &str = "旅行者";
const BOT_ID: &str = "派蒙";
const GROUP_ID: &str = "千织屋";
const OTHER_GROUP_ID: &str = "璃月";
const OTHER_SENDER_ID: &str = "钟离";

fn build_tool(
    is_group: bool,
    target_id: &str,
    sender_role: Option<&str>,
    sender_id: &str,
    planer: bool,
) -> SearchQqMessagesTool {
    SearchQqMessagesTool::new(
        None,
        ToolNotificationTarget::new(None, target_id.to_string(), None, is_group, false),
        SearchQqMessagesAccessContext {
            agent_id: "agent-1".to_string(),
            sender_id: sender_id.to_string(),
            sender_name: SENDER_NAME.to_string(),
            sender_role: sender_role.map(str::to_string),
            bot_id: BOT_ID.to_string(),
            permission_group: "default".to_string(),
            entity_type: EntityType::QqId,
            planer,
            adapter: None,
        },
    )
}

fn group_tool(role: Option<&str>) -> SearchQqMessagesTool {
    build_tool(true, GROUP_ID, role, SENDER_ID, false)
}

fn private_tool() -> SearchQqMessagesTool {
    build_tool(false, SENDER_ID, None, SENDER_ID, false)
}

fn expected_denial(query: &str, suggestion: &str) -> String {
    format!("\"{SENDER_NAME}\"没有权限查询\"{query}\"，建议改为查询: {suggestion}")
}

#[test]
fn only_access_levels_above_two_without_the_grant_are_restricted() {
    for level in 0..=2 {
        assert!(!SearchQqMessagesTool::restricted_by(level, false), "level {level}");
        assert!(!SearchQqMessagesTool::restricted_by(level, true), "level {level} with grant");
    }
    assert!(SearchQqMessagesTool::restricted_by(3, false));
    assert!(!SearchQqMessagesTool::restricted_by(3, true));
    assert!(SearchQqMessagesTool::restricted_by(4, false));
}

#[test]
fn planer_and_role_self_turns_bypass_the_restricted_rules() {
    let planer = build_tool(true, GROUP_ID, Some("member"), SENDER_ID, true);
    assert_eq!(planer.effective_access_level().unwrap(), PLANER_ACCESS_LEVEL);
    assert!(!planer.is_restricted().unwrap());

    let role_self = build_tool(true, GROUP_ID, None, "", false);
    assert_eq!(role_self.effective_access_level().unwrap(), ROLE_SELF_ACCESS_LEVEL);
    assert!(!role_self.is_restricted().unwrap());
}

#[test]
fn a_sender_without_a_resolved_level_falls_back_to_the_restricted_default() {
    let tool = group_tool(Some("member"));
    assert_eq!(tool.effective_access_level().unwrap(), DEFAULT_ACCESS_LEVEL);
    assert!(SearchQqMessagesTool::restricted_by(DEFAULT_ACCESS_LEVEL, false));
}

#[test]
fn restricted_senders_can_search_their_own_messages_anywhere() {
    let tool = group_tool(Some("member"));
    assert!(tool.permission_denial(None, Some(SENDER_ID), None, None, None).is_none());
    assert!(tool
        .permission_denial(Some(GROUP_ID), Some(SENDER_ID), None, None, None)
        .is_none());
    assert!(tool
        .permission_denial(Some(OTHER_GROUP_ID), Some(SENDER_ID), None, None, None)
        .is_none());

    let private = private_tool();
    assert!(private.permission_denial(None, Some(SENDER_ID), None, None, None).is_none());
    assert!(private
        .permission_denial(Some(OTHER_GROUP_ID), Some(SENDER_ID), None, None, None)
        .is_none());
}

#[test]
fn restricted_senders_can_search_bot_messages_in_the_current_group_only() {
    let tool = group_tool(Some("member"));
    assert!(tool.permission_denial(Some(GROUP_ID), Some(BOT_ID), None, None, None).is_none());

    let query = format!("{BOT_ID}在{OTHER_GROUP_ID}群的消息");
    assert_eq!(
        tool.permission_denial(Some(OTHER_GROUP_ID), Some(BOT_ID), None, None, None)
            .unwrap(),
        expected_denial(&query, "仅能查询在当前群聊的消息")
    );
    let query = format!("{BOT_ID}的消息");
    assert_eq!(
        tool.permission_denial(None, Some(BOT_ID), None, None, None).unwrap(),
        expected_denial(&query, "仅能查询在当前群聊的消息")
    );
}

#[test]
fn restricted_senders_can_search_bot_messages_in_private_chat_only() {
    let tool = private_tool();
    assert!(tool.permission_denial(None, Some(BOT_ID), None, None, None).is_none());

    let query = format!("{BOT_ID}在{GROUP_ID}群的消息");
    assert_eq!(
        tool.permission_denial(Some(GROUP_ID), Some(BOT_ID), None, None, None).unwrap(),
        expected_denial(&query, "仅能查询机器人发给你的私聊消息")
    );
}

#[test]
fn restricted_members_cannot_search_another_senders_messages() {
    let suggestion =
        format!("让\"{SENDER_NAME}\"成为群主或者管理员，或改为查询\"{GROUP_ID}\"群里的所有消息");
    let query = format!("{OTHER_SENDER_ID}在{GROUP_ID}群的消息");

    let member = group_tool(Some("member"));
    assert_eq!(
        member
            .permission_denial(Some(GROUP_ID), Some(OTHER_SENDER_ID), None, None, None)
            .unwrap(),
        expected_denial(&query, &suggestion)
    );
}

#[test]
fn an_unresolvable_group_role_cannot_search_another_senders_messages() {
    let tool = group_tool(None);
    let query = format!("{OTHER_SENDER_ID}在{GROUP_ID}群的消息");
    let suggestion =
        format!("让\"{SENDER_NAME}\"成为群主或者管理员，或改为查询\"{GROUP_ID}\"群里的所有消息");
    assert_eq!(
        tool.permission_denial(Some(GROUP_ID), Some(OTHER_SENDER_ID), None, None, None)
            .unwrap(),
        expected_denial(&query, &suggestion)
    );
}

#[test]
fn group_owner_or_admin_can_search_another_senders_messages_in_the_current_group() {
    for role in ["owner", "admin", "Owner", "ADMIN"] {
        let tool = group_tool(Some(role));
        assert!(
            tool.permission_denial(Some(GROUP_ID), Some(OTHER_SENDER_ID), None, None, None)
                .is_none(),
            "role {role}"
        );
    }
}

#[test]
fn another_senders_messages_are_rejected_outside_the_current_group() {
    let owner = group_tool(Some("owner"));

    let query = format!("{OTHER_SENDER_ID}的消息");
    assert_eq!(
        owner.permission_denial(None, Some(OTHER_SENDER_ID), None, None, None).unwrap(),
        expected_denial(&query, "仅能查询在当前群聊的消息")
    );
    let query = format!("{OTHER_SENDER_ID}在{OTHER_GROUP_ID}群的消息");
    assert_eq!(
        owner
            .permission_denial(Some(OTHER_GROUP_ID), Some(OTHER_SENDER_ID), None, None, None)
            .unwrap(),
        expected_denial(&query, "仅能查询在当前群聊的消息")
    );
}

#[test]
fn restricted_senders_can_search_all_messages_in_the_current_group_only() {
    let tool = group_tool(Some("member"));
    assert!(tool.permission_denial(Some(GROUP_ID), None, None, None, None).is_none());

    assert_eq!(
        tool.permission_denial(None, None, None, None, None).unwrap(),
        expected_denial("所有消息", "仅能查询在当前群聊的消息")
    );
    let query = format!("{OTHER_GROUP_ID}群的消息");
    assert_eq!(
        tool.permission_denial(Some(OTHER_GROUP_ID), None, None, None, None).unwrap(),
        expected_denial(&query, "仅能查询在当前群聊的消息")
    );
}

#[test]
fn private_chat_restricts_searches_to_own_and_bot_messages() {
    let tool = private_tool();
    assert!(tool.permission_denial(None, Some(SENDER_ID), None, None, None).is_none());
    assert!(tool
        .permission_denial(Some(GROUP_ID), Some(SENDER_ID), None, None, None)
        .is_none());

    let query = format!("{OTHER_SENDER_ID}的消息");
    assert_eq!(
        tool.permission_denial(None, Some(OTHER_SENDER_ID), None, None, None).unwrap(),
        expected_denial(&query, "仅能查询自己的消息，或机器人发给你的消息")
    );
    assert_eq!(
        tool.permission_denial(None, None, None, None, None).unwrap(),
        expected_denial("所有消息", "仅能查询自己的消息，或机器人发给你的消息")
    );
    let query = format!("{GROUP_ID}群的消息");
    assert_eq!(
        tool.permission_denial(Some(GROUP_ID), None, None, None, None).unwrap(),
        expected_denial(&query, "仅能查询自己的消息，或机器人发给你的消息")
    );
}

#[test]
fn denial_reads_back_the_keyword_and_time_filters() {
    let tool = group_tool(Some("member"));
    let suggestion =
        format!("让\"{SENDER_NAME}\"成为群主或者管理员，或改为查询\"{GROUP_ID}\"群里的所有消息");

    let query = format!(
        "{OTHER_SENDER_ID}在{GROUP_ID}群的消息（内容包含\"原神\"）（时间 2026-01-01 00:00:00 至 2026-01-02 00:00:00）"
    );
    assert_eq!(
        tool.permission_denial(
            Some(GROUP_ID),
            Some(OTHER_SENDER_ID),
            Some("原神"),
            Some("2026-01-01 00:00:00"),
            Some("2026-01-02 00:00:00"),
        )
        .unwrap(),
        expected_denial(&query, &suggestion)
    );

    let query = format!("{OTHER_SENDER_ID}在{GROUP_ID}群的消息（2026-01-01 00:00:00 之后）");
    assert_eq!(
        tool.permission_denial(
            Some(GROUP_ID),
            Some(OTHER_SENDER_ID),
            None,
            Some("2026-01-01 00:00:00"),
            None,
        )
        .unwrap(),
        expected_denial(&query, &suggestion)
    );

    let query = format!("{OTHER_SENDER_ID}在{GROUP_ID}群的消息（2026-01-02 00:00:00 之前）");
    assert_eq!(
        tool.permission_denial(
            Some(GROUP_ID),
            Some(OTHER_SENDER_ID),
            None,
            None,
            Some("2026-01-02 00:00:00"),
        )
        .unwrap(),
        expected_denial(&query, &suggestion)
    );
}

#[test]
fn denial_does_not_hint_at_the_permission_bypass() {
    let tool = group_tool(Some("member"));
    let denial = tool
        .permission_denial(Some(GROUP_ID), Some(OTHER_SENDER_ID), None, None, None)
        .unwrap();
    assert!(!denial.contains("search.search_message"));
    assert!(!denial.contains("等级"));
    assert!(!denial.contains("Planer"));
    assert!(!denial.contains("role self"));
}
