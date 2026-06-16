//! 交互模式 —— 事件循环 + 输入线程 + 差分渲染协调。
//!
//! 这是 Phase 5a 的架构骨架：把 Phase 4 的 TUI 库（差分渲染 [`Tui`]、组件树、键解码）接到真实终端，
//! 跑一个 async-safe 的事件循环。**5a 不真正调用 agent**：[`App::submit`] 在 5a 把输入回显为助手通知，
//! agent 执行留待 5b（届时新增一条 agent-event 通道并 select 进本循环）。
//!
//! ## 事件循环 / 输入线程 / 渲染协调
//!
//! 三个并发来源汇入一个事件循环：
//! 1. **输入线程**：一条专用 OS 线程在 raw 模式下阻塞 `read` stdin 原始字节，喂给 [`StdinBuffer`]
//!    （Phase 4a，比 crossterm 的 key parser 保真度更高），把切出的完整序列 / 粘贴段通过
//!    [`mpsc`](std::sync::mpsc) 发到主循环（[`InputEvent`]）。线程在 stdin EOF 或收到停止信号后退出。
//!    **该线程只读原始 stdin 字节**，不碰 crossterm 的事件读，避免对同一 fd 的争用。
//! 2. **resize**：主循环每次 `recv_timeout` 超时 tick 时用 `crossterm::terminal::size()` 轮询当前
//!    尺寸，与上一帧比较；变化则全量重绘。轮询查询尺寸不消费 stdin，故与输入线程无 fd 冲突。
//! 3. **(预留) agent-event 通道**：5b 会新增 `mpsc<AgentEvent>`，主循环在同一个 `recv_timeout` 轮询
//!    里 drain 它（无需 tokio）。
//!
//! 主循环：`recv_timeout` 一个 [`InputEvent`] → 喂给 [`App`] → 若产生 [`AppEffect::Quit`] 则退出；
//! 超时分支轮询 resize。每次状态变更后 `Tui::render`（差分，仅写出改动行）。raw 模式由
//! [`RawModeGuard`] RAII 管理，panic 也会还原。
//!
//! ## 为什么不用 tokio / 不用 alt-screen
//! - 5a 没有真正的 async I/O（agent 调用在 5b），一条输入线程 + 阻塞 channel 足够且更易测试 / 推理。
//! - 渲染进 NORMAL buffer（对齐 PI / [`Tui`]），让历史滚入 scrollback。

pub mod app;
pub mod slash;

pub use app::{App, AppEffect, AppMode, ToolCardPlaceholder};

use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

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

/// 启动交互模式，阻塞直到用户退出。**需要一个真实 TTY**（调用方在 bin 里已判断 stdin/stdout 是 TTY）。
///
/// 整个生命周期内 raw 模式经 [`RawModeGuard`] 管理，正常返回或 panic 均会还原终端。
pub fn run(options: InteractiveOptions) -> std::io::Result<()> {
    let _guard = RawModeGuard::enter()?;

    let terminal = CrosstermTerminal::new();
    let (cols, rows) = (terminal.columns(), terminal.rows());

    let mut app = App::new(options.cwd_display, options.model);
    app.set_terminal_rows(rows);
    app.push_notice("kivio-code interactive (Phase 5a). Type a message, /help for commands, Ctrl+D to exit.");

    // 输入线程：raw stdin → StdinBuffer → InputEvent channel。
    // 该线程阻塞在 stdin read 上；主循环退出后进程随即结束，线程随之被回收（故不 join，避免被
    // 阻塞的 read 卡住收尾）。
    let (tx, rx) = mpsc::channel::<InputEvent>();
    spawn_input_thread(tx);

    let mut tui = Tui::new(terminal);
    tui.set_show_hardware_cursor(true);

    // 首帧。
    render_frame(&mut tui, &mut app, cols);

    // 主循环：recv_timeout 输入事件（超时分支轮询 resize；5b 在此 drain agent-event）。
    let exit = run_loop(&mut tui, &mut app, &rx);

    // 收尾：停渲染并换行让 prompt 干净（RawModeGuard 在 run() 返回时 drop，还原终端）。
    tui.stop();
    tui.terminal.write("\r\n");

    exit
}

/// 主事件循环。返回 Ok 表示正常退出。
///
/// `recv_timeout` 超时分支用来轮询 resize（不消费 stdin，避免与输入线程争 fd），5b 也在此 drain
/// agent-event。每次有变更后做一次差分渲染。
fn run_loop(
    tui: &mut Tui<CrosstermTerminal>,
    app: &mut App,
    rx: &Receiver<InputEvent>,
) -> std::io::Result<()> {
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(InputEvent::Key(data)) => match app.handle_key(&data) {
                AppEffect::Quit => return Ok(()),
                AppEffect::Submitted(_) => { /* 5b: hand to agent loop */ }
                AppEffect::None => {}
            },
            Ok(InputEvent::Paste(content)) => {
                // 把 paste 还原成 bracketed-paste 包裹喂给 editor（Editor 内部识别 marker）。
                let wrapped = format!("\x1b[200~{content}\x1b[201~");
                if let AppEffect::Quit = app.handle_key(&wrapped) {
                    return Ok(());
                }
            }
            Ok(InputEvent::Resize(cols, rows)) => {
                // 备用路径：若未来由通道投递 resize（当前由超时分支轮询尺寸）。
                tui.terminal.set_size(cols, rows);
                app.set_terminal_rows(rows);
                tui.invalidate();
            }
            Ok(InputEvent::Eof) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // 轮询 resize：尺寸变化则同步 app + 全量重绘。
                if tui.terminal.refresh_size() {
                    app.set_terminal_rows(tui.terminal.rows());
                    tui.invalidate();
                } else {
                    // 无变化、无输入：不渲染，省 CPU。
                    continue;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }

        let width = tui.terminal.columns();
        render_frame(tui, app, width);
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
        // enter 提交
        assert_eq!(app.handle_key("\r"), AppEffect::None);
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
}
