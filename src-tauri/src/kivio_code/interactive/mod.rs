//! 交互模式 —— 事件循环 + 输入线程 + 差分渲染协调 + agent loop 接线（Phase 5b）。
//!
//! Phase 4 的 TUI 库（差分渲染 [`Tui`]、组件树、键解码）接到真实终端，跑一个事件循环；Phase 5b 在此
//! 之上把 [`run_agent_loop`](crate::chat::agent::run_agent_loop) 接进来：提交一条消息会在 tokio
//! runtime 上 **后台跑一整轮 agent**，流式 / 工具记录 / 完成事件通过 [`AgentUiEvent`] 通道回到本
//! 事件循环，折叠进 [`App`] 并差分重绘。
//!
//! ## 三路事件汇入一个循环
//! 1. **输入线程**：一条专用 OS 线程在 raw 模式下阻塞 `read` stdin 原始字节，喂给 [`StdinBuffer`]，
//!    把切出的完整序列 / 粘贴段通过 [`mpsc`](std::sync::mpsc) 发到主循环（[`InputEvent`]）。
//! 2. **agent-event 通道**：后台 agent 任务通过 [`InteractiveAgentHost`] 把 [`AgentUiEvent`] 发到
//!    第二条 mpsc；主循环在同一个 `recv_timeout` tick 里 drain 它。
//! 3. **resize**：`recv_timeout` 超时分支轮询 `crossterm::terminal::size()`，变化则全量重绘。
//!
//! ## 一轮 agent turn 的生命周期
//! `AppEffect::Submitted(text)` → 把 user 消息持久化进 session、累积进 `runtime_messages` →
//! 取一个新 generation 建 [`RunCancel`] → 在 tokio runtime 上 `spawn` 一个任务跑 `run_agent_loop`
//! （host = [`InteractiveAgentHost`]，executor = [`CliToolExecutor`]）→ 主循环进入 `Generating`，
//! drain `AgentUiEvent` 重绘 → 任务完成把 [`AgentRunResult`] 通过结果通道送回 → 主循环持久化
//! assistant 消息 + 工具调用、累积 `runtime_messages`、刷新 footer usage，回到 `Idle`。
//!
//! ## 取消 / 多轮
//! - **取消**：Esc / generating 中 Ctrl+C → `AppEffect::Cancel` → 翻 [`RunCancel`]；loop 的
//!   `is_generation_active` 转 false，在下一个检查点停并返回 `Err("cancelled")`。
//! - **多轮**：`runtime_messages` 在 [`TurnRuntime`] 里跨轮累积，每次新提交都带上完整上下文。

pub mod agent_host;
pub mod app;
pub mod slash;

pub use agent_host::{AgentUiEvent, Generations, InteractiveAgentHost, RunCancel};
pub use app::{App, AppEffect, AppMode, ToolCard, ToolCardPlaceholder};

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::chat::agent::run_agent_loop;
use crate::chat::agent::types::AgentRunResult;
use crate::chat::types::{ToolCallRecord, ToolCallStatus};
use crate::kivio_code::executor::CliToolExecutor;
use crate::kivio_code::session::{Session, SessionRecord};
use crate::kivio_code::{build_app_state, load_settings_from_disk, TurnAssembly};
use crate::state::AppState;

use super::tui::render::{Component, Tui};
use super::tui::stdin_buffer::StdinBuffer;
use super::tui::terminal::{CrosstermTerminal, RawModeGuard, Terminal};

/// 输入线程发给主循环的事件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// 一段完整的输入序列（一个按键 / 转义序列的原始字节串）。
    Key(String),
    /// 一段 bracketed paste 的内容。
    Paste(String),
    /// 终端 resize（携带新尺寸）。
    Resize(u16, u16),
    /// stdin 已 EOF / 关闭，输入线程即将退出。
    Eof,
}

/// 渲染一帧：清掉旧子组件，挂一个一次性的 [`AppFrame`] 组件，调用差分渲染器。
struct AppFrame {
    lines: Vec<String>,
}

impl Component for AppFrame {
    fn render(&mut self, _width: u16) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }
}

/// 交互模式的运行选项。
pub struct InteractiveOptions {
    /// 已折叠 home→`~` 的 cwd 展示串。
    pub cwd_display: String,
    /// 形如 `provider:model` 的模型展示串。
    pub model: String,
}

/// 后台 agent 任务完成后送回主循环的结果（连同它跑在哪个 generation，便于丢弃过期任务）。
struct TurnDone {
    generation: u64,
    result: Result<AgentRunResult, String>,
    /// 这一轮的 assistant 消息 id（finalize / 持久化用）。
    message_id: String,
}

/// 交互会话的 agent 运行时上下文：跨轮持有 tokio runtime handle、`AppState`、本轮配置装配
/// [`TurnAssembly`]、cwd、generation 计数、累积的 `runtime_messages` 与 JSONL session。
///
/// 把「提交 → spawn agent → 收结果 → 持久化 + 累积上下文」的逻辑收进这里，让 [`run_loop`] 只负责
/// 事件分发，且这套逻辑（除真实 spawn 外）可被单测覆盖（见 `tests`）。
struct TurnRuntime {
    handle: tokio::runtime::Handle,
    state: Arc<AppState>,
    assembly: Arc<TurnAssembly>,
    cwd: PathBuf,
    timeout_ms: u64,
    /// 单调 generation 源：每次提交取下一个，过期的后台任务因 generation 不匹配被忽略。
    generations: Generations,
    /// 当前在跑的取消令牌（None = 空闲）。
    current: Option<RunCancel>,
    /// 这一轮分配的 assistant 消息 id（流式事件用同一 id 定位）。
    current_message_id: Option<String>,
    /// 跨轮累积的 runtime messages（system + 历次 user/assistant/tool）。
    runtime_messages: Vec<Value>,
    /// 持久化用的 JSONL session（best-effort：写失败仅记一条通知，不中断）。
    session: Option<Session>,
    /// 已写进 session 的工具调用 id（避免一个 record 多状态多次落盘）。
    persisted_tool_calls: std::collections::HashSet<String>,
    /// agent 任务把 [`AgentUiEvent`] 发到这里的 Sender（每轮新建一对，clone 给 host）。
    turn_done_tx: Sender<TurnDone>,
}

impl TurnRuntime {
    /// 是否有一轮在跑。
    fn is_generating(&self) -> bool {
        self.current.is_some()
    }

    /// 起一轮 agent turn：把 user 消息持久化 + 累积进 runtime_messages，新建 generation/cancel，
    /// 在 tokio runtime 上 spawn 跑 `run_agent_loop`，事件经 `agent_tx` 回到主循环。
    fn begin_turn(&mut self, text: String, agent_tx: &Sender<AgentUiEvent>) {
        // 持久化 user 消息（best-effort）。
        self.append_session(SessionRecord::Message {
            id: String::new(),
            parent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            role: "user".to_string(),
            content: text.clone(),
        });

        // 累积进上下文。
        self.runtime_messages
            .push(json!({ "role": "user", "content": text }));

        let generation = self.generations.next();
        let cancel = RunCancel::new(generation);
        self.current = Some(cancel.clone());
        let message_id = format!("kivio-code-msg-{generation}");
        self.current_message_id = Some(message_id.clone());

        let host = InteractiveAgentHost::new(agent_tx.clone(), cancel);
        let state = self.state.clone();
        let assembly = self.assembly.clone();
        let messages = self.runtime_messages.clone();
        let cwd_root = self.cwd.to_string_lossy().into_owned();
        let http = self.state.http.clone();
        let timeout_ms = self.timeout_ms;
        let done_tx = self.turn_done_tx.clone();
        let run_message_id = message_id.clone();

        self.handle.spawn(async move {
            let executor = CliToolExecutor::new(vec![cwd_root], http, timeout_ms);
            // Build the borrowing config inside the task body so the borrows of the
            // owned `state`/`assembly` Arcs live exactly as long as the loop call.
            let config = assembly.into_config(
                &state,
                "kivio-code".to_string(),
                format!("kivio-code-run-{generation}"),
                run_message_id.clone(),
                generation,
                messages,
            );
            let result = run_agent_loop(config, &host, &executor).await;
            let _ = done_tx.send(TurnDone {
                generation,
                result,
                message_id: run_message_id,
            });
        });
    }

    /// 请求取消当前轮（翻 cancel flag；loop 在下个检查点停）。
    fn request_cancel(&self) {
        if let Some(cancel) = &self.current {
            cancel.cancel();
        }
    }

    /// 处理一轮结束：忽略过期 generation；否则把 assistant 消息 + 工具调用持久化、累积进
    /// runtime_messages，刷新 footer usage，回到 Idle。返回 footer usage 摘要（None = 不变）。
    fn finish_turn(&mut self, done: TurnDone, app: &mut App) {
        // 过期任务（已被取消并被新一轮取代）直接丢弃。
        let live = self
            .current
            .as_ref()
            .map(|c| c.generation() == done.generation)
            .unwrap_or(false);
        if !live {
            return;
        }
        self.current = None;
        self.current_message_id = None;

        match done.result {
            Ok(result) => {
                // finalize 助手消息（loop 已发过 Done；这里兜底，幂等）。
                app.apply_agent_event(AgentUiEvent::Done {
                    message_id: done.message_id.clone(),
                    reason: result.stream_outcome.clone(),
                });
                self.persist_turn_records(&result);
                self.accumulate_runtime_messages(&result);
                app.set_usage(format_usage(result.usage.as_ref()));
            }
            Err(err) => {
                if err == "cancelled" {
                    app.apply_agent_event(AgentUiEvent::Done {
                        message_id: done.message_id.clone(),
                        reason: "cancelled".to_string(),
                    });
                    app.push_notice("Run cancelled.");
                } else {
                    app.apply_agent_event(AgentUiEvent::Done {
                        message_id: done.message_id.clone(),
                        reason: "error".to_string(),
                    });
                    app.push_notice(format!("Error: {err}"));
                }
            }
        }
        app.set_mode(AppMode::Idle);
    }

    /// 把这一轮的 assistant 消息 + 工具调用/结果落盘（best-effort）。
    fn persist_turn_records(&mut self, result: &AgentRunResult) {
        for record in &result.tool_records {
            self.persist_tool_record(record);
        }
        if !result.content.trim().is_empty() {
            self.append_session(SessionRecord::Message {
                id: String::new(),
                parent_id: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
                role: "assistant".to_string(),
                content: result.content.clone(),
            });
        }
    }

    /// 把这一轮产生的 provider-agnostic transcript（含 assistant tool_calls / tool 结果）累积进
    /// runtime_messages，使下一轮带上完整上下文。`api_messages` 是 OpenAI 兼容的隐藏消息序列。
    fn accumulate_runtime_messages(&mut self, result: &AgentRunResult) {
        if !result.api_messages.is_empty() {
            self.runtime_messages
                .extend(result.api_messages.iter().cloned());
        } else if !result.content.trim().is_empty() {
            self.runtime_messages
                .push(json!({ "role": "assistant", "content": result.content }));
        }
    }

    /// 持久化一条工具调用 + 结果（一个 call_id 只落一次，取其终态）。
    fn persist_tool_record(&mut self, record: &ToolCallRecord) {
        // 仅在终态落盘，且每个 call_id 只落一次。
        if matches!(record.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
            return;
        }
        if !self.persisted_tool_calls.insert(record.id.clone()) {
            return;
        }
        let arguments = serde_json::from_str::<Value>(&record.arguments)
            .unwrap_or_else(|_| Value::String(record.arguments.clone()));
        self.append_session(SessionRecord::ToolCall {
            id: String::new(),
            parent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            call_id: record.id.clone(),
            name: record.name.clone(),
            arguments,
        });
        let is_error = matches!(record.status, ToolCallStatus::Error);
        let content = record
            .error
            .clone()
            .or_else(|| record.result_preview.clone())
            .unwrap_or_default();
        self.append_session(SessionRecord::ToolResult {
            id: String::new(),
            parent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            call_id: record.id.clone(),
            name: record.name.clone(),
            content,
            is_error,
        });
    }

    /// best-effort 追加一条 session record（无 session 或写失败时静默忽略——持久化不应中断交互）。
    fn append_session(&mut self, record: SessionRecord) {
        if let Some(session) = self.session.as_mut() {
            let _ = session.append(record);
        }
    }
}

/// 把 [`ModelUsage`](crate::chat::model::ModelUsage) 折成 footer 摘要（`<in> in · <out> out`）。
fn format_usage(usage: Option<&crate::chat::model::ModelUsage>) -> Option<String> {
    let usage = usage?;
    let input = usage.input_tokens?;
    let output = usage.output_tokens.unwrap_or(0);
    Some(format!("{} in · {} out", human_tokens(input), human_tokens(output)))
}

/// 把 token 数折成 `1.2k` 风格的短串。
fn human_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// 启动交互模式，阻塞直到用户退出。**需要一个真实 TTY**（调用方在 bin 里已判断 stdin/stdout 是 TTY）。
///
/// 整个生命周期内 raw 模式经 [`RawModeGuard`] 管理，正常返回或 panic 均会还原终端。自建多线程
/// tokio runtime（后台跑 agent 任务）；settings/cwd/provider 从磁盘 + 进程环境解析（与 print 模式
/// 同源），因此无需调用方再传。
pub fn run(options: InteractiveOptions) -> std::io::Result<()> {
    let _guard = RawModeGuard::enter()?;

    let terminal = CrosstermTerminal::new();
    let (cols, rows) = (terminal.columns(), terminal.rows());

    let cwd_display = options.cwd_display.clone();
    let mut app = App::new(cwd_display, options.model.clone());
    app.set_terminal_rows(rows);

    // 输入线程：raw stdin → StdinBuffer → InputEvent channel。
    let (tx, rx) = mpsc::channel::<InputEvent>();
    spawn_input_thread(tx);

    // agent-event 通道（host → 主循环）+ turn-done 通道（agent 任务 → 主循环）。
    let (agent_tx, agent_rx) = mpsc::channel::<AgentUiEvent>();
    let (done_tx, done_rx) = mpsc::channel::<TurnDone>();

    // 自建多线程 runtime 跑后台 agent。
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to start async runtime: {err}"),
            ));
        }
    };

    // 解析 settings → assembly（provider/model + 各项 knob）。失败也启动 shell，只是提交会报错。
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let settings = load_settings_from_disk();
    let (provider_override, model_override) = split_model_label(&options.model);
    let assembly = TurnAssembly::resolve(
        &settings,
        provider_override.as_deref(),
        model_override.as_deref(),
        &cwd,
        /* approve_sensitive */ true,
    );

    let mut turn = match assembly {
        Ok(assembly) => {
            // 把 system prompt 作为 runtime_messages 的第一条。
            let runtime_messages =
                vec![json!({ "role": "system", "content": assembly.system_prompt.clone() })];
            // 建 session（best-effort：失败则无持久化但继续）。
            let session = Session::create(&cwd, &assembly.model_label()).ok();
            if session.is_none() {
                app.push_notice("(session persistence unavailable; continuing without it)");
            }
            let state = build_app_state(settings.clone());
            let timeout_ms = assembly.effective_chat_tools.tool_timeout_ms;
            app.push_notice("kivio-code interactive. Type a message; /help for commands; Esc cancels a run; Ctrl+D exits.");
            Some(TurnRuntime {
                handle: runtime.handle().clone(),
                state,
                assembly: Arc::new(assembly),
                cwd,
                timeout_ms,
                generations: Generations::default(),
                current: None,
                current_message_id: None,
                runtime_messages,
                session,
                persisted_tool_calls: std::collections::HashSet::new(),
                turn_done_tx: done_tx,
            })
        }
        Err(err) => {
            app.push_notice(format!("No usable model: {err}"));
            app.push_notice("Configure a chat model in the Kivio app, then restart. Ctrl+D exits.");
            None
        }
    };

    let mut tui = Tui::new(terminal);
    tui.set_show_hardware_cursor(true);

    // 首帧。
    render_frame(&mut tui, &mut app, cols);

    let exit = run_loop(
        &mut tui,
        &mut app,
        &rx,
        &agent_rx,
        &done_rx,
        &agent_tx,
        turn.as_mut(),
    );

    // 收尾：取消任何在跑的轮，停渲染并换行让 prompt 干净。
    if let Some(turn) = turn.as_ref() {
        turn.request_cancel();
    }
    tui.stop();
    tui.terminal.write("\r\n");
    // runtime drop 会等后台任务收尾（已发取消信号，loop 很快返回）。
    drop(runtime);

    exit
}

/// 从 footer 的 `provider:model` 串拆回 provider / model override（供 `TurnAssembly::resolve`）。
/// 缺省 / `<no model>` 时返回 `(None, None)`，让 resolve 走 settings 默认。
fn split_model_label(label: &str) -> (Option<String>, Option<String>) {
    if label.is_empty() || label.starts_with('<') {
        return (None, None);
    }
    match label.split_once(':') {
        Some((provider, model)) => (Some(provider.to_string()), Some(model.to_string())),
        None => (None, Some(label.to_string())),
    }
}

/// 主事件循环。返回 Ok 表示正常退出。
///
/// `recv_timeout` 一个 [`InputEvent`] → 喂 [`App`]；超时分支先 drain agent-event / turn-done，
/// 再轮询 resize。每次有变更后做一次差分渲染。
#[allow(clippy::too_many_arguments)]
fn run_loop(
    tui: &mut Tui<CrosstermTerminal>,
    app: &mut App,
    rx: &Receiver<InputEvent>,
    agent_rx: &Receiver<AgentUiEvent>,
    done_rx: &Receiver<TurnDone>,
    agent_tx: &Sender<AgentUiEvent>,
    mut turn: Option<&mut TurnRuntime>,
) -> std::io::Result<()> {
    loop {
        let mut dirty = false;
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(InputEvent::Key(data)) => match app.handle_key(&data) {
                AppEffect::Quit => return Ok(()),
                AppEffect::Submitted(text) => {
                    if let Some(turn) = turn.as_deref_mut() {
                        if !turn.is_generating() {
                            app.set_mode(AppMode::Generating);
                            turn.begin_turn(text, agent_tx);
                        }
                    } else {
                        app.push_notice("No model configured; cannot run.");
                    }
                    dirty = true;
                }
                AppEffect::Cancel => {
                    if let Some(turn) = turn.as_deref_mut() {
                        turn.request_cancel();
                    }
                    dirty = true;
                }
                AppEffect::None => dirty = true,
            },
            Ok(InputEvent::Paste(content)) => {
                let wrapped = format!("\x1b[200~{content}\x1b[201~");
                match app.handle_key(&wrapped) {
                    AppEffect::Quit => return Ok(()),
                    _ => dirty = true,
                }
            }
            Ok(InputEvent::Resize(cols, rows)) => {
                tui.terminal.set_size(cols, rows);
                app.set_terminal_rows(rows);
                tui.invalidate();
                dirty = true;
            }
            Ok(InputEvent::Eof) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if tui.terminal.refresh_size() {
                    app.set_terminal_rows(tui.terminal.rows());
                    tui.invalidate();
                    dirty = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }

        // Drain streaming / tool-record / done events (non-blocking).
        while let Ok(event) = agent_rx.try_recv() {
            app.apply_agent_event(event);
            dirty = true;
        }
        // Drain finished turns.
        while let Ok(done) = done_rx.try_recv() {
            if let Some(turn) = turn.as_deref_mut() {
                turn.finish_turn(done, app);
            }
            dirty = true;
        }

        if dirty {
            let width = tui.terminal.columns();
            render_frame(tui, app, width);
        }
    }
}

/// 渲染一帧：把 App 渲染出的行交给差分渲染器。
fn render_frame(tui: &mut Tui<CrosstermTerminal>, app: &mut App, width: u16) {
    let lines = app.render(width);
    tui.clear_children();
    tui.add_child(Box::new(AppFrame { lines }));
    tui.render();
}

/// 起一条输入线程：阻塞读 stdin 原始字节，喂 StdinBuffer，把序列 / 粘贴发到主循环。
///
/// 线程只读原始 stdin（不碰 crossterm 事件读），避免对同一 fd 争用。stdin EOF 或 channel 关闭时退出；
/// 用户正常退出（Ctrl+D / `/quit`）后主循环返回、进程结束，本线程随之被回收（调用方不 join）。
fn spawn_input_thread(tx: Sender<InputEvent>) {
    std::thread::spawn(move || {
        let mut buffer = StdinBuffer::new();
        let mut stdin = std::io::stdin();
        let mut bytes = [0u8; 1024];

        loop {
            let n = match stdin.read(&mut bytes) {
                Ok(0) => {
                    let _ = tx.send(InputEvent::Eof);
                    return;
                }
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = tx.send(InputEvent::Eof);
                    return;
                }
            };

            let chunk = String::from_utf8_lossy(&bytes[..n]);
            let events = buffer.process(&chunk);
            for seq in events.sequences {
                if tx.send(InputEvent::Key(seq)).is_err() {
                    return;
                }
            }
            for paste in events.pastes {
                if tx.send(InputEvent::Paste(paste)).is_err() {
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::ToolCallStatus;
    use crate::settings::{ModelProvider, Settings};

    /// 用 fake channel 驱动 run_loop 的逻辑等价物：这里复用 App 直接断言事件→效果，
    /// 因为 run_loop 与真实 Tui<CrosstermTerminal> 绑定（需 TTY），其分发逻辑已在 App 单测覆盖。
    /// 本测试聚焦 InputEvent → App 的分发约定。
    #[test]
    fn key_event_drives_app_handle_key() {
        let mut app = App::new("~/p".to_string(), "m".to_string());
        app.set_terminal_rows(24);
        // 普通字符
        assert_eq!(app.handle_key("h"), AppEffect::None);
        assert_eq!(app.handle_key("i"), AppEffect::None);
        assert_eq!(app.editor_text(), "hi");
        // enter 提交 → Submitted。
        assert_eq!(app.handle_key("\r"), AppEffect::Submitted("hi".to_string()));
        assert_eq!(app.last_submitted(), Some("hi"));
    }

    #[test]
    fn paste_event_inserts_into_editor() {
        let mut app = App::new("~/p".to_string(), "m".to_string());
        app.set_terminal_rows(24);
        let wrapped = format!("\x1b[200~{}\x1b[201~", "pasted text");
        app.handle_key(&wrapped);
        assert_eq!(app.editor_text(), "pasted text");
    }

    #[test]
    fn quit_via_ctrl_d() {
        let mut app = App::new("~/p".to_string(), "m".to_string());
        app.set_terminal_rows(24);
        assert_eq!(app.handle_key("\x04"), AppEffect::Quit);
    }

    #[test]
    fn input_event_equality() {
        assert_eq!(InputEvent::Key("a".into()), InputEvent::Key("a".into()));
        assert_ne!(InputEvent::Key("a".into()), InputEvent::Paste("a".into()));
        assert_eq!(InputEvent::Resize(80, 24), InputEvent::Resize(80, 24));
    }

    #[test]
    fn split_model_label_variants() {
        assert_eq!(
            split_model_label("openai:gpt-4o"),
            (Some("openai".to_string()), Some("gpt-4o".to_string()))
        );
        assert_eq!(split_model_label("gpt-4o"), (None, Some("gpt-4o".to_string())));
        assert_eq!(split_model_label("<no model>"), (None, None));
        assert_eq!(split_model_label(""), (None, None));
    }

    #[test]
    fn human_tokens_formats() {
        assert_eq!(human_tokens(42), "42");
        assert_eq!(human_tokens(1500), "1.5k");
    }

    #[test]
    fn format_usage_summary() {
        let usage = crate::chat::model::ModelUsage {
            input_tokens: Some(1234),
            output_tokens: Some(56),
            ..Default::default()
        };
        let s = format_usage(Some(&usage)).unwrap();
        assert!(s.contains("1.2k in"));
        assert!(s.contains("56 out"));
        assert!(format_usage(None).is_none());
    }

    // ---- TurnRuntime integration (no real model / TTY) ----

    fn provider(id: &str) -> ModelProvider {
        ModelProvider {
            id: id.to_string(),
            name: id.to_string(),
            api_keys: vec!["sk-x".to_string()],
            api_key_legacy: None,
            base_url: "https://example.com/v1".to_string(),
            available_models: vec!["m1".to_string()],
            enabled_models: vec!["m1".to_string()],
            supports_tools: true,
            enabled: true,
            api_format: "openai_chat".to_string(),
            model_overrides: Default::default(),
        }
    }

    fn test_settings() -> Settings {
        let mut s = Settings::default();
        s.providers = vec![provider("chat")];
        s.default_models.chat.provider_id = "chat".to_string();
        s.default_models.chat.model = "m1".to_string();
        s
    }

    fn unique_cwd(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("kivio-code-turn-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp cwd");
        dir
    }

    /// Build a TurnRuntime wired to a real session + headless state, but never
    /// spawns a real model run — tests drive the post-turn logic directly.
    fn turn_runtime(cwd: &PathBuf) -> (TurnRuntime, Receiver<TurnDone>) {
        let settings = test_settings();
        let assembly =
            TurnAssembly::resolve(&settings, None, None, cwd, true).expect("assembly resolves");
        let runtime_messages =
            vec![json!({ "role": "system", "content": assembly.system_prompt.clone() })];
        let session = Session::create(cwd, &assembly.model_label()).expect("session create");
        let state = build_app_state(settings);
        let (done_tx, done_rx) = mpsc::channel::<TurnDone>();
        let rt = TurnRuntime {
            handle: tokio::runtime::Handle::current(),
            state,
            assembly: Arc::new(assembly),
            cwd: cwd.clone(),
            timeout_ms: 120_000,
            generations: Generations::default(),
            current: None,
            current_message_id: None,
            runtime_messages,
            session: Some(session),
            persisted_tool_calls: std::collections::HashSet::new(),
            turn_done_tx: done_tx,
        };
        (rt, done_rx)
    }

    fn result_with(content: &str, api_messages: Vec<Value>, tool_records: Vec<ToolCallRecord>) -> AgentRunResult {
        AgentRunResult {
            content: content.to_string(),
            reasoning: None,
            tool_records,
            segments: Vec::new(),
            api_messages,
            steps: Vec::new(),
            stream_outcome: "completed".to_string(),
            usage: None,
        }
    }

    fn tool_record(id: &str, name: &str) -> ToolCallRecord {
        ToolCallRecord {
            id: id.to_string(),
            name: name.to_string(),
            source: "native".to_string(),
            server_id: None,
            arguments: serde_json::json!({ "path": "a.txt" }).to_string(),
            status: ToolCallStatus::Success,
            result_preview: Some("ok".to_string()),
            error: None,
            duration_ms: None,
            started_at: None,
            completed_at: None,
            round: 1,
            sensitive: false,
            artifacts: Vec::new(),
            trace_id: None,
            span_id: None,
            structured_content: None,
        }
    }

    #[tokio::test]
    async fn session_create_append_roundtrip_after_simulated_turn() {
        let cwd = unique_cwd("roundtrip");
        let (mut rt, _done) = turn_runtime(&cwd);
        let path = rt.session.as_ref().unwrap().path.clone();

        // Simulate a user submit (without spawning): persist + accumulate.
        rt.append_session(SessionRecord::Message {
            id: String::new(),
            parent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            role: "user".to_string(),
            content: "read a.txt".to_string(),
        });
        rt.runtime_messages
            .push(json!({ "role": "user", "content": "read a.txt" }));

        // Simulate the agent finishing with one tool call + an answer.
        let result = result_with(
            "Read it.",
            vec![json!({ "role": "assistant", "content": "Read it." })],
            vec![tool_record("call_1", "read")],
        );
        rt.persist_turn_records(&result);

        // Reload the session from disk and assert the records landed.
        let reloaded = Session::load(&path).expect("reload");
        let roles: Vec<&str> = reloaded
            .records
            .iter()
            .map(|r| match r {
                SessionRecord::Message { role, .. } => role.as_str(),
                SessionRecord::ToolCall { .. } => "tool_call",
                SessionRecord::ToolResult { .. } => "tool_result",
                _ => "other",
            })
            .collect();
        assert_eq!(roles, vec!["user", "tool_call", "tool_result", "assistant"]);

        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(crate::kivio_code::session::session_dir_for_cwd(&cwd));
    }

    #[tokio::test]
    async fn multi_turn_runtime_messages_accumulate() {
        let cwd = unique_cwd("multiturn");
        let (mut rt, _done) = turn_runtime(&cwd);
        let base = rt.runtime_messages.len(); // 1 (system)

        // Turn 1.
        rt.runtime_messages
            .push(json!({ "role": "user", "content": "first" }));
        let r1 = result_with("answer one", vec![json!({ "role": "assistant", "content": "answer one" })], Vec::new());
        rt.accumulate_runtime_messages(&r1);

        // Turn 2 carries turn-1 context.
        rt.runtime_messages
            .push(json!({ "role": "user", "content": "second" }));
        let r2 = result_with("answer two", vec![json!({ "role": "assistant", "content": "answer two" })], Vec::new());
        rt.accumulate_runtime_messages(&r2);

        // system + (user1 + assistant1) + (user2 + assistant2) = base + 4
        assert_eq!(rt.runtime_messages.len(), base + 4);
        assert_eq!(rt.runtime_messages[0]["role"], "system");
        assert_eq!(rt.runtime_messages[1]["content"], "first");
        assert_eq!(rt.runtime_messages[2]["content"], "answer one");
        assert_eq!(rt.runtime_messages[3]["content"], "second");
        assert_eq!(rt.runtime_messages[4]["content"], "answer two");

        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(crate::kivio_code::session::session_dir_for_cwd(&cwd));
    }

    #[tokio::test]
    async fn tool_record_persisted_once_per_call_id() {
        let cwd = unique_cwd("toolonce");
        let (mut rt, _done) = turn_runtime(&cwd);
        let path = rt.session.as_ref().unwrap().path.clone();

        // Same call_id persisted twice (e.g. emitted again) → only one pair.
        rt.persist_tool_record(&tool_record("call_1", "read"));
        rt.persist_tool_record(&tool_record("call_1", "read"));

        let reloaded = Session::load(&path).expect("reload");
        let tool_calls = reloaded
            .records
            .iter()
            .filter(|r| matches!(r, SessionRecord::ToolCall { .. }))
            .count();
        assert_eq!(tool_calls, 1);

        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(crate::kivio_code::session::session_dir_for_cwd(&cwd));
    }

    #[tokio::test]
    async fn finish_turn_ignores_stale_generation() {
        let cwd = unique_cwd("stale");
        let (mut rt, _done) = turn_runtime(&cwd);
        let mut app = App::new("~".to_string(), "chat:m1".to_string());
        app.set_terminal_rows(24);
        app.set_mode(AppMode::Generating);

        // Live run is generation 5; a done for generation 3 must be ignored.
        rt.current = Some(RunCancel::new(5));
        let stale = TurnDone {
            generation: 3,
            result: Ok(result_with("ignored", Vec::new(), Vec::new())),
            message_id: "m3".to_string(),
        };
        rt.finish_turn(stale, &mut app);
        // Still generating; the live run was not cleared by a stale done.
        assert!(rt.is_generating());
        assert_eq!(app.mode(), AppMode::Generating);

        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(crate::kivio_code::session::session_dir_for_cwd(&cwd));
    }

    #[tokio::test]
    async fn finish_turn_live_generation_finalizes_and_idles() {
        let cwd = unique_cwd("live");
        let (mut rt, _done) = turn_runtime(&cwd);
        let mut app = App::new("~".to_string(), "chat:m1".to_string());
        app.set_terminal_rows(24);
        app.set_mode(AppMode::Generating);
        // Stream some content for message m7, then finish that generation.
        app.apply_agent_event(AgentUiEvent::StreamDelta {
            message_id: "m7".to_string(),
            delta: "done answer".to_string(),
            reasoning: String::new(),
        });
        rt.current = Some(RunCancel::new(7));
        let done = TurnDone {
            generation: 7,
            result: Ok(result_with("done answer", Vec::new(), Vec::new())),
            message_id: "m7".to_string(),
        };
        rt.finish_turn(done, &mut app);
        assert!(!rt.is_generating());
        assert_eq!(app.mode(), AppMode::Idle);
        assert!(!app.assistant_streaming());

        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(crate::kivio_code::session::session_dir_for_cwd(&cwd));
    }
}
