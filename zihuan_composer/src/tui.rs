mod install_form;

use install_form::InstallForm;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use crate::cli::{filter_connections, request};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use serde_json::{json, Value};

const KINDS: &[&str] = &[
    "zihuan",
    "mysql",
    "sqlite",
    "redis",
    "weaviate",
    "elasticsearch",
    "rustfs",
    "napcat",
    "wechat_pad",
];
const NAMES: &[&str] = &[
    "紫幻本体",
    "MySQL",
    "SQLite",
    "Redis",
    "Weaviate",
    "Elasticsearch",
    "RustFS",
    "NapCat",
    "WeChatPadPro",
];
// Footer hints: the left group packs against the left edge, the right group
// against the right edge.
const MAIN_HINTS: (&[&str], &[&str]) =
    (&["↑↓ 选择", "Enter 操作", "a 添加"], &["r 刷新", "? 帮助", "q 退出"]);
// Enter menu entries: label plus the equivalent main-view key, so menu
// execution reuses run_main_action.
const ACTIONS: &[(&str, KeyCode)] = &[
    ("查看详情 (Enter)", KeyCode::Enter),
    ("启动 (s)", KeyCode::Char('s')),
    ("停止 (t)", KeyCode::Char('t')),
    ("卸载 (d)", KeyCode::Char('d')),
    ("编辑部署配置 (f)", KeyCode::Char('f')),
    ("导出选中连接 (o)", KeyCode::Char('o')),
    ("导出全部连接 (x)", KeyCode::Char('x')),
    ("检测运行环境 (e)", KeyCode::Char('e')),
];
const INSTALL_HINTS: (&[&str], &[&str]) = (
    &[
        "↑↓ / Tab 切换字段",
        "←→ / 空格 切换选项",
        "直接输入修改字段",
        "Ctrl+U 清空",
        "F2 显示密码",
    ],
    &["Ctrl+S 提交安装", "Esc 返回组件选择"],
);

// Restore the user's terminal on errors and unwinding as well as normal exits.
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen, Show);
    }
}

enum EditPurpose {
    Config(String),
    Connections,
    Export,
}
enum Mode {
    Main,
    Help,
    Add(usize),
    Actions(usize),
    Install(InstallForm),
    Edit {
        purpose: EditPurpose,
        text: String,
        cursor: usize,
    },
    ExportPath {
        text: String,
        json: String,
    },
    ConfirmDelete(String),
    ConfirmQuit,
}
enum Effect {
    List,
    Show,
    Operation,
    Template,
    Config(String),
    Connections,
    Export(Vec<String>),
    LoginQr(String),
    LoginStatus,
}
struct Job {
    handle: tokio::task::JoinHandle<Result<Value, String>>,
    effect: Effect,
}

struct App {
    url: String,
    token: String,
    client: reqwest::Client,
    components: Vec<Value>,
    selected: ListState,
    mode: Mode,
    draft: Option<Mode>,
    details: String,
    scroll: u16,
    status: String,
    job: Option<Job>,
    task: Option<String>,
    last_poll: Instant,
    login: Option<(String, String)>,
}

impl App {
    fn fetch(&mut self, method: &str, path: &str, body: Option<Value>, effect: Effect) {
        let (client, url, token, method, path) = (
            self.client.clone(),
            self.url.clone(),
            self.token.clone(),
            method.to_owned(),
            path.to_owned(),
        );
        self.job = Some(Job {
            effect,
            handle: tokio::spawn(async move {
                request(&client, &url, &token, &method, &path, body).await
            }),
        });
        self.status = "正在处理…".into();
    }
    fn selected(&self) -> Option<&Value> {
        self.components.get(self.selected.selected().unwrap_or(0))
    }
    fn show_selected(&mut self) {
        self.details = self
            .selected()
            .map(|item| serde_json::to_string_pretty(item).unwrap_or_default())
            .unwrap_or_else(|| "尚无组件，按 a 添加组件。".into());
        self.scroll = 0;
    }
    fn edit(&mut self, purpose: EditPurpose, text: String) {
        self.mode = Mode::Edit { purpose, text, cursor: 0 };
    }
    async fn finish_job(&mut self) {
        if !self.job.as_ref().is_some_and(|job| job.handle.is_finished()) {
            return;
        }
        let job = self.job.take().unwrap();
        let result = job.handle.await.map_err(|error| error.to_string()).and_then(|value| value);
        match result {
            Err(error) => {
                self.status = format!("错误：{error}");
                if let Some(draft) = self.draft.take() {
                    self.mode = draft;
                }
                if matches!(job.effect, Effect::LoginStatus) {
                    self.login = None;
                }
            }
            Ok(mut value) => {
                self.status = "完成".into();
                match job.effect {
                    Effect::List => {
                        self.components = value.as_array().cloned().unwrap_or_default();
                        let index = self
                            .selected
                            .selected()
                            .unwrap_or(0)
                            .min(self.components.len().saturating_sub(1));
                        self.selected.select((!self.components.is_empty()).then_some(index));
                        self.show_selected();
                    }
                    Effect::Template => {
                        match InstallForm::new(value["yaml"].as_str().unwrap_or_default()) {
                            Ok(form) => {
                                self.mode = Mode::Install(form);
                                self.status = "↑↓ / Tab 选择字段，直接输入；←→ 切换选项；Ctrl+U 清空；F2 显示密码；Ctrl+S 提交，Esc 取消".into();
                            }
                            Err(error) => self.status = format!("无法加载安装表单：{error}"),
                        }
                    }
                    Effect::Config(id) => self.edit(
                        EditPurpose::Config(id),
                        value["yaml"].as_str().unwrap_or_default().into(),
                    ),
                    Effect::Connections => self.edit(
                        EditPurpose::Connections,
                        value["yaml"].as_str().unwrap_or_default().into(),
                    ),
                    Effect::Export(ids) => {
                        if !ids.is_empty() {
                            filter_connections(&mut value, &ids);
                        }
                        self.edit(
                            EditPurpose::Export,
                            serde_json::to_string_pretty(&value).unwrap_or_default(),
                        );
                    }
                    Effect::LoginQr(id) => {
                        self.login =
                            Some((id, value["auth_key"].as_str().unwrap_or_default().into()));
                        self.details = serde_json::to_string_pretty(&value).unwrap_or_default();
                        self.scroll = 0;
                        self.status = "打开详情中的 qr_link 扫码；正在轮询登录状态".into();
                    }
                    Effect::LoginStatus => {
                        self.status = value["message"].as_str().unwrap_or("等待扫码登录").into();
                        if matches!(
                            value["status"].as_str(),
                            Some("logged_in" | "expired" | "error")
                        ) {
                            self.login = None;
                        }
                    }
                    Effect::Operation => {
                        self.draft = None;
                        self.details = serde_json::to_string_pretty(&value).unwrap_or_default();
                        if let Some(id) = value["task_id"].as_str() {
                            self.task = Some(id.to_owned());
                        } else {
                            self.fetch("GET", "components", None, Effect::List);
                        }
                    }
                    Effect::Show => {
                        self.details = serde_json::to_string_pretty(&value).unwrap_or_default();
                        if self.task.is_some() {
                            let events = value.as_array().cloned().unwrap_or_default();
                            let finished = events.iter().any(|event| {
                                event["step"] == "finished" || event["status"] == "error"
                            });
                            if finished {
                                self.task = None;
                                self.status =
                                    "任务已结束；按 r 刷新组件列表，PgUp/PgDn 查看日志".into();
                            }
                        }
                    }
                }
            }
        }
    }
    fn key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.task.is_some() || self.job.is_some() {
                self.mode = Mode::ConfirmQuit;
                return false;
            }
            return true;
        }
        match &mut self.mode {
            Mode::Help => match key.code {
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                    self.mode = Mode::Main;
                    self.scroll = 0;
                }
                _ => {}
            },
            Mode::ConfirmQuit => {
                if key.code == KeyCode::Char('y') {
                    return true;
                }
                self.mode = Mode::Main;
            }
            Mode::ConfirmDelete(id) => {
                let id = id.clone();
                self.mode = Mode::Main;
                if key.code == KeyCode::Char('y') {
                    self.fetch("DELETE", &format!("components/{id}"), None, Effect::Operation);
                }
            }
            Mode::ExportPath { text, json } => match key.code {
                KeyCode::Esc => self.mode = Mode::Main,
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(ch) => text.push(ch),
                KeyCode::Enter => match std::fs::write(&*text, &*json) {
                    Ok(()) => {
                        self.status = format!("已保存到 {text}");
                        self.mode = Mode::Main;
                    }
                    Err(error) => self.status = error.to_string(),
                },
                _ => {}
            },
            Mode::Install(form) => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Add(0);
                    self.status = "已取消安装，选择组件类型或按 Esc 返回".into();
                } else {
                    let submit = (key.code == KeyCode::Char('s')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                        || form.key(key);
                    if submit {
                        match form.payload() {
                            Err(error) => self.status = format!("配置无效：{error}"),
                            Ok(body) => {
                                self.draft = Some(std::mem::replace(&mut self.mode, Mode::Main));
                                self.fetch(
                                    "POST",
                                    "components/install",
                                    Some(body),
                                    Effect::Operation,
                                );
                            }
                        }
                    }
                }
            }
            Mode::Edit { purpose, text, cursor } => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Main;
                    return false;
                }
                if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    let result = match purpose {
                        EditPurpose::Export => {
                            serde_json::from_str::<Value>(text).map_err(|error| error.to_string())
                        }
                        _ => Ok(json!({"yaml": text})),
                    };
                    match result {
                        Err(error) => self.status = format!("配置无效：{error}"),
                        Ok(body) => {
                            let (method, path) = match purpose {
                                EditPurpose::Config(id) => {
                                    ("PUT", format!("components/{id}/configuration"))
                                }
                                EditPurpose::Connections => ("PUT", "connections/yaml".into()),
                                EditPurpose::Export => {
                                    self.mode = Mode::ExportPath {
                                        text: "connections.json".into(),
                                        json: text.clone(),
                                    };
                                    return false;
                                }
                            };
                            self.draft = Some(std::mem::replace(&mut self.mode, Mode::Main));
                            self.fetch(method, &path, Some(body), Effect::Operation);
                        }
                    }
                } else {
                    edit_key(text, cursor, key);
                }
            }
            Mode::Add(index) => match key.code {
                KeyCode::Esc => self.mode = Mode::Main,
                KeyCode::Up => *index = (*index + KINDS.len() - 1) % KINDS.len(),
                KeyCode::Down => *index = (*index + 1) % KINDS.len(),
                KeyCode::Enter => {
                    let kind = KINDS[*index];
                    self.mode = Mode::Main;
                    self.fetch("GET", &format!("template/{kind}"), None, Effect::Template);
                }
                _ => {}
            },
            Mode::Main => {
                if key.code == KeyCode::Char('q') {
                    if self.task.is_some() || self.job.is_some() {
                        self.mode = Mode::ConfirmQuit;
                    } else {
                        return true;
                    }
                }
                if key.code == KeyCode::PageDown {
                    self.scroll = self.scroll.saturating_add(10);
                }
                if key.code == KeyCode::PageUp {
                    self.scroll = self.scroll.saturating_sub(10);
                }
                if self.job.is_some() || self.task.is_some() {
                    return false;
                }
                match key.code {
                    KeyCode::Char('?') => {
                        self.mode = Mode::Help;
                        self.scroll = 0;
                    }
                    KeyCode::Down | KeyCode::Up if !self.components.is_empty() => {
                        let current = self.selected.selected().unwrap_or(0);
                        let next = if key.code == KeyCode::Down {
                            (current + 1) % self.components.len()
                        } else {
                            (current + self.components.len() - 1) % self.components.len()
                        };
                        self.selected.select(Some(next));
                        self.show_selected();
                    }
                    KeyCode::Enter if !self.components.is_empty() => self.mode = Mode::Actions(0),
                    _ => self.run_main_action(key.code),
                }
            }
            Mode::Actions(index) => match key.code {
                KeyCode::Esc => self.mode = Mode::Main,
                KeyCode::Up => *index = (*index + ACTIONS.len() - 1) % ACTIONS.len(),
                KeyCode::Down => *index = (*index + 1) % ACTIONS.len(),
                KeyCode::Enter => {
                    let action = ACTIONS[*index].1;
                    self.mode = Mode::Main;
                    self.run_main_action(action);
                }
                _ => {}
            },
        }
        false
    }

    // Execute a main-view action; shared by the direct keybindings and the
    // Enter action menu.
    fn run_main_action(&mut self, key: KeyCode) {
        let item = self.selected().cloned().unwrap_or(Value::Null);
        let id = item["id"].as_str().unwrap_or_default();
        match key {
            KeyCode::Enter => self.show_selected(),
            KeyCode::Char('a') => self.mode = Mode::Add(0),
            KeyCode::Char('r') => self.fetch("GET", "components", None, Effect::List),
            KeyCode::Char('e') => self.fetch("GET", "environment", None, Effect::Show),
            KeyCode::Char('v') => self.fetch("GET", "build-info", None, Effect::Show),
            KeyCode::Char('c') => self.fetch("GET", "connections/yaml", None, Effect::Connections),
            KeyCode::Char('x') => self.fetch("GET", "connections", None, Effect::Export(vec![])),
            KeyCode::Char('o') if !id.is_empty() => {
                let ids = item["connection_ids"]
                    .as_array()
                    .map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                // Empty per-component connections must export an empty list, not all connections.
                if item["connection_ids"].as_array().is_none_or(Vec::is_empty) {
                    self.edit(EditPurpose::Export, "[]".into());
                } else {
                    self.fetch("GET", "connections", None, Effect::Export(ids));
                }
            }
            KeyCode::Char('s') if !id.is_empty() => {
                self.fetch("POST", &format!("components/{id}/enable"), None, Effect::Operation)
            }
            KeyCode::Char('t') if !id.is_empty() => {
                self.fetch("POST", &format!("components/{id}/disable"), None, Effect::Operation)
            }
            KeyCode::Char('d') if !id.is_empty() => self.mode = Mode::ConfirmDelete(id.into()),
            KeyCode::Char('f') if !id.is_empty() => self.fetch(
                "GET",
                &format!("components/{id}/configuration"),
                None,
                Effect::Config(id.into()),
            ),
            KeyCode::Char('m') if !id.is_empty() => {
                self.details = item["extra_install_metadata"]["install_command"]
                    .as_str()
                    .unwrap_or("此组件没有生成的安装命令")
                    .into();
            }
            KeyCode::Char('l') if item["component_type"] == "wechat_pad" => self.fetch(
                "POST",
                "components/wechat_pad/login_qr",
                Some(json!({"component_id": id})),
                Effect::LoginQr(id.into()),
            ),
            _ => {}
        }
    }
}

fn edit_key(text: &mut String, cursor: &mut usize, key: KeyEvent) {
    let byte = text.char_indices().nth(*cursor).map(|(index, _)| index).unwrap_or(text.len());
    match key.code {
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            text.insert(byte, ch);
            *cursor += 1;
        }
        KeyCode::Enter => {
            text.insert(byte, '\n');
            *cursor += 1;
        }
        KeyCode::Backspace if *cursor > 0 => {
            let previous = text.char_indices().nth(*cursor - 1).unwrap().0;
            text.replace_range(previous..byte, "");
            *cursor -= 1;
        }
        KeyCode::Delete if byte < text.len() => {
            text.remove(byte);
        }
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(text.chars().count()),
        KeyCode::Home => {
            *cursor = text[..byte]
                .rfind('\n')
                .map(|index| text[..=index].chars().count())
                .unwrap_or(0)
        }
        KeyCode::End => {
            *cursor += text[byte..]
                .find('\n')
                .map(|index| text[byte..byte + index].chars().count())
                .unwrap_or_else(|| text[byte..].chars().count())
        }
        KeyCode::Up | KeyCode::Down => {
            let lines: Vec<&str> = text.split('\n').collect();
            let row = text[..byte].chars().filter(|ch| *ch == '\n').count();
            let col = text[..byte].rsplit('\n').next().unwrap_or_default().chars().count();
            let next = if key.code == KeyCode::Up {
                row.saturating_sub(1)
            } else {
                (row + 1).min(lines.len() - 1)
            };
            *cursor = lines[..next].iter().map(|line| line.chars().count() + 1).sum::<usize>()
                + col.min(lines[next].chars().count());
        }
        _ => {}
    }
}

fn text_width(text: &str) -> usize {
    ratatui::text::Line::from(text).width()
}

/// Greedily wrap hint items into rows that fit `width`.
fn wrap_hints(groups: (&[&str], &[&str]), width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut row: Vec<&str> = Vec::new();
    let mut row_width = 0;
    for item in groups.0.iter().chain(groups.1.iter()).copied() {
        let item_width = text_width(item);
        if !row.is_empty() && row_width + 2 + item_width > width {
            rows.push(row.join("  "));
            row.clear();
            row_width = 0;
        }
        if !row.is_empty() {
            row_width += 2;
        }
        row_width += item_width;
        row.push(item);
    }
    if !row.is_empty() {
        rows.push(row.join("  "));
    }
    rows
}

// Footer hints per mode; in overlay views Esc always returns to the
// previous view.
fn footer_hints(mode: &Mode) -> (&'static [&'static str], &'static [&'static str]) {
    match mode {
        Mode::Main => MAIN_HINTS,
        Mode::Install(_) => INSTALL_HINTS,
        Mode::Actions(_) => (&["↑↓ 选择", "Enter 执行"], &["Esc 返回"]),
        Mode::Add(_) => (&["↑↓ 选择", "Enter 配置"], &["Esc 返回"]),
        Mode::Help => (&["PgUp / PgDn 滚动"], &["Esc 返回"]),
        Mode::Edit { .. } => (&["Ctrl+S 提交"], &["Esc 取消"]),
        Mode::ExportPath { .. } => (&["Enter 保存"], &["Esc 取消"]),
        Mode::ConfirmDelete(_) => (&["y 确认卸载"], &["其他键取消"]),
        Mode::ConfirmQuit => (&["y 退出"], &["其他键返回"]),
    }
}

/// Build the footer: the left hint group, the status message and the right
/// hint group share one row; when that does not fit, hints wrap and the
/// status takes its own row.
fn footer_text(groups: (&[&str], &[&str]), status: &str, width: usize) -> Text<'static> {
    let left = groups.0.join("  ");
    let right = groups.1.join("  ");
    let left_width = text_width(&left);
    let right_width = text_width(&right);
    let status_width = text_width(status);
    let color = if status.starts_with("错误") {
        Color::Red
    } else {
        Color::Cyan
    };
    let status_span = Span::styled(status.to_owned(), Style::default().fg(color));
    let separator = usize::from(!status.is_empty()) * 2;
    if left_width + right_width + status_width + separator + 2 <= width {
        let mut spans = vec![Span::from(left)];
        if separator > 0 {
            spans.push(Span::from("  "));
            spans.push(status_span);
        }
        let pad = width - left_width - right_width - status_width - separator;
        spans.push(Span::from(" ".repeat(pad)));
        spans.push(Span::from(right));
        return Text::from(Line::from(spans));
    }
    let mut lines: Vec<Line<'static>> =
        wrap_hints(groups, width).into_iter().map(Line::from).collect();
    if !status.is_empty() {
        lines.push(Line::from(vec![status_span]));
    }
    Text::from(lines)
}

pub async fn run(url: &str, token: &str) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("TUI 需要交互终端。请使用 --help 或 list、environment 等子命令。".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    let mut app = App {
        url: url.into(),
        token: token.into(),
        client,
        components: vec![],
        selected: ListState::default(),
        mode: Mode::Main,
        draft: None,
        details: String::new(),
        scroll: 0,
        status: String::new(),
        job: None,
        task: None,
        last_poll: Instant::now(),
        login: None,
    };
    let _guard = TerminalGuard;
    enable_raw_mode().map_err(|error| error.to_string())?;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)
        .map_err(|error| error.to_string())?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(|error| error.to_string())?;
    app.fetch("GET", "components", None, Effect::List);
    loop {
        app.finish_job().await;
        if app.job.is_none() && app.last_poll.elapsed() >= Duration::from_millis(500) {
            if let Some(id) = app.task.clone() {
                app.fetch("GET", &format!("tasks/{id}"), None, Effect::Show);
                app.last_poll = Instant::now();
            } else if app.last_poll.elapsed() >= Duration::from_secs(2)
                && matches!(app.mode, Mode::Main)
            {
                if let Some((id, auth_key)) = app.login.clone() {
                    app.fetch(
                        "POST",
                        "components/wechat_pad/login_status",
                        Some(json!({"component_id": id, "auth_key": auth_key})),
                        Effect::LoginStatus,
                    );
                    app.last_poll = Instant::now();
                }
            }
        }
        terminal.draw(|frame| {
            let hints = footer_hints(&app.mode);
            let footer = footer_text(hints, app.status.as_str(), frame.area().width as usize);
            let areas = Layout::vertical([Constraint::Length(3), Constraint::Min(5), Constraint::Length(footer.lines.len() as u16)]).split(frame.area());
            frame.render_widget(Paragraph::new(format!("Zihuan Composer v{} · {} · {}", env!("CARGO_PKG_VERSION"), env!("ZIHUAN_BUILD_VERSION"), env!("ZIHUAN_BUILD_TYPE"))).block(Block::bordered()).style(Style::default().fg(Color::Cyan)), areas[0]);
            let block = Block::default().borders(Borders::ALL);
            match &mut app.mode {
                Mode::Help => frame.render_widget(Paragraph::new("↑ / ↓：选择组件，Enter：打开操作菜单（↑↓ 选择，Enter 执行，Esc 返回）\n操作菜单：详情 / 启动 / 停止 / 卸载 / 配置 / 导出 / 环境检测\na：添加组件，选择类型后逐项填写安装字段\n安装表单：Tab / ↑↓ 选择字段，←→ 切换选项\nCtrl+U 清空字段，F2 显示密码，Ctrl+S 提交\ns：启动，t：停止，d：卸载（y 确认）\nf：编辑部署 YAML，c：编辑保存的连接 YAML\no：导出选中组件的连接，x：导出全部连接\n导出先编辑临时 JSON，Ctrl+S 后填写保存路径\nm：查看生成的安装命令\ne：检测运行环境，r：刷新列表，v：版本\nl：获取 WeChatPadPro 二维码及链接，自动轮询登录状态\n编辑器：方向键移动，Home / End 到行首尾\nCtrl+S：提交编辑，Esc：取消编辑\nPgUp / PgDn：滚动详情及任务日志\nq / Ctrl+C：退出；任务运行中需要确认\n\n按 Esc、q 或 ? 返回").block(block.title("快捷键帮助")).wrap(Wrap { trim: false }).scroll((app.scroll, 0)), areas[1]),
                Mode::Edit { purpose, text, cursor } => {
                    let title = match purpose { EditPurpose::Config(_) => "部署配置 YAML", EditPurpose::Connections => "连接配置 YAML", EditPurpose::Export => "导出连接 · 临时编辑 JSON" };
                    let mut display = text.clone();
                    let byte = display.char_indices().nth(*cursor).map(|(index, _)| index).unwrap_or(display.len());
                    display.insert(byte, '▏');
                    let row = text[..byte].chars().filter(|ch| *ch == '\n').count();
                    let scroll = row.saturating_sub(areas[1].height.saturating_sub(4) as usize) as u16;
                    let column = ratatui::text::Line::from(text[..byte].rsplit('\n').next().unwrap_or_default()).width();
                    let horizontal = column.saturating_sub(areas[1].width.saturating_sub(4) as usize).min(u16::MAX as usize) as u16;
                    frame.render_widget(Paragraph::new(display).block(block.title(format!("{title} · Ctrl+S 提交 / Esc 取消"))).scroll((scroll, horizontal)), areas[1]);
                }
                Mode::Install(form) => form.draw(frame, areas[1]),
                Mode::Add(index) => {
                    let items: Vec<ListItem> = NAMES.iter().map(|name| ListItem::new(*name)).collect();
                    let mut state = ListState::default().with_selected(Some(*index));
                    frame.render_stateful_widget(List::new(items).block(block.title("添加组件 · ↑↓ 选择 / Enter 配置 / Esc 返回")).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)).highlight_symbol("› "), areas[1], &mut state);
                }
                Mode::Actions(index) => {
                    let items: Vec<ListItem> = ACTIONS.iter().map(|(label, _)| ListItem::new(*label)).collect();
                    let mut state = ListState::default().with_selected(Some(*index));
                    let name = app.components.get(app.selected.selected().unwrap_or(0)).and_then(|item| item["name"].as_str()).unwrap_or("组件");
                    frame.render_stateful_widget(List::new(items).block(block.title(format!("{name} · ↑↓ 选择 / Enter 执行 / Esc 返回"))).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)).highlight_symbol("› "), areas[1], &mut state);
                }
                Mode::ExportPath { text, .. } => frame.render_widget(Paragraph::new(format!("保存路径：{text}▏\nEnter 保存 / Esc 取消")).block(block.title("保存连接 JSON")), areas[1]),
                Mode::ConfirmDelete(id) => frame.render_widget(Paragraph::new(format!("确认卸载 {id}？安装器可能删除其管理的数据。\ny 确认 / 其他键取消")).block(block.title("卸载组件")), areas[1]),
                Mode::ConfirmQuit => frame.render_widget(Paragraph::new("操作仍在进行，退出会中断任务。\ny 退出 / 其他键返回").block(block.title("退出 Composer")), areas[1]),
                Mode::Main => {
                    let panes = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(areas[1]);
                    let items: Vec<ListItem> = app.components.iter().map(|item| ListItem::new(format!("{} [{}]", item["name"].as_str().unwrap_or("?"), item["status"].as_str().unwrap_or("?")))).collect();
                    frame.render_stateful_widget(List::new(items).block(block.clone().title("组件管理")).highlight_style(Style::default().fg(Color::Cyan)).highlight_symbol("› "), panes[0], &mut app.selected);
                    frame.render_widget(Paragraph::new(app.details.as_str()).block(block.title("详情 / 运行环境 / 任务进度")).wrap(Wrap { trim: false }).scroll((app.scroll, 0)), panes[1]);
                }
            }
            frame.render_widget(Paragraph::new(footer).style(Style::default().fg(Color::Gray)), areas[2]);
        }).map_err(|error| error.to_string())?;
        if event::poll(Duration::from_millis(50)).map_err(|error| error.to_string())? {
            match event::read().map_err(|error| error.to_string())? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press && app.key(key) {
                        break;
                    }
                }
                Event::Paste(paste) => match &mut app.mode {
                    Mode::Edit { text, cursor, .. } => {
                        let byte = text
                            .char_indices()
                            .nth(*cursor)
                            .map(|(index, _)| index)
                            .unwrap_or(text.len());
                        text.insert_str(byte, &paste);
                        *cursor += paste.chars().count();
                    }
                    Mode::Install(form) => form.paste(&paste),
                    Mode::ExportPath { text, .. } => text.push_str(&paste),
                    _ => {}
                },
                _ => {}
            }
        }
        tokio::task::yield_now().await;
    }
    if let Some(job) = app.job {
        job.handle.abort();
    }
    Ok(())
}
