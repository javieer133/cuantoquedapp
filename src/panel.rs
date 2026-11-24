//! Floating panel opened by clicking the icon: a small borderless window with a web view that
//! shows `panel_view`'s HTML. Visual code only; the content is built by `panel_view`.
use std::{
    sync::mpsc::{Receiver, Sender, channel},
    time::{Duration, Instant},
};

use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowId},
};
use tray_icon::Rect;
use wry::{WebView, WebViewBuilder};

use crate::{
    debug_log::debug_log,
    panel_view::{PanelView, WIDTH, parse_choice, parse_height},
    settings::Choice,
};

/// A click on the icon right after the panel closed itself (focus lost) is the same click
/// that closed it: ignore it instead of reopening.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
/// Right after the panel opens, Windows may hand the focus back to the taskbar for a moment;
/// losing focus during this time must not close the panel (it would never be seen).
const SHOW_GRACE: Duration = Duration::from_millis(500);
const GAP: f64 = 6.0;
const BLANK: &str = "<!doctype html><html></html>";

/// What the user chose in the panel footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    ToggleAutostart,
    Refresh,
    Quit,
    OpenSettings,
    CloseSettings,
    Choose(Choice),
}

/// What the page itself asks of the window (handled here, not by the caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageRequest {
    Height(u32),
    Close,
}

fn parse_request(message: &str) -> Option<PageRequest> {
    match message {
        "close" => Some(PageRequest::Close),
        other => parse_height(other).map(PageRequest::Height),
    }
}

fn parse_action(message: &str) -> Option<PanelAction> {
    match message {
        "autostart" => Some(PanelAction::ToggleAutostart),
        "refresh" => Some(PanelAction::Refresh),
        "quit" => Some(PanelAction::Quit),
        "settings" => Some(PanelAction::OpenSettings),
        "back" => Some(PanelAction::CloseSettings),
        other => parse_choice(other).map(PanelAction::Choose),
    }
}

pub struct Panel {
    actions: Receiver<String>,
    window: Window,
    webview: WebView,
    html: String,
    height: u32,
    /// Where the icon was when the panel last opened, to place it again after a resize.
    icon: Option<Rect>,
    closed_by_focus_at: Option<Instant>,
    shown_at: Option<Instant>,
}

impl Panel {
    /// `None` if the system cannot create the window or the web view (the menu still works).
    pub fn new<T: 'static>(target: &EventLoopWindowTarget<T>) -> Option<Self> {
        let window = WindowBuilder::new()
            .with_title("CuantoQuedapp")
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(true)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(WIDTH, 200))
            .build(target)
            .ok()?;
        #[cfg(windows)]
        let window = {
            use tao::platform::windows::WindowExtWindows;
            // Cosmetic: if it fails, the panel just also shows in the taskbar.
            let _ = window.set_skip_taskbar(true);
            window
        };
        let (sender, actions) = channel();
        let webview = build_web_view(&window, sender)?;
        Some(Panel {
            actions,
            window,
            webview,
            html: String::new(),
            height: 200,
            icon: None,
            closed_by_focus_at: None,
            shown_at: None,
        })
    }

    /// Actions clicked since the last call. Requests of the page itself (its height, Escape)
    /// are applied here; unknown messages are ignored.
    pub fn actions(&mut self) -> Vec<PanelAction> {
        let messages: Vec<String> = self.actions.try_iter().collect();
        let mut actions = Vec::new();
        for message in messages {
            match parse_request(&message) {
                Some(PageRequest::Height(height)) => self.fit_height(height),
                Some(PageRequest::Close) => self.window.set_visible(false),
                None => actions.extend(parse_action(&message)),
            }
        }
        actions
    }

    /// The page measured itself: resize, and keep the panel next to the icon (on Windows it
    /// opens upwards, so a taller panel must move up).
    fn fit_height(&mut self, height: u32) {
        if height == self.height {
            return;
        }
        debug_log(&format!(
            "panel height {} -> {height} (measured)",
            self.height
        ));
        self.height = height;
        self.window.set_inner_size(LogicalSize::new(WIDTH, height));
        if self.window.is_visible()
            && let Some(icon) = self.icon
        {
            self.window.set_outer_position(self.position_for(icon));
        }
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Updates the content; the page is reloaded only when it actually changed. While hidden
    /// the window takes the estimated height; while visible it waits for the page to report its
    /// real one (so a visible panel does not jump twice).
    pub fn set_view(&mut self, view: &PanelView) {
        if view.html == self.html {
            return;
        }
        if !self.window.is_visible() && view.height != self.height {
            self.height = view.height;
            self.window
                .set_inner_size(LogicalSize::new(WIDTH, view.height));
        }
        if self.webview.load_html(&view.html).is_ok() {
            self.html = view.html.clone();
        }
    }

    /// Opens the panel under (Mac) or over (Windows taskbar) the icon, or closes it.
    pub fn toggle(&mut self, icon: Rect) {
        if self.window.is_visible() {
            self.window.set_visible(false);
            return;
        }
        if self
            .closed_by_focus_at
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
        {
            return;
        }
        self.icon = Some(icon);
        let position = self.position_for(icon);
        debug_log(&format!(
            "open panel at ({:.0}, {:.0}), icon at ({:.0}, {:.0}) size {}x{}, height {}",
            position.x,
            position.y,
            icon.position.x,
            icon.position.y,
            icon.size.width,
            icon.size.height,
            self.height
        ));
        self.window.set_outer_position(position);
        // Always show a freshly loaded page: whatever happened to it while hidden is gone.
        if !self.html.is_empty() {
            let _ = self.webview.load_html(&self.html);
        }
        self.window.set_visible(true);
        self.window.set_focus();
        self.shown_at = Some(Instant::now());
    }

    /// The panel loses focus when the user clicks anywhere else: close it.
    pub fn focus_lost(&mut self) {
        if !self.window.is_visible() {
            return;
        }
        if self.shown_at.is_some_and(|at| at.elapsed() < SHOW_GRACE) {
            debug_log("focus lost during the grace period: panel kept open");
            return;
        }
        debug_log("focus lost: panel closed");
        self.window.set_visible(false);
        self.closed_by_focus_at = Some(Instant::now());
    }

    fn position_for(&self, icon: Rect) -> PhysicalPosition<f64> {
        let scale = self.window.scale_factor();
        let width = f64::from(WIDTH) * scale;
        let height = f64::from(self.height) * scale;
        let screen = self
            .window
            .current_monitor()
            .or_else(|| self.window.primary_monitor())
            .map(|monitor| (monitor.position(), monitor.size()));
        let icon_center = icon.position.x + f64::from(icon.size.width) / 2.0;
        let mut x = icon_center - width / 2.0;
        let mut y = icon.position.y + f64::from(icon.size.height) + GAP * scale;
        if let Some((origin, size)) = screen {
            let (left, top) = (f64::from(origin.x), f64::from(origin.y));
            let (right, bottom) = (left + f64::from(size.width), top + f64::from(size.height));
            // Icon in the lower half (Windows taskbar): open upwards.
            if icon.position.y > top + (bottom - top) / 2.0 {
                y = icon.position.y - height - GAP * scale;
            }
            x = x.min(right - width - GAP * scale).max(left + GAP * scale);
            y = y.min(bottom - height).max(top);
        }
        PhysicalPosition::new(x, y)
    }
}

/// The panel only ever shows its own inline page: `about:blank` (macOS) or the `about:`/`data:`
/// URL that Windows reports for it. Any other address (a website, a local file) is refused,
/// so even injected markup could not take the panel elsewhere or send data out.
fn is_own_page(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.is_empty() || url.starts_with("about:") || url.starts_with("data:")
}

fn build_web_view(window: &Window, sender: Sender<String>) -> Option<WebView> {
    #[cfg(windows)]
    {
        // WebView2 keeps its profile in a folder: a per-user one, not next to the .exe.
        let folder = crate::single_instance::default_lock_path()?
            .parent()?
            .join("webview");
        let mut context = wry::WebContext::new(Some(folder));
        // The panel is not a website: no F5, no browser shortcuts.
        use wry::WebViewBuilderExtWindows;
        WebViewBuilder::new_with_web_context(&mut context)
            .with_browser_accelerator_keys(false)
            .with_html(BLANK)
            .with_navigation_handler(|url| is_own_page(&url))
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                let _ = sender.send(request.body().clone());
            })
            .build(window)
            .ok()
    }
    #[cfg(not(windows))]
    {
        WebViewBuilder::new()
            .with_html(BLANK)
            .with_navigation_handler(|url| is_own_page(&url))
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                let _ = sender.send(request.body().clone());
            })
            .build(window)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_panel_own_page_can_be_shown() {
        for own in [
            "about:blank",
            "",
            "data:text/html;charset=utf-8;base64,PGI+",
            "ABOUT:blank",
        ] {
            assert!(is_own_page(own), "{own}");
        }
        for other in [
            "https://example.com",
            "http://localhost:8080/",
            "file:///Users/x/.codex/auth.json",
            "javascript:alert(1)",
            "blob:https://example.com/1",
        ] {
            assert!(!is_own_page(other), "{other}");
        }
    }

    #[test]
    fn known_messages_become_actions_and_others_are_ignored() {
        assert_eq!(parse_action("quit"), Some(PanelAction::Quit));
        assert_eq!(
            parse_action("autostart"),
            Some(PanelAction::ToggleAutostart)
        );
        assert_eq!(parse_action("settings"), Some(PanelAction::OpenSettings));
        assert_eq!(parse_action("back"), Some(PanelAction::CloseSettings));
        assert_eq!(
            parse_action("theme:dark"),
            Some(PanelAction::Choose(Choice::Theme(
                crate::settings::Theme::Dark
            )))
        );
        assert_eq!(parse_action("rm -rf"), None);
        assert_eq!(parse_action("h:300"), None, "page requests are not actions");
        assert_eq!(parse_request("h:300"), Some(PageRequest::Height(300)));
        assert_eq!(parse_request("close"), Some(PageRequest::Close));
        assert_eq!(parse_request("quit"), None);
        assert_eq!(parse_action(""), None);
    }
}
