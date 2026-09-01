use serde::{Deserialize, Serialize};

use crate::ims_bot_adapter::models::event_model::{MessageEvent, Sender as EventSender};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FriendSender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(default)]
    pub card: String,
    pub role: Option<String>,
}

impl From<&EventSender> for FriendSender {
    fn from(value: &EventSender) -> Self {
        Self {
            user_id: value.user_id,
            nickname: value.nickname.clone(),
            card: value.card.clone(),
            role: value.role.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupSender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(default)]
    pub card: String,
    pub role: Option<String>,
    pub group_id: i64,
    #[serde(default)]
    pub group_name: Option<String>,
}

impl GroupSender {
    pub fn from_event_sender(
        sender: &EventSender,
        group_id: i64,
        group_name: Option<String>,
    ) -> Self {
        Self {
            user_id: sender.user_id,
            nickname: sender.nickname.clone(),
            card: sender.card.clone(),
            role: sender.role.clone(),
            group_id,
            group_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sender {
    Friend(FriendSender),
    Group(GroupSender),
}

impl Sender {
    pub fn from_message_event(event: &MessageEvent) -> Option<Self> {
        if event.is_group_message {
            Some(Sender::Group(GroupSender::from_event_sender(
                &event.sender,
                event.group_id?,
                event.group_name.clone(),
            )))
        } else {
            Some(Sender::Friend(FriendSender::from(&event.sender)))
        }
    }

    pub fn target_id(&self) -> String {
        match self {
            Sender::Friend(sender) => sender.user_id.to_string(),
            Sender::Group(sender) => sender.group_id.to_string(),
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Sender::Group(_))
    }
}
