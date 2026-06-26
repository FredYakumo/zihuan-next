use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub content: String,
    pub at_target_list: Option<String>,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
}
