use tauri::{
    window::Color, AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
#[cfg(target_os = "macos")]
use tauri::{LogicalPosition, TitleBarStyle};

/// 侧栏收起时主内容区最小宽度（与前端 `CHAT_MIN_SIZE_COLLAPSED` 一致）。
pub const CHAT_MIN_INNER_WIDTH_COLLAPSED: f64 = 400.0;
/// 侧栏展开时整窗最小宽度（240px 侧栏 + 主内容区最小宽度）。
pub const CHAT_MIN_INNER_WIDTH_EXPANDED: f64 = 640.0;
pub const CHAT_MIN_INNER_HEIGHT: f64 = 400.0;
const CHAT_DEFAULT_INNER_WIDTH: f64 = 1280.0;
const CHAT_DEFAULT_INNER_HEIGHT: f64 = 800.0;

fn chat_window_size_for_visible_content(width: f64, height: f64) -> (f64, f64) {
    (width, height)
}

pub fn apply_chat_window_min_size(window: &WebviewWindow, sidebar_expanded: bool) {
    let width = if sidebar_expanded {
        CHAT_MIN_INNER_WIDTH_EXPANDED
    } else {
        CHAT_MIN_INNER_WIDTH_COLLAPSED
    };
    let (width, height) = chat_window_size_for_visible_content(width, CHAT_MIN_INNER_HEIGHT);
    let _ = window.set_min_size(Some(LogicalSize::new(width, height)));
}

#[cfg(target_os = "macos")]
const CHAT_TRAFFIC_LIGHT_X: f64 = 14.0;

/// 与侧栏顶栏图标（52px 行高居中）垂直对齐。
#[cfg(target_os = "macos")]
const CHAT_TRAFFIC_LIGHT_Y: f64 = 29.0;

#[cfg(target_os = "macos")]
pub(crate) fn apply_macos_traffic_light_position(window: &WebviewWindow) {
    use cocoa::base::id;

    let window_for_main = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ptr) = window_for_main.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        unsafe {
            hide_macos_window_title(ptr as id);
            inset_traffic_lights(ptr as id, CHAT_TRAFFIC_LIGHT_X, CHAT_TRAFFIC_LIGHT_Y);
        }
    });
}

/// NSWindowTitleHidden — 隐藏 Overlay 标题栏中的窗口标题文字。
#[cfg(target_os = "macos")]
unsafe fn hide_macos_window_title(window: cocoa::base::id) {
    use objc::{msg_send, sel, sel_impl};

    const NS_WINDOW_TITLE_HIDDEN: u64 = 1;
    let _: () = msg_send![window, setTitleVisibility: NS_WINDOW_TITLE_HIDDEN];
}

/// 与 wry `inset_traffic_lights` 相同逻辑，用于运行时微调原生交通灯位置。
#[cfg(target_os = "macos")]
unsafe fn inset_traffic_lights(window: cocoa::base::id, x: f64, y: f64) {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSRect;
    use objc::{msg_send, sel, sel_impl};

    let close: id = msg_send![window, standardWindowButton: 0u64];
    if close == nil {
        return;
    }
    let miniaturize: id = msg_send![window, standardWindowButton: 1u64];
    if miniaturize == nil {
        return;
    }
    let zoom: id = msg_send![window, standardWindowButton: 2u64];

    let title_bar_container_view: id = msg_send![close, superview];
    let title_bar_container_view: id = msg_send![title_bar_container_view, superview];

    let close_rect: NSRect = msg_send![close, frame];
    let title_bar_frame_height = close_rect.size.height + y;
    let mut title_bar_rect: NSRect = msg_send![title_bar_container_view, frame];
    title_bar_rect.size.height = title_bar_frame_height;
    let window_frame: NSRect = msg_send![window, frame];
    title_bar_rect.origin.y = window_frame.size.height - title_bar_frame_height;
    let _: () = msg_send![title_bar_container_view, setFrame: title_bar_rect];

    let miniaturize_rect: NSRect = msg_send![miniaturize, frame];
    let space_between = miniaturize_rect.origin.x - close_rect.origin.x;

    let mut buttons = vec![close, miniaturize];
    if zoom != nil {
        buttons.push(zoom);
    }

    for (i, button) in buttons.into_iter().enumerate() {
        let mut rect: NSRect = msg_send![button, frame];
        rect.origin.x = x + (i as f64 * space_between);
        let _: () = msg_send![button, setFrameOrigin: rect.origin];
    }
}

/// Chat 作为普通桌面窗口：不置顶、不跨全 Space（与 Lens overlay 区分）。
pub fn normalize_chat_window_behavior(window: &WebviewWindow) {
    let _ = window.set_always_on_top(false);
    let _ = window.set_skip_taskbar(false);
    #[cfg(target_os = "macos")]
    let _ = window.set_visible_on_all_workspaces(false);
}

/// macOS Chat：系统 Overlay 标题栏 + 原生交通灯；其他平台保持无边框自绘控件。
pub fn apply_chat_window_chrome(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_decorations(true);
        let _ = window.set_title_bar_style(TitleBarStyle::Overlay);
        let _ = window.set_shadow(true);
        apply_macos_traffic_light_position(window);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_decorations(false);
        let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_shadow(true);
            apply_windows_chat_window_frame(window);
        }
        #[cfg(not(target_os = "windows"))]
        let _ = window.set_shadow(false);
    }
}

// Windows Chat: let DWM own the outer shadow/corners. The WebView content
// fills the window without a second CSS-drawn rounded frame.
#[cfg(target_os = "windows")]
fn apply_windows_chat_window_frame(window: &WebviewWindow) {
    use std::ffi::c_void;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    unsafe {
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            std::mem::size_of_val(&corner) as u32,
        );

        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const _ as *const c_void,
            std::mem::size_of_val(&border_color) as u32,
        );
    }
}

/// 翻译器 / 设置等悬浮小窗：无边框透明壳。
pub fn apply_frameless_window_chrome(window: &WebviewWindow) {
    let _ = window.set_decorations(false);
    let _ = window.set_shadow(false);
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_title_bar_style(TitleBarStyle::Visible);
    }
}

/**
 * 获取主窗口
 */
pub fn get_main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

pub fn get_settings_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("settings")
}

pub fn get_chat_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("chat")
}

/**
 * 确保主窗口存在（不存在则创建）
 * 从 tauri.conf.json 中读取主窗口配置进行创建
 */
pub fn ensure_main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = get_main_window(app) {
        return Ok(window);
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or_else(|| "Main window config not found".to_string())?;

    WebviewWindowBuilder::from_config(app, config)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())
}

/**
 * 确保独立 Chat 窗口存在。
 */
pub fn ensure_chat_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    ensure_chat_window_with_hash(app, "chat")
}

/**
 * 确保独立 Chat 窗口存在，并在首次创建时进入指定 hash 路由。
 */
pub fn ensure_chat_window_with_hash(app: &AppHandle, hash: &str) -> Result<WebviewWindow, String> {
    if let Some(window) = get_chat_window(app) {
        return Ok(window);
    }

    let route = hash.trim_start_matches('#');
    let route = if route.is_empty() { "chat" } else { route };
    let url = format!("index.html#{route}");
    let (min_width, min_height) =
        chat_window_size_for_visible_content(CHAT_MIN_INNER_WIDTH_COLLAPSED, CHAT_MIN_INNER_HEIGHT);
    let (default_width, default_height) =
        chat_window_size_for_visible_content(CHAT_DEFAULT_INNER_WIDTH, CHAT_DEFAULT_INNER_HEIGHT);
    let mut builder = WebviewWindowBuilder::new(app, "chat", WebviewUrl::App(url.into()))
        .title("Kivio")
        .inner_size(default_width, default_height)
        .min_inner_size(min_width, min_height)
        .resizable(true)
        .visible_on_all_workspaces(false)
        .skip_taskbar(false)
        .visible(false);

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .decorations(true)
            .title_bar_style(TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(LogicalPosition::new(
                CHAT_TRAFFIC_LIGHT_X,
                CHAT_TRAFFIC_LIGHT_Y,
            ))
            .transparent(false)
            .shadow(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        // 透明 WebView 背景允许前端 shell 自绘圆角；Windows 不使用原生 shadow，
        // 避免透明窗口矩形外壳在桌面上显示成第二层边框。
        builder = builder
            .decorations(false)
            .transparent(true)
            .background_color(Color(0, 0, 0, 0))
            .shadow(false);
    }

    builder.build().map_err(|e| e.to_string())
}

/**
 * 确保 Lens 窗口存在（不存在则创建）
 * 单 webview 三态：select 全屏 / ready 悬浮 600x72 / answering 悬浮 600x420。
 * 创建时尺寸为悬浮态默认值；后端按需要 set_size 切换。
 */
pub fn ensure_lens_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("lens") {
        return Ok(window);
    }

    let window = WebviewWindowBuilder::new(app, "lens", WebviewUrl::App("index.html#lens".into()))
        .title("Lens")
        .inner_size(600.0, 72.0)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .resizable(true)
        .decorations(false)
        .shadow(false)
        .transparent(true)
        // 把 WebView2 / WKWebView 的默认背景设成全透明。Windows 上 WebView2 控件
        // 在 HTML/CSS 把 html、body、#root 设为 transparent 之前会用系统主题色（白）
        // 渲染首帧，导致全屏白闪 —— 设了 (0,0,0,0) 后默认背景本身就是透明的。
        // 文档：Windows 8+ 上 webview 层的 alpha=0 被尊重；macOS 上此调用是 no-op。
        .background_color(Color(0, 0, 0, 0))
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    // macOS：把 lens 浮窗转成非激活 NSPanel，使其能浮现在别的 App 原生全屏 Space 上方。
    #[cfg(target_os = "macos")]
    ensure_overlay_panel(&window);

    Ok(window)
}

/// macOS：把短命浮窗（lens / 翻译）转成**非激活 NSPanel**，使其能浮现在别的 App
/// 原生全屏 Space 上方且不切换 Space。幂等：已是 panel 则跳过重分类，只重申行为。
///
/// 背景：macOS（Big Sur 起）只允许 NSPanel、或 Accessory(LSUIElement) 策略 app 的窗口
/// 画进别的 App 的全屏 Space。本 app 为 `ActivationPolicy::Regular`（Chat 需要 Dock 身份），
/// 所以普通 NSWindow 无论 collectionBehavior / level 怎么设都进不去别人的全屏 Space。改成
/// 非激活 NSPanel 后：①panel 被系统允许覆盖全屏；②非激活 → 点击/聚焦不激活宿主 app →
/// 不会把用户从全屏 Space 拽走。Chat 窗口**绝不**走这里，保持普通 NSWindow。
#[cfg(target_os = "macos")]
pub fn ensure_overlay_panel(window: &WebviewWindow) {
    run_overlay_on_main(window, |ptr| unsafe {
        configure_overlay_panel(ptr);
    });
}

/// macOS：显示浮窗。`orderFrontRegardless` 不激活 app、不切 Space；`need_key=true` 时再
/// `makeKeyWindow`，让 WebView 能接收键盘（翻译输入框 / lens 问题框 / Escape）。非激活 panel
/// 成为 key 也不会激活 app。**绝不**用 `set_focus` / `makeKeyAndOrderFront` /
/// `activateIgnoringOtherApps`——那会激活 Regular app 并把用户从全屏 Space 拽走。
#[cfg(target_os = "macos")]
pub fn show_overlay_panel(window: &WebviewWindow, need_key: bool) {
    use objc::{msg_send, sel, sel_impl};
    run_overlay_on_main(window, move |ptr| unsafe {
        // 显示前再重申一次 panel 行为，抵消 tao set_resizable / set_always_on_top 可能造成的
        // styleMask / level 漂移。
        configure_overlay_panel(ptr);
        let _: () = msg_send![ptr, orderFrontRegardless];
        if need_key {
            let _: () = msg_send![ptr, makeKeyWindow];
            // configure_overlay_panel 里的 setStyleMask 会清掉 first responder，使 WebView 在
            // 窗口被再次点击前收不到键盘（tao 自己在每次 setStyleMask 后也补这一步）。lens 会经
            // set_resizable 间接走 tao 的补偿，但翻译窗不会，所以这里显式把 contentView 设回
            // first responder，保证首帧就能输入。
            let view: *mut objc::runtime::Object = msg_send![ptr, contentView];
            if !view.is_null() {
                let _: () = msg_send![ptr, makeFirstResponder: view];
            }
        }
    });
}

/// 在主线程上拿到 ns_window 指针并执行 `f`（AppKit 调用必须落在主线程）。
#[cfg(target_os = "macos")]
fn run_overlay_on_main<F>(window: &WebviewWindow, f: F)
where
    F: FnOnce(*mut objc::runtime::Object) + Send + 'static,
{
    let run = move |window: &WebviewWindow| {
        let Ok(ptr) = window.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        f(ptr as *mut objc::runtime::Object);
    };

    if macos_is_main_thread() {
        run(window);
        return;
    }

    let window_for_task = window.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    if window
        .run_on_main_thread(move || {
            run(&window_for_task);
            let _ = tx.send(());
        })
        .is_ok()
    {
        let _ = rx.recv_timeout(std::time::Duration::from_millis(250));
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_is_main_thread() -> bool {
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let is_main: bool = msg_send![class!(NSThread), isMainThread];
        is_main
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn object_setClass(
        obj: *mut objc::runtime::Object,
        cls: *const objc::runtime::Class,
    ) -> *const objc::runtime::Class;
}

/// 运行时注册一个 NSPanel 子类：borderless 窗口默认 `canBecomeKeyWindow=NO`，强制 YES 才能
/// 接收键盘；`canBecomeMainWindow=NO` 保持其辅助身份。进程内只注册一次。
#[cfg(target_os = "macos")]
fn kivio_overlay_panel_class() -> *const objc::runtime::Class {
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL, NO, YES};
    use objc::{class, sel, sel_impl};
    use std::sync::OnceLock;

    // ClassDecl::register 返回的类指针进程生命周期常驻、只读，可安全跨线程共享。
    struct PanelClass(*const Class);
    unsafe impl Send for PanelClass {}
    unsafe impl Sync for PanelClass {}

    static PANEL_CLASS: OnceLock<PanelClass> = OnceLock::new();

    extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
        YES
    }
    extern "C" fn no_(_: &Object, _: Sel) -> BOOL {
        NO
    }

    PANEL_CLASS
        .get_or_init(|| {
            let superclass = class!(NSPanel);
            let mut decl =
                ClassDecl::new("KivioOverlayPanel", superclass).expect("declare KivioOverlayPanel");
            unsafe {
                decl.add_method(
                    sel!(canBecomeKeyWindow),
                    yes as extern "C" fn(&Object, Sel) -> BOOL,
                );
                decl.add_method(
                    sel!(canBecomeMainWindow),
                    no_ as extern "C" fn(&Object, Sel) -> BOOL,
                );
            }
            PanelClass(decl.register() as *const Class)
        })
        .0
}

/// 重分类窗口为非激活 NSPanel 并设置全屏浮现所需的 styleMask / collectionBehavior / level（幂等）。
#[cfg(target_os = "macos")]
unsafe fn configure_overlay_panel(window: *mut objc::runtime::Object) {
    use objc::{msg_send, sel, sel_impl};

    // 1) 重分类到 NSPanel 子类（已是则跳过 object_setClass）。
    //    注意：重分类后实例的类不再是 tao 的 `TaoWindow`，丢失了它的 `focusable` ivar——
    //    因此**绝不能**对 lens/翻译窗调用 `WebviewWindow::set_focusable()`，tao 会用
    //    `get_mut_ivar::<Bool>("focusable")` 在新类上找不到该 ivar 而 abort。当前代码无人调用，
    //    且 show/hide/set_size/set_resizable/set_focus 都不触发它，安全。
    //    实例尺寸安全：NSPanel 不新增 ivar，尺寸与 NSWindow 一致（≤ TaoWindow = NSWindow + 1 Bool），
    //    重分类不会越界读写。
    let panel_class = kivio_overlay_panel_class();
    let already: bool = msg_send![window, isKindOfClass: panel_class];
    if !already {
        object_setClass(window, panel_class);
    }

    // 2) 非激活面板样式：点击/聚焦不激活宿主 app（Spotlight 式）。保留既有 borderless/resizable 位。
    const NONACTIVATING_PANEL: usize = 1 << 7;
    let mask: usize = msg_send![window, styleMask];
    let _: () = msg_send![window, setStyleMask: mask | NONACTIVATING_PANEL];

    // 3) collectionBehavior：跨全 Space + 进别的 App 全屏 Space + 不受 Mission Control 影响 +
    //    不进 Cmd+` 循环。清掉互斥/不需要的位（MoveToActiveSpace 与 CanJoinAllSpaces 互斥；
    //    Stationary 与 Transient 互斥）。
    const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
    const MOVE_TO_ACTIVE_SPACE: usize = 1 << 1;
    const TRANSIENT: usize = 1 << 3;
    const STATIONARY: usize = 1 << 4;
    const IGNORES_CYCLE: usize = 1 << 6;
    const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
    let behavior: usize = msg_send![window, collectionBehavior];
    let behavior = (behavior & !MOVE_TO_ACTIVE_SPACE & !TRANSIENT)
        | CAN_JOIN_ALL_SPACES
        | STATIONARY
        | IGNORES_CYCLE
        | FULL_SCREEN_AUXILIARY;
    let _: () = msg_send![window, setCollectionBehavior: behavior];

    // 4) 置于菜单栏之上以盖住全屏内容；用 status 档(25)，避开 screenSaver(1000) 那种会在
    //    错误 Space 闪一下的过高层级。
    const NS_STATUS_WINDOW_LEVEL: isize = 25;
    let _: () = msg_send![window, setLevel: NS_STATUS_WINDOW_LEVEL];

    // 5) 关键：NSPanel 默认在宿主 app 失活时自动隐藏；浮窗显示时前台是别的 App（如全屏 Chrome），
    //    不设 NO 会立刻消失。
    let _: () = msg_send![window, setHidesOnDeactivate: false];
}
