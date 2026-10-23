use crate::model::Event;
use crate::{localization::Localization, model::*, worker::Worker};
use eframe::egui::{epaint::PathShape, *};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
const INK: Color32 = Color32::from_rgb(30, 32, 40);
const MUTED: Color32 = Color32::from_rgb(119, 124, 139);
const ACCENT: Color32 = Color32::from_rgb(55, 119, 255);
const PALE: Color32 = Color32::from_rgb(237, 240, 255);
const BG: Color32 = Color32::from_rgb(247, 247, 247);
const LINE: Color32 = Color32::from_rgb(232, 234, 240);
fn mode_columns(width: f32, count: usize) -> usize {
    (((width - 48.) / 148.).floor() as usize)
        .max(1)
        .min(count.max(1))
}
struct HomeLayout {
    cols: usize,
    rows: usize,
    hero: f32,
    noise: f32,
    eq: f32,
    extra: f32,
}
impl HomeLayout {
    fn new(width: f32, height: f32, count: usize, connected: bool, compact: bool) -> Self {
        let cols = mode_columns(width, count);
        let rows = count.div_ceil(cols).max(1);
        let dense = compact && rows > 1;
        let hero = if dense {
            102.
        } else if compact {
            164.
        } else {
            198.
        } + if connected { 0. } else { 32. };
        let noise = if dense {
            158.
        } else if compact {
            162.
        } else {
            184.
        } + (rows - 1) as f32 * 70.;
        let eq = if compact { 206. } else { 220. };
        let extra = (height - hero - noise - eq - 28.).clamp(0., 400.);
        Self {
            cols,
            rows,
            hero: hero + extra * 0.2,
            noise: noise + extra * 0.25,
            eq: eq + extra * 0.55,
            extra,
        }
    }
    fn height(&self) -> f32 {
        self.hero + self.noise + self.eq + 28.
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w.max(0.), h.max(0.)))
}
fn txt(ui: &Ui, r: Rect, text: impl Into<String>, size: f32, color: Color32, bold: bool) {
    let text = text.into();
    let font = FontId::new(
        size,
        if bold {
            FontFamily::Name("semibold".into())
        } else {
            FontFamily::Proportional
        },
    );
    let galley = ui.painter().layout(text, font, color, r.width());
    ui.painter().galley(r.min, galley, color);
}
fn card(ui: &Ui, r: Rect) {
    ui.painter()
        .rect_filled(r, 28., Color32::from_rgb(253, 253, 255));
}
fn remember_focus(ui: &Ui, response: &Response) {
    if response.has_focus() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(Id::new("page.focus"), response.rect));
    }
}
fn button(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    r: Rect,
    label: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    let response = ui.interact(
        r,
        ui.id().with(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if selected {
        PALE
    } else if response.hovered() && enabled {
        Color32::from_rgb(241, 242, 247)
    } else {
        Color32::from_rgb(248, 249, 252)
    };
    ui.painter().rect_filled(r, 12., fill);
    if selected || response.has_focus() {
        ui.painter()
            .rect_stroke(r, 12., Stroke::new(1.0_f32, ACCENT));
    }
    let color = if !enabled {
        MUTED
    } else if selected {
        ACCENT
    } else {
        INK
    };
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(14.),
        color,
    );
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, selected, label));
    remember_focus(ui, &response);
    response
}
fn primary(ui: &mut Ui, id: &str, r: Rect, label: &str) -> bool {
    let response = ui.interact(r, ui.id().with(id), Sense::click());
    ui.painter().rect_filled(
        r,
        r.height() * 0.5,
        if response.hovered() {
            Color32::from_rgb(44, 104, 237)
        } else {
            ACCENT
        },
    );
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(14.),
        Color32::WHITE,
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    remember_focus(ui, &response);
    response.clicked()
}
fn noise_mode(ui: &mut Ui, id: u8, r: Rect, label: &str, selected: bool, enabled: bool) -> bool {
    let response = ui.interact(
        r,
        ui.id().with(("mode", id)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let c = pos2(r.center().x, r.top() + 23.);
    let color = if selected {
        Color32::WHITE
    } else {
        Color32::from_rgb(154, 159, 169)
    };
    ui.painter().circle_filled(
        c,
        23.,
        if selected {
            if enabled {
                ACCENT
            } else {
                Color32::from_rgb(171, 195, 247)
            }
        } else {
            Color32::from_rgb(240, 241, 244)
        },
    );
    match id {
        0 => {
            let points = (0..=24)
                .map(|n| {
                    let a = std::f32::consts::PI + n as f32 / 24. * std::f32::consts::PI;
                    pos2(c.x + a.cos() * 9., c.y + a.sin() * 11.)
                })
                .collect();
            ui.painter()
                .add(PathShape::line(points, Stroke::new(2.0_f32, color)));
            for dx in [-9., 9.] {
                ui.painter()
                    .rect_filled(rect(c.x + dx - 2.5, c.y - 2., 5., 12.), 2., color);
            }
        }
        1 => {
            for i in 0..3 {
                let dx = (i as f32 - 1.) * 6.;
                ui.painter().line_segment(
                    [
                        pos2(c.x + dx, c.y - 7. - (i % 2) as f32 * 3.),
                        pos2(c.x + dx, c.y + 7. + (i % 2) as f32 * 3.),
                    ],
                    Stroke::new(2.5_f32, color),
                );
            }
        }
        2 => {
            ui.painter()
                .circle_stroke(c, 9., Stroke::new(2.0_f32, color));
            ui.painter().line_segment(
                [pos2(c.x - 13., c.y), pos2(c.x + 13., c.y)],
                Stroke::new(2.0_f32, color),
            );
        }
        _ => {
            ui.painter().rect_stroke(
                rect(c.x - 11., c.y - 8., 22., 16.),
                3.,
                Stroke::new(2.0_f32, color),
            );
            ui.painter().line_segment(
                [pos2(c.x - 4., c.y + 11.), pos2(c.x + 4., c.y + 11.)],
                Stroke::new(2.0_f32, color),
            );
        }
    }
    ui.painter().text(
        pos2(c.x, r.top() + 53.),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(12.),
        if selected && enabled { INK } else { MUTED },
    );
    if response.has_focus() {
        ui.painter()
            .circle_stroke(c, 27., Stroke::new(1.0_f32, ACCENT));
    }
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, enabled, selected, label));
    remember_focus(ui, &response);
    response.clicked()
}
fn toggle(ui: &mut Ui, id: &str, r: Rect, value: bool, enabled: bool, label: &str) -> bool {
    let response = ui.interact(
        r,
        ui.id().with(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let track = rect(r.right() - 46., r.center().y - 13., 46., 26.);
    ui.painter().rect_filled(
        track,
        13.,
        if value {
            ACCENT
        } else {
            Color32::from_rgb(212, 216, 225)
        },
    );
    ui.painter().circle_filled(
        pos2(
            if value {
                track.right() - 13.
            } else {
                track.left() + 13.
            },
            track.center().y,
        ),
        10.,
        Color32::WHITE,
    );
    if response.has_focus() {
        ui.painter()
            .rect_stroke(track.expand(3.), 16., Stroke::new(1.0_f32, ACCENT));
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, value, label));
    remember_focus(ui, &response);
    response.clicked()
}
fn slider(
    ui: &mut Ui,
    key: SliderKey,
    r: Rect,
    value: &mut i32,
    min: i32,
    max: i32,
    enabled: bool,
    label: &str,
) -> (bool, bool) {
    let response = ui.interact(
        r,
        ui.id().with(("slider", key)),
        if enabled {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        },
    );
    let old = *value;
    let start = r.left() + 12.;
    let end = r.right() - 12.;
    if enabled && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
    {
        if let Some(p) = response.interact_pointer_pos() {
            *value = (min as f32
                + ((p.x - start) / (end - start)).clamp(0., 1.) * (max - min) as f32)
                .round() as i32;
            response.request_focus();
        }
    }
    if enabled && response.has_focus() {
        ui.input(|i| {
            if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown) {
                *value = (*value - 1).max(min);
            }
            if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp) {
                *value = (*value + 1).min(max);
            }
            if i.key_pressed(Key::Home) {
                *value = min;
            }
            if i.key_pressed(Key::End) {
                *value = max;
            }
        });
    }
    let t = (*value - min) as f32 / (max - min).max(1) as f32;
    let y = r.center().y;
    let x = lerp(start..=end, t);
    ui.painter()
        .line_segment([pos2(start, y), pos2(end, y)], Stroke::new(5.0_f32, LINE));
    ui.painter().line_segment(
        [pos2(start, y), pos2(x, y)],
        Stroke::new(5.0_f32, if enabled { ACCENT } else { MUTED }),
    );
    ui.painter()
        .circle_filled(pos2(x, y + 1.), 10., Color32::from_black_alpha(12));
    ui.painter().circle_filled(pos2(x, y), 9., Color32::WHITE);
    ui.painter().circle_stroke(
        pos2(x, y),
        9.,
        Stroke::new(2.0_f32, if enabled { ACCENT } else { MUTED }),
    );
    if response.has_focus() {
        ui.painter().circle_stroke(
            pos2(x, y),
            13.,
            Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.45)),
        );
    }
    response.widget_info(|| WidgetInfo::slider(enabled, *value as f64, label));
    remember_focus(ui, &response);
    let focused = response.has_focus();
    let release = response.drag_stopped()
        || response.clicked()
        || ui.input(|i| {
            focused
                && [
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::Home,
                    Key::End,
                ]
                .iter()
                .any(|k| i.key_released(*k))
        });
    (*value != old, release)
}
fn headphone_texture(ctx: &Context) -> TextureHandle {
    let image = image::load_from_memory(include_bytes!("../assets/qc-ultra-2-black.webp"))
        .expect("embedded headphone artwork")
        .into_rgba8();
    let mut bounds = (image.width(), image.height(), 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] != 0 {
            bounds = (
                bounds.0.min(x),
                bounds.1.min(y),
                bounds.2.max(x + 1),
                bounds.3.max(y + 1),
            );
        }
    }
    let image = image::imageops::crop_imm(
        &image,
        bounds.0,
        bounds.1,
        bounds.2 - bounds.0,
        bounds.3 - bounds.1,
    )
    .to_image();
    let image = image::DynamicImage::ImageRgba8(image)
        .resize(384, 384, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    ctx.load_texture(
        "qc-ultra-2-black",
        ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        TextureOptions::LINEAR,
    )
}
fn headphones(ui: &Ui, texture: &TextureHandle, r: Rect) -> Rect {
    let size = texture.size_vec2();
    let scale = (r.width() / size.x).min(r.height() / size.y);
    let image = Rect::from_center_size(r.center(), size * scale);
    ui.painter().image(
        texture.id(),
        image,
        Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
        Color32::WHITE,
    );
    image
}
pub fn app_icon() -> IconData {
    let image = image::load_from_memory(include_bytes!("../assets/app.png"))
        .expect("embedded app icon")
        .into_rgba8();
    IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

fn setup(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let font_dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()))
        .join("Fonts");
    for (name, path) in [
        ("ui", "segoeui.ttf"),
        ("cjk", "msyh.ttc"),
        ("strong", "seguisb.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(font_dir.join(path)) {
            fonts
                .font_data
                .insert(name.into(), FontData::from_owned(bytes));
        }
    }
    let mut regular = vec![];
    for name in ["ui", "cjk"] {
        if fonts.font_data.contains_key(name) {
            regular.push(name.into());
        }
    }
    regular.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts
        .families
        .insert(FontFamily::Proportional, regular.clone());
    let mut bold = vec![];
    if fonts.font_data.contains_key("strong") {
        bold.push("strong".into());
    }
    bold.extend(regular);
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), bold);

    ctx.set_fonts(fonts);

    let mut style = Style::default();
    style.visuals = Visuals::light();
    style.visuals.panel_fill = BG;
    style.visuals.override_text_color = Some(INK);
    style.spacing.item_spacing = vec2(12., 12.);
    style.animation_time = 0.;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.));
    ctx.set_style(style);
}
pub struct Dashboard {
    pub state: State,
    pub drafts: Drafts,
    pub language: Localization,
    pub page: usize,
    headphone_texture: TextureHandle,
    worker: Option<Worker>,
    pub error: Option<(String, Instant)>,
    notice: Option<(String, Instant)>,
    pending_device: Option<(String, DeviceAction)>,
    scroll: f32,
    confirm: Option<Command>,
    rename: String,
    quitting: bool,
    tray: Option<crate::platform::Tray>,
    first_frame: bool,
}
impl Dashboard {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup(&cc.egui_ctx);
        let worker = Worker::start(cc.egui_ctx.clone());
        let tray = match crate::platform::Tray::start(cc) {
            Ok(tray) => Some(tray),
            Err(error) => {
                crate::worker::log(&format!("Tray unavailable; close will exit: {error:#}"));
                None
            }
        };
        Self {
            state: State::default(),
            drafts: Drafts::default(),
            language: Localization::new(),
            page: 0,
            headphone_texture: headphone_texture(&cc.egui_ctx),
            worker: Some(worker),
            error: None,
            notice: None,
            pending_device: None,
            scroll: 0.,
            confirm: None,
            rename: String::new(),
            quitting: false,
            tray,
            first_frame: true,
        }
    }
    fn t(&self, key: &str) -> String {
        self.language.text(key)
    }
    fn device_name(&self, address: &str) -> String {
        self.state
            .devices
            .iter()
            .find(|d| d.mac == address)
            .map_or_else(|| address.to_owned(), |d| d.name.clone())
    }
    fn send(&self, c: Command) {
        if let Some(w) = &self.worker {
            w.sender.send(c, true);
        }
    }
    fn edit(&mut self, key: SliderKey, value: i32, release: bool) {
        let e = self.drafts.edit(key, value);
        if let Some(w) = &self.worker {
            w.sender.send(Command::Slider(e), release);
        }
    }
    fn mode_name(&self, id: u8) -> String {
        if let Some(name) = self.state.modes.get(&id) {
            let key = format!("term.{}", name.to_lowercase());
            let translated = self.t(&key);
            if translated != key {
                translated
            } else {
                name.clone()
            }
        } else {
            self.t("term.custom")
        }
    }
    fn events(&mut self, ctx: &Context) {
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        for e in events {
            self.apply_event(ctx, e);
        }
        if let Some(tray) = &self.tray {
            for e in tray.events.try_iter() {
                match e {
                    Event::Show => {
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                    Event::Quit => self.quitting = true,
                    _ => {}
                }
            }
        }
    }
    fn apply_event(&mut self, ctx: &Context, e: Event) {
        match e {
            Event::ResetAck(edits) => {
                for edit in edits {
                    self.drafts.complete(edit);
                }
            }
            Event::State(state) => {
                if self.state.connected
                    && state.connected
                    && self.state.active.is_some()
                    && self.state.active != state.active
                {
                    if let Some(address) = &state.active {
                        if self
                            .pending_device
                            .as_ref()
                            .is_some_and(|(target, _)| target == address)
                        {
                            self.state = *state;
                            return;
                        }
                        let name = state
                            .devices
                            .iter()
                            .find(|d| d.mac == *address)
                            .map_or(address.as_str(), |d| d.name.as_str());
                        self.notice = Some((
                            self.t("devices.sourceChanged").replace("{0}", name),
                            Instant::now(),
                        ));
                    }
                }
                self.state = *state;
            }
            Event::DeviceAck(address, action) => {
                if self
                    .pending_device
                    .as_ref()
                    .is_some_and(|(mac, _)| *mac == address)
                {
                    self.pending_device = None;
                }
                let key = match action {
                    DeviceAction::Connect => "devices.connected",
                    DeviceAction::Disconnect => "devices.disconnected",
                };
                self.notice = Some((
                    self.t(key).replace("{0}", &self.device_name(&address)),
                    Instant::now(),
                ));
            }
            Event::DeviceFailed(address, action) => {
                if self
                    .pending_device
                    .as_ref()
                    .is_some_and(|(mac, _)| *mac == address)
                {
                    self.pending_device = None;
                }
                let key = match action {
                    DeviceAction::Connect => "devices.connectFailed",
                    DeviceAction::Disconnect => "devices.disconnectFailed",
                };
                self.error = Some((
                    self.t(key).replace("{0}", &self.device_name(&address)),
                    Instant::now(),
                ));
            }
            Event::Ack(edit, value) => {
                if self.drafts.complete(edit) {
                    match edit.key {
                        SliderKey::Cnc => self.state.cnc = value,
                        SliderKey::Eq(id) => {
                            if let Some(b) = self.state.eq.iter_mut().find(|b| b.id == id) {
                                b.value = value;
                            }
                        }
                    }
                }
            }
            Event::Failed(edit, error) => {
                if let Some(e) = edit {
                    self.drafts.complete(e);
                }
                self.error = Some((error, Instant::now()));
            }
            Event::Rejected(edit) => {
                if let Some(edit) = edit {
                    self.drafts.complete(edit);
                }
                self.error = Some((self.t("error.rejected"), Instant::now()));
            }
            Event::Disconnected(error) => {
                self.state.connected = false;
                self.state.playback = None;
                self.pending_device = None;
                self.drafts.disconnect();
                if !error.is_empty() {
                    crate::worker::log(&error);
                }
            }
            Event::Show => {
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            Event::Quit => self.quitting = true,
        }
    }
    pub fn render(&mut self, ctx: &Context) {
        ctx.data_mut(|data| data.remove::<Rect>(Id::new("page.focus")));
        CentralPanel::default()
            .frame(Frame::none().fill(BG))
            .show(ctx, |ui| {
                let all = ui.max_rect();
                let compact = all.height() < 730.;
                let margin = (all.width() * 0.045).clamp(24., 80.);
                let page_width = all.width() - margin * 2.;
                let x = all.left() + margin;
                txt(
                    ui,
                    rect(x, 20., 245., 28.),
                    "Bose Headphone Control",
                    if all.width() < 900. { 15. } else { 17. },
                    INK,
                    true,
                );
                let nav_width = if all.width() < 900. { 115. } else { 126. };
                let nav_x = x + page_width - nav_width * 4.;
                for (i, key) in ["nav.home", "nav.devices", "nav.settings", "nav.about"]
                    .iter()
                    .enumerate()
                {
                    let r = rect(nav_x + i as f32 * nav_width, 14., nav_width - 6., 36.);
                    if button(ui, ("nav", i), r, &self.t(key), self.page == i, true).clicked() {
                        self.page = i;
                        self.scroll = 0.;
                    }
                }
                let top = if self.page == 0 { 66. } else { 126. };
                let content = rect(x, top, page_width, all.height() - top - 18.);
                if self.page != 0 {
                    let name = ["home", "devices", "settings", "about"][self.page];
                    txt(
                        ui,
                        rect(x + 4., 73., page_width - 150., 36.),
                        self.t(&format!("page.{name}")),
                        27.,
                        INK,
                        true,
                    );
                    if button(
                        ui,
                        "reconnect",
                        rect(x + page_width - 122., 74., 122., 34.),
                        &self.t("action.reconnect"),
                        false,
                        true,
                    )
                    .clicked()
                    {
                        self.drafts.disconnect();
                        self.pending_device = None;
                        self.send(Command::Reconnect);
                    }
                }
                let wide = page_width >= 1240.;
                let favorite_width = if wide {
                    (page_width - 20.) * 0.5
                } else {
                    page_width
                };
                let mode_cols = ((favorite_width - 44.) / 145.).floor().max(1.) as usize;
                let favorite_rows = self.state.modes.len().div_ceil(mode_cols);
                let home = HomeLayout::new(
                    page_width,
                    content.height(),
                    self.state.modes.len(),
                    self.state.connected,
                    compact,
                );
                let required = match self.page {
                    0 => home.height(),
                    1 if wide => (74. + self.state.devices.len().max(1) as f32 * 76.).max(216.),
                    1 => 286. + self.state.devices.len().max(1) as f32 * 76.,
                    2 if wide => (140. + favorite_rows.saturating_sub(1) as f32 * 44.).max(346.),
                    2 => {
                        (if compact { 478. } else { 502. })
                            + favorite_rows.saturating_sub(1) as f32 * 44.
                    }
                    _ if wide => 260.,
                    _ => 530.,
                };
                let max_scroll = (required - content.height()).max(0.);
                if ui.rect_contains_pointer(content) {
                    self.scroll =
                        (self.scroll - ui.input(|i| i.smooth_scroll_delta.y)).clamp(0., max_scroll);
                }
                ui.input(|i| {
                    if i.key_pressed(Key::PageDown) {
                        self.scroll = (self.scroll + content.height() * 0.8).min(max_scroll);
                    }
                    if i.key_pressed(Key::PageUp) {
                        self.scroll = (self.scroll - content.height() * 0.8).max(0.);
                    }
                    if i.modifiers.ctrl && i.key_pressed(Key::Home) {
                        self.scroll = 0.;
                    }
                    if i.modifiers.ctrl && i.key_pressed(Key::End) {
                        self.scroll = max_scroll;
                    }
                });
                self.scroll = self.scroll.min(max_scroll);
                let old_clip = ui.clip_rect();
                ui.set_clip_rect(content.intersect(old_clip));
                let origin = pos2(x, content.top() - self.scroll);
                match self.page {
                    0 => self.home(ui, origin, page_width, compact, &home),
                    1 => self.devices(ui, origin, page_width),
                    2 => self.settings(ui, origin, page_width, compact),
                    _ => self.about(ui, origin, page_width),
                }
                if ui.input(|i| i.key_pressed(Key::Tab)) {
                    if let Some(focus) =
                        ctx.data(|data| data.get_temp::<Rect>(Id::new("page.focus")))
                    {
                        if focus.top() < content.top() && focus.bottom() > 60. {
                            self.scroll =
                                (self.scroll - (content.top() - focus.top()) - 8.).max(0.);
                            ctx.request_repaint();
                        } else if focus.bottom() > content.bottom() {
                            self.scroll = (self.scroll + focus.bottom() - content.bottom() + 8.)
                                .min(max_scroll);
                            ctx.request_repaint();
                        }
                    }
                }
                ui.set_clip_rect(old_clip);
                if let Some((error, when)) = &self.error {
                    if when.elapsed() < Duration::from_secs(7) {
                        ctx.request_repaint_after(Duration::from_secs(7) - when.elapsed());
                        let r = rect(x, all.bottom() - 58., page_width, 40.);
                        ui.painter()
                            .rect_filled(r, 12., Color32::from_rgb(255, 237, 236));
                        txt(
                            ui,
                            r.shrink2(vec2(12., 9.)),
                            format!("{} · {}", self.t("error.action"), error),
                            13.,
                            Color32::from_rgb(170, 55, 47),
                            false,
                        );
                    } else {
                        self.error = None;
                    }
                }
                if self.error.is_none() {
                    if let Some((notice, when)) = &self.notice {
                        if when.elapsed() < Duration::from_secs(5) {
                            ctx.request_repaint_after(Duration::from_secs(5) - when.elapsed());
                            let r = rect(x, all.bottom() - 58., page_width, 40.);
                            ui.painter().rect_filled(r, 12., PALE);
                            txt(ui, r.shrink2(vec2(12., 9.)), notice, 13., ACCENT, false);
                        } else {
                            self.notice = None;
                        }
                    }
                }
            });
        self.dialog(ctx);
    }
    fn home(&mut self, ui: &mut Ui, p: Pos2, w: f32, compact: bool, layout: &HomeLayout) {
        let cols = layout.cols;
        let rows = layout.rows;
        let dense = compact && rows > 1;
        let hero_h = layout.hero;
        let hero_y = p.y + layout.extra * 0.1;
        let center = p.x + w * 0.5;
        let art = if dense {
            48.
        } else {
            92. + layout.extra * 0.18
        };
        headphones(
            ui,
            &self.headphone_texture,
            rect(center - art * 0.5, hero_y, art, art),
        );
        let art_extra = if dense { 0. } else { art - 92. };
        let name = if self.state.name.is_empty() {
            self.t("device.defaultName")
        } else {
            self.state.name.clone()
        };
        ui.painter().text(
            pos2(center, hero_y + art_extra + if dense { 52. } else { 102. }),
            Align2::CENTER_TOP,
            name,
            FontId::proportional(if dense {
                24.
            } else if compact {
                28.
            } else {
                32.
            }),
            INK,
        );
        let status = if self.state.connected {
            self.t("status.connected")
        } else {
            self.t("status.waiting")
        };
        let status = if let Some(battery) = self.state.battery.filter(|_| self.state.connected) {
            format!("{status}  ·  {} {battery}%", self.t("battery.label"))
        } else {
            status
        };
        ui.painter().text(
            pos2(
                center,
                hero_y
                    + art_extra
                    + if dense {
                        86.
                    } else if compact {
                        139.
                    } else {
                        145.
                    },
            ),
            Align2::CENTER_TOP,
            status,
            FontId::proportional(13.),
            MUTED,
        );
        if !self.state.connected {
            let r = rect(
                center - 128.,
                hero_y + art_extra + if dense { 111. } else { 164. },
                256.,
                if dense { 28. } else { 36. },
            );
            if primary(ui, "hero.connect", r, &self.t("action.connect")) {
                self.drafts.disconnect();
                self.send(Command::Reconnect);
            }
        }
        let noise_h = layout.noise;
        let y = p.y + hero_h + 14.;
        let noise_card = rect(p.x, y, w, noise_h);
        card(ui, noise_card);
        txt(
            ui,
            rect(p.x + 24., y + 16., w - 48., 25.),
            self.t("noise.title"),
            17.,
            INK,
            true,
        );
        let tile = (w - 48.) / cols as f32;
        for (i, id) in self.state.modes.clone().keys().enumerate() {
            let r = rect(
                p.x + 24. + (i % cols) as f32 * tile,
                y + 42. + layout.extra * 0.1125 + (i / cols) as f32 * 70.,
                tile,
                70.,
            );
            if noise_mode(
                ui,
                *id,
                r,
                &self.mode_name(*id),
                self.state.mode == *id,
                self.state.connected,
            ) {
                self.send(Command::Mode(*id));
            }
        }
        let sy = y + noise_h - 49.;
        let mut value = self.drafts.value(SliderKey::Cnc, self.state.cnc);
        let r = rect(p.x + 22., sy, w - 44., 27.);
        let (changed, release) = slider(
            ui,
            SliderKey::Cnc,
            r,
            &mut value,
            0,
            10,
            self.state.connected,
            &self.t("noise.level"),
        );
        if changed || release && self.drafts.values.contains_key(&SliderKey::Cnc) {
            self.edit(SliderKey::Cnc, value, release);
        }
        txt(
            ui,
            rect(p.x + 32., sy + 27., w * 0.5, 18.),
            self.t("noise.quietEnd"),
            11.,
            MUTED,
            false,
        );
        ui.painter().text(
            pos2(p.x + w - 32., sy + 27.),
            Align2::RIGHT_TOP,
            self.t("noise.awareEnd"),
            FontId::proportional(11.),
            MUTED,
        );
        let y = y + noise_h + 14.;
        let left = (w - 16.) * 0.51;
        let right = w - left - 16.;
        let eq_h = layout.eq;
        let eq_card = rect(p.x, y, left, eq_h);
        card(ui, eq_card);
        txt(
            ui,
            rect(p.x + 24., y + 18., left - 130., 26.),
            self.t("eq.title"),
            17.,
            INK,
            true,
        );
        let reset = rect(p.x + left - 84., y + 15., 62., 30.);
        if button(
            ui,
            "eq.reset",
            reset,
            &self.t("action.resetEq"),
            false,
            self.state.connected && !self.state.eq.is_empty(),
        )
        .clicked()
        {
            self.drafts.reset_eq();
            let edits = self
                .state
                .eq
                .clone()
                .iter()
                .map(|b| {
                    self.drafts
                        .edit(SliderKey::Eq(b.id), 0i32.clamp(b.min, b.max))
                })
                .collect();
            self.send(Command::ResetEq(edits));
        }
        let gap = if compact { 48. } else { 52. } + layout.extra * 0.275;
        for (i, b) in self.state.eq.clone().iter().enumerate() {
            let yy = y + 56. + i as f32 * gap;
            let key = SliderKey::Eq(b.id);
            let mut value = self.drafts.value(key, b.value);
            let name = self.t(match b.id {
                0 => "term.bass",
                1 => "term.mid",
                _ => "term.treble",
            });
            txt(
                ui,
                rect(p.x + 24., yy, left - 48., 20.),
                &name,
                13.,
                INK,
                false,
            );
            ui.painter().text(
                pos2(p.x + left - 24., yy),
                Align2::RIGHT_TOP,
                format!("{value:+} dB"),
                FontId::proportional(13.),
                ACCENT,
            );
            let r = rect(p.x + 18., yy + 18., left - 36., 28.);
            let (changed, release) = slider(
                ui,
                key,
                r,
                &mut value,
                b.min,
                b.max,
                self.state.connected,
                &name,
            );
            if changed || release && self.drafts.values.contains_key(&key) {
                self.edit(key, value, release);
            }
        }
        if self.state.eq.is_empty() {
            txt(
                ui,
                rect(p.x + 24., y + 74., left - 48., 70.),
                self.t("eq.waiting"),
                14.,
                MUTED,
                false,
            );
        }
        let x = p.x + left + 16.;
        let sh = if compact { 118. } else { 130. } + layout.extra * 0.286;
        let spatial_card = rect(x, y, right, sh);
        card(ui, spatial_card);
        txt(
            ui,
            rect(x + 24., y + 18., right - 48., 25.),
            self.t("spatial.title"),
            17.,
            INK,
            true,
        );
        txt(
            ui,
            rect(x + 24., y + 49., right - 48., 30.),
            self.t("spatial.description"),
            12.,
            MUTED,
            false,
        );
        let sw = (right - 48. - 12.) / 3.;
        for (i, key) in ["spatial.off", "spatial.fixed", "spatial.head"]
            .iter()
            .enumerate()
        {
            if button(
                ui,
                ("spatial", i),
                rect(x + 24. + i as f32 * (sw + 6.), y + sh - 48., sw, 32.),
                &self.t(key),
                self.state.spatial == i as u8,
                self.state.connected,
            )
            .clicked()
            {
                self.send(Command::Spatial(i as u8));
            }
        }
        let wy = y + sh + 14.;
        let wh = eq_h - sh - 14.;
        let wind_card = rect(x, wy, right, wh);
        card(ui, wind_card);
        let wind_y = wy + (wh - 76.).max(0.) * 0.5;
        txt(
            ui,
            rect(x + 24., wind_y + 15., right - 110., 23.),
            self.t("wind.title"),
            16.,
            INK,
            true,
        );
        txt(
            ui,
            rect(x + 24., wind_y + 43., right - 110., 28.),
            self.t("wind.description"),
            11.,
            MUTED,
            false,
        );
        if toggle(
            ui,
            "wind",
            rect(x + right - 72., wind_y + 14., 48., 46.),
            self.state.wind,
            self.state.connected,
            &self.t("wind.title"),
        ) {
            self.send(Command::Wind(!self.state.wind));
        }
    }
    fn devices(&mut self, ui: &mut Ui, p: Pos2, w: f32) {
        let wide = w >= 1240.;
        let origin = p;
        let full_width = w;
        let w = if wide { (w - 20.) * 0.42 } else { w };
        card(ui, rect(p.x, p.y, w, 90.));
        txt(
            ui,
            rect(p.x + 22., p.y + 17., w - 110., 26.),
            self.t("multi.title"),
            18.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 50., w - 110., 22.),
            self.t("multi.description"),
            13.,
            MUTED,
            false,
        );
        if toggle(
            ui,
            "multipoint",
            rect(p.x + w - 70., p.y + 20., 48., 46.),
            self.state.multipoint == Some(true),
            self.state.connected && self.state.multipoint.is_some(),
            &self.t("multi.title"),
        ) {
            self.send(Command::Multipoint(self.state.multipoint != Some(true)));
        }
        let p = if wide { pos2(p.x + w + 20., p.y) } else { p };
        let w = if wide { full_width - w - 20. } else { w };
        let y = p.y + if wide { 0. } else { 110. };
        txt(
            ui,
            rect(p.x + 4., y, w - 130., 28.),
            self.t("devices.title"),
            18.,
            INK,
            true,
        );
        if button(
            ui,
            "refresh",
            rect(p.x + w - 110., y - 2., 110., 32.),
            &self.t("action.refresh"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.send(Command::Refresh);
        }
        for (i, d) in self.state.devices.clone().iter().enumerate() {
            let r = rect(p.x, y + 50. + i as f32 * 76., w, 64.);
            let source =
                self.state.connected && d.connected && self.state.active.as_ref() == Some(&d.mac);
            let active = source
                && matches!(
                    self.state.playback,
                    Some(Playback::Playing | Playback::Seeking)
                );
            card(ui, r);
            ui.painter().circle_filled(
                pos2(r.left() + 31., r.center().y),
                16.,
                if active { PALE } else { BG },
            );
            ui.painter().rect_stroke(
                rect(r.left() + 22., r.top() + 23., 18., 13.),
                3.,
                Stroke::new(1.5_f32, if active { ACCENT } else { MUTED }),
            );
            txt(
                ui,
                rect(r.left() + 62., r.top() + 11., w - 210., 23.),
                &d.name,
                15.,
                INK,
                true,
            );
            txt(
                ui,
                rect(r.left() + 62., r.top() + 37., w - 210., 18.),
                if source {
                    self.t(match self.state.playback {
                        Some(Playback::Playing | Playback::Seeking) => "devices.playing",
                        Some(Playback::Paused) => "devices.paused",
                        Some(Playback::Stopped) => "devices.stopped",
                        None => "devices.active",
                    })
                } else if d.connected {
                    self.t("status.connected")
                } else {
                    self.t("status.disconnected")
                },
                11.,
                MUTED,
                false,
            );
            let pending = self
                .pending_device
                .as_ref()
                .filter(|(mac, _)| mac == &d.mac);
            let action = if let Some((_, action)) = pending {
                self.t(match action {
                    DeviceAction::Connect => "devices.connecting",
                    DeviceAction::Disconnect => "devices.disconnecting",
                })
            } else if d.connected {
                self.t(if source {
                    "devices.disconnect"
                } else {
                    "status.connected"
                })
            } else {
                self.t("devices.connect")
            };
            if button(
                ui,
                ("source", &d.mac),
                rect(r.right() - 164., r.top() + 16., 146., 32.),
                &action,
                false,
                self.state.connected && self.pending_device.is_none() && (!d.connected || source),
            )
            .clicked()
            {
                let action = if !d.connected {
                    DeviceAction::Connect
                } else {
                    DeviceAction::Disconnect
                };
                self.pending_device = Some((d.mac.clone(), action));
                self.error = None;
                self.notice = None;
                self.send(match action {
                    DeviceAction::Connect => Command::ConnectDevice(d.mac.clone()),
                    DeviceAction::Disconnect => Command::DisconnectDevice(d.mac.clone()),
                });
            }
        }
        if self.state.devices.is_empty() {
            card(ui, rect(p.x, y + 50., w, 70.));
            txt(
                ui,
                rect(p.x + 22., y + 72., w - 44., 30.),
                self.t("devices.waiting"),
                14.,
                MUTED,
                false,
            );
        }
        let y = if wide {
            origin.y + 110.
        } else {
            y + 86. + self.state.devices.len().max(1) as f32 * 76.
        };
        let p = if wide { origin } else { p };
        let w = if wide { (full_width - 20.) * 0.42 } else { w };
        card(ui, rect(p.x, y, w, 90.));
        txt(
            ui,
            rect(p.x + 22., y + 16., w - 200., 25.),
            self.t("pair.title"),
            17.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., y + 49., w - 200., 28.),
            self.t("pair.description"),
            12.,
            MUTED,
            false,
        );
        if button(
            ui,
            "pair",
            rect(p.x + w - 167., y + 28., 145., 36.),
            &self.t("action.pair"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.confirm = Some(Command::Pair);
        }
    }
    fn settings(&mut self, ui: &mut Ui, p: Pos2, w: f32, compact: bool) {
        let wide = w >= 1240.;
        let w = if wide { (w - 20.) * 0.5 } else { w };
        let wear_h = if compact { 190. } else { 200. };
        let calls_h = if compact { 120. } else { 130. };
        let gap = if compact { 14. } else { 16. };
        card(ui, rect(p.x, p.y, w, wear_h));
        txt(
            ui,
            rect(p.x + 22., p.y + 18., w - 44., 28.),
            self.t("wear.title"),
            18.,
            INK,
            true,
        );
        for (i, (key, desc, value)) in [
            ("wear.pause", "wear.pauseDescription", self.state.pause),
            ("wear.answer", "wear.answerDescription", self.state.answer),
        ]
        .into_iter()
        .enumerate()
        {
            let y = p.y
                + if compact {
                    62. + i as f32 * 62.
                } else {
                    66. + i as f32 * 65.
                };
            txt(
                ui,
                rect(p.x + 22., y, w - 110., 25.),
                self.t(key),
                15.,
                INK,
                true,
            );
            txt(
                ui,
                rect(p.x + 22., y + 29., w - 110., 26.),
                self.t(desc),
                12.,
                MUTED,
                false,
            );
            if toggle(
                ui,
                key,
                rect(p.x + w - 70., y, 48., 46.),
                value == Some(true),
                self.state.connected && value.is_some(),
                &self.t(key),
            ) {
                self.send(if i == 0 {
                    Command::Pause(value != Some(true))
                } else {
                    Command::Answer(value != Some(true))
                });
            }
        }
        let y = p.y + wear_h + gap;
        card(ui, rect(p.x, y, w, calls_h));
        txt(
            ui,
            rect(p.x + 22., y + 18., w - 44., 28.),
            self.t("calls.title"),
            18.,
            INK,
            true,
        );
        let sw = (w - 44. - 24.) / 4.;
        for (i, (value, key)) in [
            (0, "term.off"),
            (3, "term.low"),
            (2, "term.medium"),
            (1, "term.high"),
        ]
        .iter()
        .enumerate()
        {
            if button(
                ui,
                ("sidetone", value),
                rect(p.x + 22. + i as f32 * (sw + 8.), y + calls_h - 61., sw, 38.),
                &self.t(key),
                self.state.sidetone == Some(*value),
                self.state.connected && self.state.sidetone.is_some(),
            )
            .clicked()
            {
                self.send(Command::Sidetone(*value));
            }
        }
        let y = if wide { p.y } else { y + calls_h + gap };
        let p = if wide { pos2(p.x + w + 20., p.y) } else { p };
        let count = self.state.modes.keys().filter(|id| **id < 8).count();
        let cols = (((w - 44.) / 145.).floor() as usize)
            .max(1)
            .min(count.max(1));
        let rows = count.div_ceil(cols).max(1);
        card(
            ui,
            rect(p.x, y, w, 140. + rows.saturating_sub(1) as f32 * 44.),
        );
        txt(
            ui,
            rect(p.x + 22., y + 18., w - 44., 28.),
            self.t("favorites.title"),
            18.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., y + 51., w - 44., 24.),
            self.t("favorites.description"),
            12.,
            MUTED,
            false,
        );
        let modes: Vec<_> = self
            .state
            .modes
            .keys()
            .filter(|id| **id < 8)
            .copied()
            .collect();
        let sw = (w - 44. - (cols - 1) as f32 * 8.) / cols as f32;
        for (i, id) in modes.iter().enumerate() {
            let r = rect(
                p.x + 22. + (i % cols) as f32 * (sw + 8.),
                y + 87. + (i / cols) as f32 * 44.,
                sw,
                32.,
            );
            if button(
                ui,
                ("favorite", id),
                r,
                &self.mode_name(*id),
                self.state.favorites & (1 << id) != 0,
                self.state.connected,
            )
            .clicked()
            {
                self.send(Command::Favorite(*id));
            }
        }
    }
    fn about(&mut self, ui: &mut Ui, p: Pos2, w: f32) {
        let wide = w >= 1240.;
        let full_width = w;
        let w = if wide { (w - 20.) * 0.55 } else { w };
        card(ui, rect(p.x, p.y, w, 258.));
        txt(
            ui,
            rect(p.x + 22., p.y + 19., w - 150., 30.),
            if self.state.name.is_empty() {
                self.t("device.defaultName")
            } else {
                self.state.name.clone()
            },
            20.,
            INK,
            true,
        );
        if button(
            ui,
            "rename",
            rect(p.x + w - 104., p.y + 18., 82., 32.),
            &self.t("action.rename"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.rename = self.state.name.clone();
            self.confirm = Some(Command::Rename(String::new()));
        }
        for (i, (key, value)) in [
            ("about.firmware", self.state.firmware.clone()),
            ("about.connection", self.state.transport.clone()),
        ]
        .iter()
        .enumerate()
        {
            let y = p.y + 77. + i as f32 * 42.;
            txt(
                ui,
                rect(p.x + 22., y, 150., 25.),
                self.t(key),
                13.,
                MUTED,
                false,
            );
            txt(
                ui,
                rect(p.x + 200., y, w - 222., 25.),
                if value.is_empty() { "—" } else { value },
                13.,
                INK,
                false,
            );
        }
        ui.painter().line_segment(
            [pos2(p.x + 22., p.y + 169.), pos2(p.x + w - 22., p.y + 169.)],
            Stroke::new(1.0_f32, LINE),
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 190., 160., 26.),
            self.t("language.title"),
            15.,
            INK,
            true,
        );
        let selected = if self.language.preference == "system" {
            self.t("language.system")
        } else {
            self.language
                .packs
                .get(&self.language.preference)
                .map_or(self.language.preference.clone(), |p| p.native_name.clone())
        };
        let mut choice = self.language.preference.clone();
        ui.allocate_new_ui(
            UiBuilder::new().max_rect(rect(p.x + 200., p.y + 183., w - 222., 52.)),
            |ui| {
                ui.spacing_mut().button_padding = vec2(14., 10.);
                let visuals = ui.visuals_mut();
                for widget in [
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                ] {
                    widget.rounding = Rounding::same(12.);
                    widget.bg_stroke = Stroke::NONE;
                }
                visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(245, 246, 250);
                visuals.widgets.hovered.weak_bg_fill = PALE;
                ComboBox::from_id_salt("language")
                    .selected_text(selected)
                    .width(230.)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut choice,
                            "system".into(),
                            self.t("language.system"),
                        );
                        for pack in self.language.packs.values() {
                            ui.selectable_value(&mut choice, pack.code.clone(), &pack.native_name);
                        }
                    });
            },
        );
        if choice != self.language.preference {
            if let Err(e) = self.language.set(choice) {
                self.error = Some((e.to_string(), Instant::now()));
            }
        }
        txt(
            ui,
            rect(p.x + 22., p.y + 230., w - 44., 20.),
            format!("{} {}", crate::APP_NAME, crate::APP_VERSION),
            11.,
            MUTED,
            false,
        );
        let p = if wide { pos2(p.x + w + 20., p.y) } else { p };
        let w = if wide { full_width - w - 20. } else { w };
        for (i, (title, description, action)) in [
            (
                "diagnostics.title",
                "diagnostics.description",
                "action.logs",
            ),
            ("power.title", "power.description", "action.power"),
            ("exit.title", "exit.description", "action.exit"),
        ]
        .iter()
        .enumerate()
        {
            let y = p.y + if wide { 0. } else { 270. } + i as f32 * 90.;
            card(ui, rect(p.x, y, w, 80.));
            txt(
                ui,
                rect(p.x + 22., y + 17., w - 218., 26.),
                self.t(title),
                16.,
                INK,
                true,
            );
            txt(
                ui,
                rect(p.x + 22., y + 48., w - 218., 26.),
                self.t(description),
                12.,
                MUTED,
                false,
            );
            if button(
                ui,
                ("about", i),
                rect(p.x + w - 164., y + 25., 142., 36.),
                &self.t(action),
                false,
                i != 1 || self.state.connected,
            )
            .clicked()
            {
                match i {
                    0 => {
                        let path = crate::localization::data_dir().join("logs");
                        let _ = std::fs::create_dir_all(&path);
                        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
                    }
                    1 => self.confirm = Some(Command::Power),
                    _ => self.quitting = true,
                }
            }
        }
    }
    fn dialog(&mut self, ctx: &Context) {
        if let Some(command) = self.confirm.clone() {
            let rename = matches!(command, Command::Rename(_));
            let title = self.t(if rename {
                "dialog.renameTitle"
            } else if matches!(command, Command::Power) {
                "action.power"
            } else {
                "dialog.pairTitle"
            });
            Window::new(title)
                .id(Id::new("confirmation"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(350.)
                .show(ctx, |ui| {
                    if rename {
                        ui.label(self.t("dialog.renamePrompt"));
                        ui.text_edit_singleline(&mut self.rename);
                    } else {
                        let key = if matches!(command, Command::Power) {
                            "dialog.powerPrompt"
                        } else {
                            "dialog.pairPrompt"
                        };
                        ui.label(self.t(key).replace("{0}", &self.state.name));
                    }
                    ui.add_space(12.);
                    ui.horizontal(|ui| {
                        if ui.button(self.t("dialog.cancel")).clicked() {
                            self.confirm = None;
                        }
                        if ui.button(self.t("dialog.ok")).clicked() {
                            if rename {
                                self.send(Command::Rename(self.rename.clone()));
                            } else {
                                self.send(command);
                            }
                            self.confirm = None;
                        }
                    });
                });
        }
    }
}
impl eframe::App for Dashboard {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            if std::env::args().any(|arg| arg == "--maximized") {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
            }
        }
        self.events(ctx);
        self.quitting |= self.tray.as_ref().is_some_and(|tray| tray.quitting());
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.quitting || self.tray.is_none() {
                self.quitting = true;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            }
        }
        self.render(ctx);
        if self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
