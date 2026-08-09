//! Immediate-mode overlay UI drawn in the SwapBuffers hook.

use crate::gl;
use crate::ipc;
use crate::lru;
use crate::mpv;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::Foundation::POINT;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Youtube,
    Friends,
    Chat,
}

pub struct UiState {
    tab: Tab,
    feed: Vec<ipc::FeedItem>,
    friends_json: String,
    chat_json: String,
    selected: Option<String>,
    scroll: f32,
    last_fetch: std::time::Instant,
    status: String,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            tab: Tab::Youtube,
            feed: Vec::new(),
            friends_json: String::new(),
            chat_json: String::new(),
            selected: None,
            scroll: 0.0,
            last_fetch: std::time::Instant::now() - std::time::Duration::from_secs(60),
            status: "F8 overlay · Esc close".into(),
        }
    }

    pub fn on_open(&mut self) {
        self.refresh_data(true);
    }

    pub fn tick_background(&mut self) {
        mpv::tick();
    }

    pub fn tick_and_draw(&mut self, hdc: HDC) {
        if self.last_fetch.elapsed() > std::time::Duration::from_secs(8) {
            self.refresh_data(false);
        }
        lru::with_textures(|c| c.pump_uploads());
        mpv::tick();

        gl::begin_overlay_frame(hdc);
        let (vw, vh) = gl::viewport_size();
        let vw = vw as f32;
        let vh = vh as f32;

        // Dim game.
        gl::fill_rect(0.0, 0.0, vw, vh, 0.0, 0.0, 0.0, 0.55);

        let panel_w = (vw * 0.72).min(960.0);
        let panel_h = (vh * 0.78).min(640.0);
        let px = (vw - panel_w) * 0.5;
        let py = (vh - panel_h) * 0.5;
        gl::fill_rect(px, py, panel_w, panel_h, 0.08, 0.09, 0.12, 0.94);

        // Rail
        let rail_w = 120.0;
        gl::fill_rect(px, py, rail_w, panel_h, 0.05, 0.06, 0.08, 1.0);
        let tabs = [
            (Tab::Youtube, "YouTube", 0),
            (Tab::Friends, "Friends", 1),
            (Tab::Chat, "Chat", 2),
        ];
        let mouse = cursor();
        let click = left_click_edge();

        for (tab, _label, i) in tabs {
            let ty = py + 24.0 + i as f32 * 48.0;
            let hot = mouse.0 >= px && mouse.0 <= px + rail_w && mouse.1 >= ty && mouse.1 <= ty + 40.0;
            let active = self.tab == tab;
            let (r, g, b) = if active {
                (0.15, 0.45, 0.85)
            } else if hot {
                (0.18, 0.2, 0.28)
            } else {
                (0.1, 0.11, 0.14)
            };
            gl::fill_rect(px + 8.0, ty, rail_w - 16.0, 40.0, r, g, b, 1.0);
            if hot && click {
                self.tab = tab;
            }
        }

        let content_x = px + rail_w + 16.0;
        let content_y = py + 16.0;
        let content_w = panel_w - rail_w - 32.0;
        let content_h = panel_h - 32.0;

        match self.tab {
            Tab::Youtube => self.draw_youtube(content_x, content_y, content_w, content_h, mouse, click),
            Tab::Friends => {
                gl::fill_rect(content_x, content_y, content_w, 40.0, 0.12, 0.14, 0.18, 1.0);
                // JSON blob as colored bars proxy for list length
                let n = self.friends_json.len().min(4000) as f32 / 4000.0;
                gl::fill_rect(content_x, content_y + 56.0, content_w * n.max(0.05), 12.0, 0.3, 0.7, 0.4, 1.0);
                self.status = format!("Friends IPC ({} bytes)", self.friends_json.len());
            }
            Tab::Chat => {
                gl::fill_rect(content_x, content_y, content_w, 40.0, 0.12, 0.14, 0.18, 1.0);
                let n = self.chat_json.len().min(4000) as f32 / 4000.0;
                gl::fill_rect(content_x, content_y + 56.0, content_w * n.max(0.05), 12.0, 0.4, 0.5, 0.9, 1.0);
                self.status = format!("Chat IPC ({} bytes)", self.chat_json.len());
            }
        }

        // Status bar
        gl::fill_rect(px, py + panel_h - 28.0, panel_w, 28.0, 0.04, 0.05, 0.07, 1.0);
        let _ = &self.status;

        gl::end_overlay_frame();
    }

    fn draw_youtube(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mouse: (f32, f32),
        click: bool,
    ) {
        let row_h = 72.0;
        let mut cy = y - self.scroll;
        for item in &self.feed {
            if cy + row_h < y || cy > y + h {
                cy += row_h + 8.0;
                continue;
            }
            let hot = mouse.0 >= x && mouse.0 <= x + w && mouse.1 >= cy && mouse.1 <= cy + row_h;
            gl::fill_rect(
                x,
                cy,
                w,
                row_h,
                if hot { 0.14 } else { 0.11 },
                if hot { 0.16 } else { 0.12 },
                if hot { 0.22 } else { 0.16 },
                1.0,
            );
            let thumb = lru::with_textures(|c| c.get_or_request(&item.id, &item.thumbnail_url))
                .flatten()
                .unwrap_or(0);
            gl::textured_rect(thumb, x + 8.0, cy + 8.0, 96.0, 56.0);
            // Title proxy bar
            let tw = (item.title.len() as f32 * 6.0).min(w - 130.0).max(40.0);
            gl::fill_rect(x + 116.0, cy + 18.0, tw, 10.0, 0.85, 0.85, 0.9, 1.0);
            gl::fill_rect(
                x + 116.0,
                cy + 36.0,
                (item.channel.len() as f32 * 5.0).min(w - 140.0).max(20.0),
                8.0,
                0.5,
                0.55,
                0.6,
                1.0,
            );
            if hot && click {
                self.selected = Some(item.id.clone());
                if let Some(url) = ipc::resolve_youtube(&item.id) {
                    match mpv::play_url(&url) {
                        Ok(()) => self.status = format!("Playing {}", item.id),
                        Err(e) => self.status = e,
                    }
                }
            }
            cy += row_h + 8.0;
        }

        // Mini-player
        if let Some((tex, pw, ph)) = mpv::texture() {
            let pw = pw as f32;
            let ph = ph as f32;
            let mx = x + w - pw - 12.0;
            let my = y + h - ph - 48.0;
            gl::fill_rect(mx - 8.0, my - 8.0, pw + 16.0, ph + 40.0, 0.05, 0.05, 0.08, 0.95);
            gl::textured_rect(tex, mx, my, pw, ph);
            // Transport hit targets
            let pause = hit(mouse, mx, my + ph + 4.0, 60.0, 22.0);
            let stop = hit(mouse, mx + 70.0, my + ph + 4.0, 60.0, 22.0);
            gl::fill_rect(mx, my + ph + 4.0, 60.0, 22.0, 0.2, 0.5, 0.3, 1.0);
            gl::fill_rect(mx + 70.0, my + ph + 4.0, 60.0, 22.0, 0.5, 0.2, 0.2, 1.0);
            if click && pause {
                mpv::toggle_pause();
            }
            if click && stop {
                mpv::stop();
                self.selected = None;
            }
        }

        // Scroll via wheel approximation: hold RMB edge — use arrow keys via GetAsyncKeyState
        unsafe {
            if GetAsyncKeyState(0x28) < 0 {
                // VK_DOWN
                self.scroll += 12.0;
            }
            if GetAsyncKeyState(0x26) < 0 {
                self.scroll = (self.scroll - 12.0).max(0.0);
            }
        }
    }

    fn refresh_data(&mut self, force: bool) {
        if !force && self.last_fetch.elapsed() < std::time::Duration::from_secs(5) {
            return;
        }
        self.last_fetch = std::time::Instant::now();
        self.feed = ipc::fetch_youtube_feed();
        self.friends_json = ipc::fetch_friends().to_string();
        self.chat_json = ipc::fetch_chat().to_string();
        self.status = format!("Feed {} · IPC ok", self.feed.len());
    }
}

fn cursor() -> (f32, f32) {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        let _ = GetCursorPos(&mut pt);
        (pt.x as f32, pt.y as f32)
    }
}

fn left_click_edge() -> bool {
    static WAS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
    let was = WAS.load(std::sync::atomic::Ordering::SeqCst);
    WAS.store(down, std::sync::atomic::Ordering::SeqCst);
    down && !was
}

fn hit(mouse: (f32, f32), x: f32, y: f32, w: f32, h: f32) -> bool {
    mouse.0 >= x && mouse.0 <= x + w && mouse.1 >= y && mouse.1 <= y + h
}
