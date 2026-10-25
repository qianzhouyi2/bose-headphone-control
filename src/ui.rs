use crate::model::Event;
use crate::{localization::Localization, model::*, worker::Worker};
use eframe::egui::{epaint::PathShape, *};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
const INK: Color32 = Color32::from_rgb(17, 17, 17);
const MUTED: Color32 = Color32::from_rgb(96, 96, 96);
const ACCENT: Color32 = INK;
const PALE: Color32 = Color32::from_rgb(229, 229, 229);
const BG: Color32 = Color32::from_rgb(243, 243, 243);
const LINE: Color32 = Color32::from_rgb(221, 221, 221);
fn mode_columns(width: f32, count: usize) -> usize {
    (((width - 48.) / 112.).floor() as usize)
        .max(1)
        .min(count.max(1))
}
struct HomeLayout {
    split: bool,
    product: f32,
    cols: usize,
    rows: usize,
    hero: f32,
    noise: f32,
    eq: f32,
}
impl HomeLayout {
    fn new(width: f32, height: f32, count: usize, compact: bool) -> Self {
        let split = width >= 1000.;
        let product = if split { (width - 24.) * 0.32 } else { 0. };
        let control_width = if split { width - product - 24. } else { width };
        let cols = mode_columns(control_width, count);
        let rows = count.div_ceil(cols).max(1);
        let dense = compact && rows > 1;
        let hero = if dense {
            108.
        } else if compact {
            136.
        } else {
            170.
        };
        let noise = if dense {
            168.
        } else if compact {
            174.
        } else {
            190.
        } + (rows - 1) as f32 * 64.;
        let eq = if dense { 206. } else { 220. };
        let extra = (height - if split { 0. } else { hero + 14. } - noise - eq - 14.)
            .clamp(0., if split { 480. } else { 320. });
        Self {
            split,
            product,
            cols,
            rows,
            hero: hero + if split { 0. } else { extra * 0.45 },
            noise: noise + extra * if split { 0.35 } else { 0.2 },
            eq: eq + extra * if split { 0.65 } else { 0.35 },
        }
    }
}
struct SettingsLayout {
    wide: bool,
    compact: bool,
    cols: usize,
    wear: f32,
    calls: f32,
    favorites: f32,
    le_audio: f32,
    gap: f32,
}
impl SettingsLayout {
    fn new(width: f32, height: f32, count: usize) -> Self {
        let wide = width >= 700.;
        let column_width = if wide { (width - 20.) * 0.5 } else { width };
        let cols = (((column_width - 44.) / 145.).floor() as usize)
            .max(1)
            .min(count.max(1));
        let rows = count.div_ceil(cols).max(1);
        let favorites = 144. + rows.saturating_sub(1) as f32 * 44.;
        let relaxed_height = if wide {
            (224.0_f32 + 16. + 132.).max(favorites + 16. + 112.)
        } else {
            200. + 130. + favorites + 92. + 16. * 3.
        };
        let compact = height < relaxed_height;
        Self {
            wide,
            compact,
            cols,
            wear: if compact { 164. } else { 224. },
            calls: if compact { 100. } else { 132. },
            favorites: if compact {
                116. + rows.saturating_sub(1) as f32 * 34.
            } else {
                favorites
            },
            le_audio: if compact { 92. } else { 112. },
            gap: if compact { 10. } else { 16. },
        }
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
    ui.painter().rect_filled(r, 8., Color32::WHITE);
}
fn navigation(ui: &mut Ui, index: usize, r: Rect, label: &str, selected: bool) -> bool {
    let response = ui
        .interact(r, ui.id().with(("nav", index)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if selected {
        ui.painter().line_segment(
            [
                pos2(r.left() + 16., r.bottom() + 7.),
                pos2(r.right() - 16., r.bottom() + 7.),
            ],
            Stroke::new(2.0_f32, INK),
        );
    } else if response.hovered() {
        ui.painter().rect_filled(r, 4., PALE);
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(r, 4., Stroke::new(1.0_f32, ACCENT));
    }
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::new(14., FontFamily::Name("semibold".into())),
        if selected { INK } else { MUTED },
    );
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    remember_focus(ui, &response);
    response.clicked()
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
        if enabled {
            INK
        } else {
            Color32::from_rgb(160, 160, 160)
        }
    } else if response.hovered() && enabled {
        PALE
    } else {
        BG
    };
    ui.painter().rect_filled(r, 4., fill);
    if response.has_focus() {
        ui.painter()
            .rect_stroke(r.expand(2.), 5., Stroke::new(1.0_f32, MUTED));
    }
    let color = if selected {
        Color32::WHITE
    } else if !enabled {
        MUTED
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
            Color32::from_rgb(55, 55, 55)
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
    let radius = if r.height() < 70. { 19. } else { 23. };
    let c = pos2(r.center().x, r.top() + radius);
    let color = if selected { Color32::WHITE } else { INK };
    ui.painter().circle_filled(
        c,
        radius,
        if selected {
            if enabled {
                ACCENT
            } else {
                Color32::from_rgb(160, 160, 160)
            }
        } else {
            BG
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
    let color = if selected && enabled { INK } else { MUTED };
    let mut job = text::LayoutJob::simple(
        label.into(),
        FontId::proportional(12.),
        color,
        r.width() - 8.,
    );
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(c.x - galley.size().x * 0.5, r.top() + radius * 2. + 8.),
        galley,
        color,
    );
    if response.has_focus() {
        ui.painter()
            .circle_stroke(c, radius + 4., Stroke::new(1.0_f32, ACCENT));
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
            Color32::from_rgb(204, 204, 204)
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
    range: std::ops::RangeInclusive<i32>,
    enabled: bool,
    label: &str,
) -> (bool, bool) {
    let (min, max) = (*range.start(), *range.end());
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
    let vertical = r.height() > r.width();
    let start = if vertical {
        r.bottom() - 12.
    } else {
        r.left() + 12.
    };
    let end = if vertical {
        r.top() + 12.
    } else {
        r.right() - 12.
    };
    if enabled && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
    {
        if let Some(p) = response.interact_pointer_pos() {
            let position = if vertical { p.y } else { p.x };
            *value = (min as f32
                + ((position - start) / (end - start)).clamp(0., 1.) * (max - min) as f32)
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
    let point = |value| {
        if vertical {
            pos2(r.center().x, value)
        } else {
            pos2(value, r.center().y)
        }
    };
    let thumb = point(lerp(start..=end, t));
    ui.painter()
        .line_segment([point(start), point(end)], Stroke::new(3.0_f32, LINE));
    ui.painter().line_segment(
        [point(start), thumb],
        Stroke::new(3.0_f32, if enabled { ACCENT } else { MUTED }),
    );
    if vertical {
        let zero = point(lerp(start..=end, (-min) as f32 / (max - min).max(1) as f32));
        ui.painter().line_segment(
            [zero - vec2(6., 0.), zero + vec2(6., 0.)],
            Stroke::new(1.0_f32, LINE),
        );
    }
    ui.painter()
        .circle_filled(thumb, 8., if enabled { INK } else { MUTED });
    if response.has_focus() {
        ui.painter().circle_stroke(
            thumb,
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

    let mut style = Style {
        visuals: Visuals::light(),
        animation_time: 0.,
        ..Style::default()
    };
    style.visuals.panel_fill = BG;
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = PALE;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, INK);
    style.visuals.hyperlink_color = INK;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.window_rounding = Rounding::same(8.);
    style.visuals.widgets.active.bg_fill = PALE;
    style.visuals.widgets.active.weak_bg_fill = PALE;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, INK);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    style.spacing.item_spacing = vec2(12., 12.);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.));
    ctx.set_style(style);
}
#[derive(Default)]
enum FirmwareStatus {
    #[default]
    Idle,
    Checking,
    Available(String),
    NoUpdate,
    Failed,
}

type FirmwareIdentity = (u16, String, String);
type FirmwareResult = Result<crate::firmware::FirmwareCheck, ()>;

pub struct Dashboard {
    pub state: State,
    pub drafts: Drafts,
    pub language: Localization,
    pub page: usize,
    headphone_texture: TextureHandle,
    worker: Option<Worker>,
    connecting: bool,
    pub error: Option<(String, Instant)>,
    notice: Option<(String, Instant)>,
    pending_device: Option<(String, DeviceAction)>,
    pending_connectivity: Option<u64>,
    connectivity_revision: u64,
    devices_page: usize,
    confirm: Option<Command>,
    rename: String,
    quitting: bool,
    tray: Option<crate::platform::Tray>,
    first_frame: bool,
    firmware_identity: Option<FirmwareIdentity>,
    firmware_status: FirmwareStatus,
    firmware_result: Option<mpsc::Receiver<FirmwareResult>>,
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
            connecting: true,
            error: None,
            notice: None,
            pending_device: None,
            pending_connectivity: None,
            connectivity_revision: 0,
            devices_page: 0,
            confirm: None,
            rename: String::new(),
            quitting: false,
            tray,
            first_frame: true,
            firmware_identity: None,
            firmware_status: FirmwareStatus::Idle,
            firmware_result: None,
        }
    }
    fn t(&self, key: &str) -> String {
        self.language.text(key)
    }
    fn firmware_identity(&self) -> Option<FirmwareIdentity> {
        if !self.state.connected
            || self.state.product_guid.is_empty()
            || self.state.firmware.is_empty()
        {
            return None;
        }
        let product_id = self.state.product_id?;
        if !matches!(product_id, 0x4082 | 0x4066) {
            return None;
        }
        Some((
            product_id,
            self.state.product_guid.clone(),
            self.state.firmware.clone(),
        ))
    }
    fn firmware_events(&mut self) {
        let identity = self.firmware_identity();
        if identity != self.firmware_identity {
            self.firmware_identity = identity;
            self.firmware_result = None;
            self.firmware_status = FirmwareStatus::Idle;
        }
        if let Some(receiver) = &self.firmware_result {
            match receiver.try_recv() {
                Ok(Ok(result)) => {
                    self.firmware_status = if result.update_available {
                        FirmwareStatus::Available(result.version)
                    } else {
                        FirmwareStatus::NoUpdate
                    };
                    self.firmware_result = None;
                }
                Ok(Err(())) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.firmware_status = FirmwareStatus::Failed;
                    self.firmware_result = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    fn check_firmware(&mut self, ctx: &Context) {
        let Some((product_id, guid, version)) = self.firmware_identity() else {
            return;
        };
        if self.firmware_result.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        self.firmware_status = FirmwareStatus::Checking;
        self.firmware_result = Some(receiver);
        std::thread::spawn(move || {
            let result = crate::firmware::check(product_id, &guid, &version).map_err(|_| ());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
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
    fn begin_connection(&mut self) {
        self.connecting = true;
        self.state.connected = false;
        self.state.playback = None;
        self.drafts.disconnect();
        self.pending_device = None;
        self.pending_connectivity = None;
        self.error = None;
        self.notice = None;
    }
    fn reconnect(&mut self) {
        if !self.connecting
            && self
                .worker
                .as_ref()
                .is_some_and(|worker| worker.sender.send(Command::Reconnect, true))
        {
            self.begin_connection();
        }
    }
    fn set_connectivity(&mut self, setting: ConnectivitySetting, enabled: bool) {
        if !self.state.connected
            || self.worker.is_none()
            || self.pending_connectivity.is_some()
            || self.pending_device.is_some()
        {
            return;
        }
        self.connectivity_revision += 1;
        let revision = self.connectivity_revision;
        self.pending_connectivity = Some(revision);
        self.send(Command::Connectivity {
            setting,
            enabled,
            revision,
        });
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
            Event::Connecting => self.begin_connection(),
            Event::ConnectivityDone(revision) => {
                if self.pending_connectivity == Some(revision) {
                    self.pending_connectivity = None;
                }
            }
            Event::ResetAck(edits) => {
                for edit in edits {
                    self.drafts.complete(edit);
                }
            }
            Event::State(state) => {
                self.connecting = false;
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
                self.connecting = false;
                self.state.connected = false;
                self.state.playback = None;
                self.pending_device = None;
                self.pending_connectivity = None;
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
                let compact = all.height() < 760.;
                let margin = (all.width() * 0.035).clamp(24., 64.);
                let page_width = (all.width() - margin * 2.).min(1360.);
                let x = all.center().x - page_width * 0.5;
                let nav_width = if all.width() < 900. { 106. } else { 118. };
                let nav_x = x;
                ui.painter().line_segment(
                    [pos2(x, 66.), pos2(x + page_width, 66.)],
                    Stroke::new(1.0_f32, LINE),
                );
                for (i, key) in ["nav.home", "nav.devices", "nav.settings", "nav.about"]
                    .iter()
                    .enumerate()
                {
                    let r = rect(nav_x + i as f32 * nav_width, 19., nav_width - 4., 36.);
                    if navigation(ui, i, r, &self.t(key), self.page == i) {
                        self.page = i;
                    }
                }
                let top = if self.page == 0 { 82. } else { 134. };
                let content = rect(x, top, page_width, all.height() - top - 20.);
                if self.page != 0 {
                    let name = ["home", "devices", "settings", "about"][self.page];
                    txt(
                        ui,
                        rect(x + 2., 85., page_width - 150., 36.),
                        self.t(&format!("page.{name}")),
                        26.,
                        INK,
                        true,
                    );
                    if self.connecting {
                        txt(
                            ui,
                            rect(x + page_width - 122., 95., 122., 24.),
                            self.t("status.readingShort"),
                            13.,
                            MUTED,
                            false,
                        );
                    } else if !self.state.connected
                        && button(
                            ui,
                            "reconnect",
                            rect(x + page_width - 122., 85., 122., 34.),
                            &self.t("action.reconnect"),
                            false,
                            true,
                        )
                        .clicked()
                    {
                        self.reconnect();
                    }
                }
                let settings = SettingsLayout::new(
                    page_width,
                    content.height(),
                    self.state.modes.keys().filter(|id| **id < 8).count(),
                );
                let home = HomeLayout::new(
                    page_width,
                    content.height(),
                    self.state.modes.len(),
                    compact,
                );
                let old_clip = ui.clip_rect();
                ui.set_clip_rect(content.intersect(old_clip));
                let origin = content.min;
                match self.page {
                    0 => self.home(ui, origin, page_width, compact, &home),
                    1 => self.devices(ui, origin, page_width, content.height()),
                    2 => self.settings(ui, origin, page_width, &settings),
                    _ => self.about(ui, origin, page_width),
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
        let hero_h = if layout.split {
            layout.noise + layout.eq + 14.
        } else {
            layout.hero
        };
        let hero_w = if layout.split { layout.product } else { w };
        let hero = rect(p.x, p.y, hero_w, hero_h);
        ui.painter()
            .rect_filled(hero, 8., Color32::from_rgb(232, 232, 230));
        let art = (hero_h - 20.).min(260.);
        let name = if self.state.name.is_empty() {
            self.t("device.defaultName")
        } else {
            self.state.name.clone()
        };
        let text_x = p.x + 28.;
        let text_width = if layout.split {
            hero_w - 56.
        } else {
            w - art - 84.
        };
        let mut job = text::LayoutJob::simple(
            name,
            FontId::new(
                if layout.split {
                    24.
                } else if dense {
                    20.
                } else if compact {
                    24.
                } else {
                    28.
                },
                FontFamily::Name("semibold".into()),
            ),
            INK,
            text_width,
        );
        job.wrap.max_rows = if layout.split {
            3
        } else if dense {
            1
        } else {
            2
        };
        let name = ui.painter().layout_job(job);
        let retry = !self.state.connected && !self.connecting;
        let block_height = name.size().y + 42. + if retry { 38. } else { 0. };
        let hero_y = if layout.split {
            p.y + 28.
        } else {
            p.y + ((hero_h - block_height) * 0.5).max(4.)
        };
        let status_y = if layout.split {
            hero.bottom() - 58. - if retry { 38. } else { 0. }
        } else {
            hero_y + name.size().y + 12.
        };
        let art_box = if layout.split {
            let top = hero_y + name.size().y + 32.;
            rect(p.x + 24., top, hero_w - 48., status_y - top - 24.)
        } else {
            rect(p.x + w - art - 28., p.y + (hero_h - art) * 0.5, art, art)
        };
        headphones(ui, &self.headphone_texture, art_box);
        ui.painter().galley(pos2(text_x, hero_y), name, INK);
        let status = if self.state.connected {
            self.t("status.connected")
        } else if self.connecting {
            self.t("status.reading")
        } else {
            self.t("status.disconnected")
        };
        let status_text = ui
            .painter()
            .layout_no_wrap(status, FontId::proportional(12.), MUTED);
        let status_width = status_text.size().x + 32.;
        ui.painter().circle_filled(
            pos2(text_x + 13., status_y + 14.),
            3.,
            if self.state.connected { INK } else { MUTED },
        );
        ui.painter()
            .galley(pos2(text_x + 23., status_y + 6.), status_text, MUTED);
        if let Some(battery) = self.state.battery.filter(|_| self.state.connected) {
            let bx = text_x + status_width + 16.;
            let br = rect(bx, status_y + 8., 21., 12.);
            ui.painter()
                .rect_stroke(br, 3., Stroke::new(1.2_f32, MUTED));
            ui.painter()
                .rect_filled(rect(br.right() + 1., br.top() + 4., 2., 4.), 1., MUTED);
            ui.painter().rect_filled(
                rect(
                    br.left() + 3.,
                    br.top() + 3.,
                    15. * battery as f32 / 100.,
                    6.,
                ),
                1.,
                if battery <= 20 {
                    Color32::from_rgb(198, 136, 54)
                } else {
                    MUTED
                },
            );
            let response = ui.interact(
                rect(bx, status_y, 85., 28.),
                ui.id().with("battery"),
                Sense::hover(),
            );
            response.on_hover_text(self.t("battery.label"));
            ui.painter().text(
                pos2(bx + 31., status_y + 6.),
                Align2::LEFT_TOP,
                format!("{battery}%"),
                FontId::proportional(13.),
                INK,
            );
        }
        if retry {
            let r = rect(text_x, status_y + 36., 132., 30.);
            if primary(ui, "hero.connect", r, &self.t("action.retry")) {
                self.reconnect();
            }
        }
        let p = if layout.split {
            pos2(p.x + hero_w + 24., p.y)
        } else {
            pos2(p.x, p.y + hero_h + 14.)
        };
        let w = if layout.split { w - hero_w - 24. } else { w };
        let noise_h = layout.noise;
        let y = p.y;
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
        let modes_width = (w - 48.).min(cols as f32 * 164.);
        let modes_x = p.x + (w - modes_width) * 0.5;
        let tile = modes_width / cols as f32;
        for (i, id) in self.state.modes.clone().keys().enumerate() {
            let r = rect(
                modes_x + (i % cols) as f32 * tile,
                y + 47.
                    + (noise_h - 100. - if rows > 1 { rows as f32 * 64. } else { 72. }).max(0.)
                        * 0.5
                    + (i / cols) as f32 * 64.,
                tile,
                if rows > 1 { 64. } else { 72. },
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
        let sy = y + noise_h - 53.;
        let mut value = self.drafts.value(SliderKey::Cnc, self.state.cnc);
        let r = rect(p.x + 22., sy, w - 44., 27.);
        let (changed, release) = slider(
            ui,
            SliderKey::Cnc,
            r,
            &mut value,
            0..=10,
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
        let bands = self.state.eq.len().max(3);
        let band_width = (left - 36.) / bands as f32;
        for (i, b) in self.state.eq.clone().iter().enumerate() {
            let center = p.x + 18. + (i as f32 + 0.5) * band_width;
            let key = SliderKey::Eq(b.id);
            let mut value = self.drafts.value(key, b.value);
            let name = self.t(match b.id {
                0 => "term.bass",
                1 => "term.mid",
                _ => "term.treble",
            });
            ui.painter().text(
                pos2(center, y + eq_h - 27.),
                Align2::CENTER_TOP,
                &name,
                FontId::proportional(13.),
                INK,
            );
            ui.painter().text(
                pos2(center, y + 58.),
                Align2::CENTER_TOP,
                format!("{value:+} dB"),
                FontId::proportional(13.),
                ACCENT,
            );
            let r = rect(center - 20., y + 80., 40., eq_h - 118.);
            let (changed, release) = slider(
                ui,
                key,
                r,
                &mut value,
                b.min..=b.max,
                self.state.connected,
                &name,
            );
            if changed || release && self.drafts.values.contains_key(&key) {
                self.edit(key, value, release);
            }
        }
        if self.state.eq.is_empty() && self.connecting {
            for i in 0..3 {
                let center = p.x + 18. + (i as f32 + 0.5) * band_width;
                ui.painter()
                    .rect_filled(rect(center - 16., y + 62., 32., 8.), 4., LINE);
                ui.painter().line_segment(
                    [pos2(center, y + 92.), pos2(center, y + eq_h - 50.)],
                    Stroke::new(3.0_f32, LINE),
                );
            }
        } else if self.state.eq.is_empty() {
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
        let sh = eq_h * 0.59;
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
            rect(x + 24., y + 49., right - 48., 34.),
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
                rect(
                    x + 24. + i as f32 * (sw + 6.),
                    y + 82. + (sh - 130.).max(0.) * 0.5,
                    sw,
                    32.,
                ),
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
            rect(x + 24., wind_y + 43., right - 110., 32.),
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
    fn devices(&mut self, ui: &mut Ui, p: Pos2, w: f32, height: f32) {
        let side = (w - 16.) * 0.35;
        card(ui, rect(p.x, p.y, side, 150.));
        txt(
            ui,
            rect(p.x + 22., p.y + 20., side - 44., 28.),
            self.t("multi.title"),
            17.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 54., side - 44., 36.),
            self.t("multi.description"),
            12.,
            MUTED,
            false,
        );
        if toggle(
            ui,
            "multipoint",
            rect(p.x + side - 70., p.y + 94., 48., 40.),
            self.state.multipoint == Some(true),
            self.state.connected
                && self.state.multipoint.is_some()
                && self.pending_connectivity.is_none()
                && self.pending_device.is_none(),
            &self.t("multi.title"),
        ) {
            self.set_connectivity(
                ConnectivitySetting::Multipoint,
                self.state.multipoint != Some(true),
            );
        }
        let pair_y = p.y + 166.;
        card(ui, rect(p.x, pair_y, side, 182.));
        txt(
            ui,
            rect(p.x + 22., pair_y + 20., side - 44., 26.),
            self.t("pair.title"),
            17.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., pair_y + 54., side - 44., 56.),
            self.t("pair.description"),
            12.,
            MUTED,
            false,
        );
        if button(
            ui,
            "pair",
            rect(p.x + 22., pair_y + 126., side - 44., 36.),
            &self.t("action.pair"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.confirm = Some(Command::Pair);
        }
        let p = pos2(p.x + side + 16., p.y);
        let w = w - side - 16.;
        let capacity = ((height - 120.) / 78.).floor().max(1.) as usize;
        let pages = self.state.devices.len().div_ceil(capacity).max(1);
        self.devices_page = self.devices_page.min(pages - 1);
        let start = self.devices_page * capacity;
        let count = self.state.devices.len().saturating_sub(start).min(capacity);
        let panel_h = 72. + count.max(1) as f32 * 78. + if pages > 1 { 48. } else { 14. };
        card(ui, rect(p.x, p.y, w, panel_h));
        txt(
            ui,
            rect(p.x + 22., p.y + 23., w - 156., 28.),
            self.t("devices.title"),
            17.,
            INK,
            true,
        );
        if button(
            ui,
            "refresh",
            rect(p.x + w - 126., p.y + 18., 104., 32.),
            &self.t("action.refresh"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.send(Command::Refresh);
        }
        for (i, d) in self
            .state
            .devices
            .clone()
            .iter()
            .skip(start)
            .take(capacity)
            .enumerate()
        {
            let r = rect(p.x + 12., p.y + 66. + i as f32 * 78., w - 24., 72.);
            let source =
                self.state.connected && d.connected && self.state.active.as_ref() == Some(&d.mac);
            let active = source
                && matches!(
                    self.state.playback,
                    Some(Playback::Playing | Playback::Seeking)
                );
            if source {
                ui.painter().rect_filled(r, 4., BG);
            }
            let icon = pos2(r.left() + 28., r.center().y);
            ui.painter()
                .circle_filled(icon, 19., if active { PALE } else { BG });
            ui.painter().rect_stroke(
                rect(icon.x - 9., icon.y - 7., 18., 13.),
                3.,
                Stroke::new(1.5_f32, if active { ACCENT } else { MUTED }),
            );
            ui.painter().line_segment(
                [
                    pos2(icon.x - 5., icon.y + 9.),
                    pos2(icon.x + 5., icon.y + 9.),
                ],
                Stroke::new(1.5_f32, if active { ACCENT } else { MUTED }),
            );
            let mut name = text::LayoutJob::simple(
                d.name.clone(),
                FontId::new(14., FontFamily::Name("semibold".into())),
                INK,
                r.width() - 202.,
            );
            name.wrap.max_rows = 1;
            let name = ui.painter().layout_job(name);
            ui.painter()
                .galley(pos2(r.left() + 58., r.top() + 14.), name, INK);
            ui.interact(
                rect(r.left() + 58., r.top() + 10., r.width() - 202., 25.),
                ui.id().with(("device.name", &d.mac)),
                Sense::hover(),
            )
            .on_hover_text(&d.name);
            txt(
                ui,
                rect(r.left() + 58., r.top() + 41., r.width() - 202., 20.),
                if source {
                    self.t(if active {
                        "devices.playing"
                    } else {
                        "devices.active"
                    })
                } else if d.connected {
                    self.t("status.connected")
                } else {
                    self.t("status.disconnected")
                },
                12.,
                if source { ACCENT } else { MUTED },
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
                rect(r.right() - 136., r.top() + 20., 124., 32.),
                &action,
                false,
                self.state.connected
                    && self.pending_device.is_none()
                    && self.pending_connectivity.is_none()
                    && (!d.connected || source),
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
            if i + 1 < count {
                ui.painter().line_segment(
                    [
                        pos2(r.left() + 58., r.bottom() + 3.),
                        pos2(r.right() - 12., r.bottom() + 3.),
                    ],
                    Stroke::new(1.0_f32, LINE),
                );
            }
        }
        if self.state.devices.is_empty() {
            txt(
                ui,
                rect(p.x + 22., p.y + 88., w - 44., 44.),
                self.t(if self.connecting {
                    "status.readingShort"
                } else {
                    "devices.waiting"
                }),
                14.,
                MUTED,
                false,
            );
        }
        if pages > 1 {
            let y = p.y + panel_h - 44.;
            if button(
                ui,
                "devices.previous",
                rect(p.x + 22., y, 40., 30.),
                "‹",
                false,
                self.devices_page > 0,
            )
            .clicked()
            {
                self.devices_page -= 1;
            }
            ui.painter().text(
                pos2(p.x + w * 0.5, y + 15.),
                Align2::CENTER_CENTER,
                format!("{} / {pages}", self.devices_page + 1),
                FontId::proportional(12.),
                MUTED,
            );
            if button(
                ui,
                "devices.next",
                rect(p.x + w - 62., y, 40., 30.),
                "›",
                false,
                self.devices_page + 1 < pages,
            )
            .clicked()
            {
                self.devices_page += 1;
            }
        }
    }
    fn settings(&mut self, ui: &mut Ui, p: Pos2, w: f32, layout: &SettingsLayout) {
        let wide = layout.wide;
        let compact = layout.compact;
        let w = if wide { (w - 20.) * 0.5 } else { w };
        let wear_h = layout.wear;
        let calls_h = layout.calls;
        let gap = layout.gap;
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
                    50. + i as f32 * 56.
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
                rect(
                    p.x + 22.,
                    y + if compact { 24. } else { 29. },
                    w - 110.,
                    if compact { 22. } else { 26. },
                ),
                self.t(desc),
                12.,
                MUTED,
                false,
            );
            if toggle(
                ui,
                key,
                rect(p.x + w - 70., y, 48., if compact { 42. } else { 46. }),
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
                rect(
                    p.x + 22. + i as f32 * (sw + 8.),
                    y + calls_h - if compact { 48. } else { 61. },
                    sw,
                    if compact { 34. } else { 38. },
                ),
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
        let cols = layout.cols;
        let favorite_h = layout.favorites;
        card(ui, rect(p.x, y, w, favorite_h));
        txt(
            ui,
            rect(p.x + 22., y + if compact { 12. } else { 18. }, w - 44., 28.),
            self.t("favorites.title"),
            18.,
            INK,
            true,
        );
        txt(
            ui,
            rect(p.x + 22., y + if compact { 40. } else { 51. }, w - 44., 24.),
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
                y + (if compact { 65. } else { 87. })
                    + (i / cols) as f32 * if compact { 34. } else { 44. },
                sw,
                if compact { 30. } else { 32. },
            );
            let selected = self.state.favorites & (1 << id) != 0;
            let can_toggle = self.state.connected
                && (!selected || self.state.favorites.count_ones() > MIN_FAVORITE_MODES);
            if button(
                ui,
                ("favorite", id),
                r,
                &self.mode_name(*id),
                selected,
                can_toggle,
            )
            .clicked()
            {
                self.send(Command::Favorite(*id));
            }
        }
        let le_audio_y = y + favorite_h + gap;
        let le_audio_h = layout.le_audio;
        card(ui, rect(p.x, le_audio_y, w, le_audio_h));
        txt(
            ui,
            rect(
                p.x + 22.,
                le_audio_y + if compact { 10. } else { 16. },
                w - 110.,
                25.,
            ),
            self.t("leAudio.title"),
            15.,
            INK,
            true,
        );
        txt(
            ui,
            rect(
                p.x + 22.,
                le_audio_y + if compact { 36. } else { 47. },
                w - 110.,
                28.,
            ),
            self.t("leAudio.description"),
            12.,
            MUTED,
            false,
        );
        if toggle(
            ui,
            "le-audio",
            rect(
                p.x + w - 70.,
                le_audio_y + if compact { 15. } else { 24. },
                48.,
                if compact { 42. } else { 46. },
            ),
            self.state.le_audio == Some(true),
            self.state.connected
                && self.state.le_audio.is_some()
                && self.pending_connectivity.is_none()
                && self.pending_device.is_none(),
            &self.t("leAudio.title"),
        ) {
            self.set_connectivity(
                ConnectivitySetting::LeAudio,
                self.state.le_audio != Some(true),
            );
        }
    }
    fn about(&mut self, ui: &mut Ui, p: Pos2, width: f32) {
        let w = (width - 16.) * 0.55;
        card(ui, rect(p.x, p.y, w, 420.));
        let mut title = text::LayoutJob::simple(
            if self.state.name.is_empty() {
                self.t("device.defaultName")
            } else {
                self.state.name.clone()
            },
            FontId::new(20., FontFamily::Name("semibold".into())),
            INK,
            w - 44.,
        );
        title.wrap.max_rows = 2;
        let title = ui.painter().layout_job(title);
        ui.painter().galley(pos2(p.x + 22., p.y + 20.), title, INK);
        if button(
            ui,
            "rename",
            rect(p.x + 22., p.y + 79., 100., 32.),
            &self.t("action.rename"),
            false,
            self.state.connected,
        )
        .clicked()
        {
            self.rename = self.state.name.clone();
            self.confirm = Some(Command::Rename(String::new()));
        }
        txt(
            ui,
            rect(p.x + 22., p.y + 130., w - 44., 20.),
            self.t("about.firmware"),
            12.,
            MUTED,
            false,
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 151., w - 44., 25.),
            if self.state.firmware.is_empty() {
                "—"
            } else {
                &self.state.firmware
            },
            15.,
            INK,
            false,
        );
        let action_width = (w - 52.) * 0.5;
        if button(
            ui,
            "firmware.check",
            rect(p.x + 22., p.y + 187., action_width, 34.),
            &self.t(if self.firmware_result.is_some() {
                "firmware.checking"
            } else {
                "firmware.check"
            }),
            false,
            self.firmware_identity.is_some() && self.firmware_result.is_none(),
        )
        .clicked()
        {
            self.check_firmware(ui.ctx());
        }
        if button(
            ui,
            "firmware.usb",
            rect(p.x + 30. + action_width, p.y + 187., action_width, 34.),
            &self.t("firmware.usb"),
            false,
            true,
        )
        .on_hover_text(self.t("firmware.usbHint"))
        .clicked()
            && crate::firmware::open_official_updater().is_err()
        {
            self.error = Some((self.t("firmware.openFailed"), Instant::now()));
        }
        let status = match &self.firmware_status {
            FirmwareStatus::Idle | FirmwareStatus::Checking => None,
            FirmwareStatus::Available(version) => {
                Some((self.t("firmware.available").replace("{0}", version), ACCENT))
            }
            FirmwareStatus::NoUpdate => Some((self.t("firmware.none"), MUTED)),
            FirmwareStatus::Failed => {
                Some((self.t("firmware.failed"), Color32::from_rgb(180, 65, 65)))
            }
        };
        if let Some((status, color)) = status {
            txt(
                ui,
                rect(p.x + 22., p.y + 231., w - 44., 24.),
                status,
                11.,
                color,
                false,
            );
        }
        txt(
            ui,
            rect(p.x + 22., p.y + 264., w - 44., 20.),
            self.t("about.connection"),
            12.,
            MUTED,
            false,
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 285., w - 44., 25.),
            if self.state.transport.is_empty() {
                "—"
            } else {
                &self.state.transport
            },
            14.,
            INK,
            false,
        );
        ui.painter().line_segment(
            [pos2(p.x + 22., p.y + 327.), pos2(p.x + w - 22., p.y + 327.)],
            Stroke::new(1.0_f32, LINE),
        );
        txt(
            ui,
            rect(p.x + 22., p.y + 350., 110., 25.),
            self.t("language.title"),
            14.,
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
        let combo_width = w - 152.;
        ui.allocate_new_ui(
            UiBuilder::new().max_rect(rect(p.x + 130., p.y + 340., combo_width, 42.)),
            |ui| {
                ui.spacing_mut().button_padding = vec2(12., 9.);
                let visuals = ui.visuals_mut();
                for widget in [
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                ] {
                    widget.rounding = Rounding::same(4.);
                    widget.bg_stroke = Stroke::NONE;
                }
                visuals.widgets.inactive.weak_bg_fill = BG;
                visuals.widgets.hovered.weak_bg_fill = PALE;
                ComboBox::from_id_salt("language")
                    .selected_text(selected)
                    .width(combo_width - 24.)
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
            rect(p.x + 22., p.y + 394., w - 44., 18.),
            format!("{} {}", crate::APP_NAME, crate::APP_VERSION),
            11.,
            MUTED,
            false,
        );
        let p = pos2(p.x + w + 16., p.y);
        let w = width - w - 16.;
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
            let y = p.y + i as f32 * 144.;
            card(ui, rect(p.x, y, w, 132.));
            txt(
                ui,
                rect(p.x + 22., y + 18., w - 44., 26.),
                self.t(title),
                16.,
                INK,
                true,
            );
            txt(
                ui,
                rect(p.x + 22., y + 49., w - 44., 34.),
                self.t(description),
                12.,
                MUTED,
                false,
            );
            if button(
                ui,
                ("about", i),
                rect(p.x + 22., y + 89., 146., 30.),
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
        self.firmware_events();
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
