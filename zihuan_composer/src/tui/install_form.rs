use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};
use serde_json::Value;

use crate::api::components::InstallComponentRequest;

enum Input {
    Text,
    Password,
    Number(u64),
    Toggle,
    Choice(Vec<(&'static str, &'static str)>),
}
#[derive(Clone, Copy)]
enum When {
    Always,
    Docker,
    Native,
    Public,
    Target,
}
struct Field {
    path: String,
    label: &'static str,
    input: Input,
    value: String,
    when: When,
}

pub(super) struct InstallForm {
    request: Value,
    fields: Vec<Field>,
    selected: usize,
    cursor: usize,
    reveal: bool,
    list: ListState,
}

impl InstallForm {
    pub fn new(yaml: &str) -> Result<Self, String> {
        let request: InstallComponentRequest =
            serde_yaml::from_str(yaml).map_err(|error| error.to_string())?;
        let mut form = Self {
            request: serde_json::to_value(request).map_err(|error| error.to_string())?,
            fields: vec![],
            selected: 0,
            cursor: 0,
            reveal: false,
            list: ListState::default(),
        };
        let kind = form.request["component_type"].as_str().unwrap_or_default().to_owned();
        form.add("/name", "组件名称（必填）", Input::Text, When::Always);
        form.add("/version", "版本", Input::Text, When::Always);
        let methods = match kind.as_str() {
            "zihuan" => vec![("docker", "本机 Docker"), ("native", "本机 native 安装")],
            "sqlite" => vec![("embedded", "内嵌数据库")],
            "wechat_pad" => vec![("docker", "本机 Docker"), ("command_docker", "Docker 安装命令")],
            "napcat" => vec![
                ("docker", "本机 Docker"),
                ("native", "本机 native 安装"),
                ("command_docker", "Docker 安装命令"),
                ("command_native", "native 安装命令"),
            ],
            _ => vec![
                ("docker", "本机 Docker"),
                ("binary", "本机包管理器"),
                ("command_docker", "Docker 安装命令"),
                ("command_binary", "包管理器安装命令"),
            ],
        };
        form.add("/install_method", "安装方式（← → 切换）", Input::Choice(methods), When::Always);
        if kind == "zihuan" {
            let prefix = "/extra_install_metadata";
            for (key, label, input, when) in [
                ("image", "Docker 镜像", Input::Text, When::Docker),
                (
                    "source",
                    "发布目录或发布包 URL（留空下载官方发布包）",
                    Input::Text,
                    When::Native,
                ),
                ("http_proxy", "下载代理（可留空）", Input::Text, When::Native),
                ("host", "监听地址", Input::Text, When::Always),
                ("port", "端口", Input::Number(65535), When::Always),
            ] {
                form.add(&format!("{prefix}/{key}"), label, input, when);
            }
        } else {
            if kind != "sqlite" {
                form.add(
                    "/detailed_config/expose_public_access",
                    "暴露公网访问",
                    Input::Toggle,
                    When::Always,
                );
                form.add(
                    "/detailed_config/use_target_machine_address",
                    "使用目标机器地址",
                    Input::Toggle,
                    When::Public,
                );
                form.add(
                    "/detailed_config/target_machine_address",
                    "目标机器地址",
                    Input::Text,
                    When::Target,
                );
            }
            let section = match kind.as_str() {
                "mysql" => Some("relational"),
                "redis" => Some("redis"),
                "rustfs" => Some("rustfs"),
                "weaviate" | "elasticsearch" => Some("search"),
                _ => None,
            };
            if let Some(section) = section {
                let prefix = format!("/detailed_config/{section}");
                for (key, label) in [
                    ("image", "Docker 镜像"),
                    ("container_name", "容器名称"),
                    ("data_dir", "数据目录"),
                    ("restart_policy", "重启策略"),
                ] {
                    form.add(
                        &format!("{prefix}/deployment/{key}"),
                        label,
                        Input::Text,
                        When::Docker,
                    );
                }
                form.add(
                    &format!("{prefix}/deployment/port"),
                    "端口",
                    Input::Number(65535),
                    When::Always,
                );
                let definitions = match kind.as_str() {
                    "mysql" => vec![
                        ("host", "主机", Input::Text),
                        ("username", "用户名", Input::Text),
                        ("password", "密码", Input::Password),
                        ("database", "数据库", Input::Text),
                        ("max_connections", "最大连接数", Input::Number(u32::MAX as u64)),
                        ("acquire_timeout_secs", "获取连接超时（秒）", Input::Number(u64::MAX)),
                    ],
                    "redis" => vec![
                        ("url", "Redis URL", Input::Text),
                        ("username", "用户名", Input::Text),
                        ("password", "密码", Input::Password),
                    ],
                    "rustfs" => vec![
                        ("endpoint", "Endpoint", Input::Text),
                        ("bucket", "Bucket", Input::Text),
                        ("region", "Region", Input::Text),
                        ("access_key", "Access Key", Input::Text),
                        ("secret_key", "Secret Key", Input::Password),
                        ("public_base_url", "公开访问 URL", Input::Text),
                        ("path_style", "使用 path-style", Input::Toggle),
                    ],
                    "weaviate" => vec![
                        ("base_url", "Base URL", Input::Text),
                        ("api_key", "API Key", Input::Password),
                        ("vector_dimensions", "向量维度", Input::Number(u32::MAX as u64)),
                    ],
                    _ => vec![
                        ("base_url", "Base URL", Input::Text),
                        ("username", "用户名", Input::Text),
                        ("password", "密码", Input::Password),
                        ("vector_dimensions", "向量维度", Input::Number(u32::MAX as u64)),
                    ],
                };
                for (key, label, input) in definitions {
                    form.add(&format!("{prefix}/{key}"), label, input, When::Always);
                }
            }
        }
        form.reset_cursor();
        Ok(form)
    }
    fn add(&mut self, path: &str, label: &'static str, input: Input, when: When) {
        let value = match self.request.pointer(path) {
            Some(Value::String(value)) => value.clone(),
            Some(Value::Null) | None => String::new(),
            Some(value) => value.to_string(),
        };
        self.fields.push(Field {
            path: path.into(),
            label,
            input,
            value,
            when,
        });
    }
    fn value(&self, path: &str) -> &str {
        self.fields
            .iter()
            .find(|field| field.path == path)
            .map(|field| field.value.as_str())
            .unwrap_or_default()
    }
    fn visible(&self) -> Vec<usize> {
        let docker = self.value("/install_method").contains("docker");
        let public = self.value("/detailed_config/expose_public_access") == "true";
        let target = self.value("/detailed_config/use_target_machine_address") == "true";
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, field)| match field.when {
                When::Always => true,
                When::Docker => docker,
                When::Native => !docker,
                When::Public => public,
                When::Target => public && target,
            })
            .map(|(index, _)| index)
            .collect()
    }
    fn reset_cursor(&mut self) {
        self.cursor = self
            .visible()
            .get(self.selected)
            .map(|index| self.fields[*index].value.chars().count())
            .unwrap_or(0);
        self.reveal = false;
    }
    pub fn payload(&self) -> Result<Value, String> {
        let mut request = self.request.clone();
        for index in self.visible() {
            let field = &self.fields[index];
            let value = match &field.input {
                Input::Number(max) => {
                    let number = field
                        .value
                        .parse::<u64>()
                        .ok()
                        .filter(|number| *number >= 1 && number <= max)
                        .ok_or_else(|| format!("{}需要填写 1–{max} 的整数", field.label))?;
                    Value::from(number)
                }
                Input::Toggle => Value::Bool(field.value == "true"),
                _ => Value::String(field.value.clone()),
            };
            *request
                .pointer_mut(&field.path)
                .ok_or_else(|| format!("缺少字段：{}", field.label))? = value;
        }
        if request["name"].as_str().unwrap_or_default().trim().is_empty() {
            return Err("请填写组件名称".into());
        }
        if request["detailed_config"].is_object() {
            request["detailed_config"]["install_method"] = Value::String(
                if request["install_method"].as_str().unwrap_or_default().contains("docker") {
                    "docker"
                } else {
                    "binary"
                }
                .into(),
            );
        }
        let typed: InstallComponentRequest =
            serde_json::from_value(request).map_err(|error| error.to_string())?;
        serde_json::to_value(typed).map_err(|error| error.to_string())
    }
    /// Returns true when the user activates the submit button.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let visible = self.visible();
        if key.code == KeyCode::Enter && self.selected == visible.len() {
            return true;
        }
        match key.code {
            KeyCode::Up | KeyCode::BackTab => {
                self.selected = if self.selected == 0 {
                    visible.len()
                } else {
                    self.selected - 1
                };
                self.reset_cursor();
            }
            KeyCode::Down | KeyCode::Tab | KeyCode::Enter => {
                self.selected = (self.selected + 1) % (visible.len() + 1);
                self.reset_cursor();
            }
            _ => {
                if let Some(index) = visible.get(self.selected) {
                    let field = &mut self.fields[*index];
                    match &field.input {
                        Input::Choice(choices)
                            if matches!(
                                key.code,
                                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                            ) =>
                        {
                            let current = choices
                                .iter()
                                .position(|(value, _)| *value == field.value)
                                .unwrap_or(0);
                            let next = if key.code == KeyCode::Left {
                                (current + choices.len() - 1) % choices.len()
                            } else {
                                (current + 1) % choices.len()
                            };
                            field.value = choices[next].0.into();
                        }
                        Input::Toggle
                            if matches!(
                                key.code,
                                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                            ) =>
                        {
                            field.value = (field.value != "true").to_string()
                        }
                        Input::Choice(_) | Input::Toggle => {}
                        _ => {
                            if key.code == KeyCode::Char('u')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                field.value.clear();
                                self.cursor = 0;
                            } else if key.code == KeyCode::F(2) {
                                self.reveal = !self.reveal;
                            } else {
                                super::edit_key(&mut field.value, &mut self.cursor, key);
                            }
                        }
                    }
                }
            }
        }
        false
    }
    pub fn paste(&mut self, paste: &str) {
        if let Some(index) = self.visible().get(self.selected) {
            let field = &mut self.fields[*index];
            if matches!(field.input, Input::Text | Input::Password | Input::Number(_)) {
                let byte = field
                    .value
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(index, _)| index)
                    .unwrap_or(field.value.len());
                let paste = paste.replace(['\r', '\n'], "");
                field.value.insert_str(byte, &paste);
                self.cursor += paste.chars().count();
            }
        }
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        let kind = self.request["component_type"].as_str().unwrap_or_default();
        let name = super::KINDS
            .iter()
            .position(|value| *value == kind)
            .map(|index| super::NAMES[index])
            .unwrap_or(kind);
        let visible = self.visible();
        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .map(|(row, index)| {
                let field = &self.fields[*index];
                let mut value = match &field.input {
                    Input::Choice(choices) => choices
                        .iter()
                        .find(|(value, _)| *value == field.value)
                        .map(|(_, label)| format!("← {label} →"))
                        .unwrap_or_default(),
                    Input::Toggle => if field.value == "true" {
                        "[✓] 开启"
                    } else {
                        "[ ] 关闭"
                    }
                    .into(),
                    Input::Password if !(self.reveal && row == self.selected) => {
                        "•".repeat(field.value.chars().count())
                    }
                    _ => field.value.clone(),
                };
                if row == self.selected && !matches!(field.input, Input::Toggle | Input::Choice(_))
                {
                    let byte = value
                        .char_indices()
                        .nth(self.cursor)
                        .map(|(index, _)| index)
                        .unwrap_or(value.len());
                    value.insert(byte, '▏');
                    // Keep the active cursor visible for long URLs, paths and credentials.
                    let width = area.width.saturating_sub(8) as usize;
                    let before = ratatui::text::Line::from(&value[..byte]).width();
                    if before >= width {
                        let mut skip = before.saturating_sub(width.saturating_sub(1));
                        value = value
                            .chars()
                            .skip_while(|ch| {
                                if skip > 0 {
                                    skip = skip.saturating_sub(
                                        ratatui::text::Span::raw(ch.to_string()).width(),
                                    );
                                    true
                                } else {
                                    false
                                }
                            })
                            .collect();
                    }
                }
                if value.is_empty() {
                    value = "（可留空）".into();
                }
                ListItem::new(format!("{}\n  {}", field.label, value))
            })
            .chain(std::iter::once(ListItem::new(
                if self.value("/install_method").starts_with("command_") {
                    "[ 生成安装命令 ]"
                } else {
                    "[ 开始安装 ]"
                },
            )))
            .collect();
        self.list.select(Some(self.selected));
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::bordered().title(format!("添加组件 · {name} · 填写安装字段")))
                .highlight_style(Style::default().fg(Color::Cyan))
                .highlight_symbol("› "),
            area,
            &mut self.list,
        );
    }
}
