//! Graphite — the unified design system, for egui / eframe.
//!
//! Copy this file into the app as `src/graphite.rs`, add `mod graphite;`,
//! and call [`install`] once at startup (in the eframe creation closure).
//! Screen code then uses the palette, constants and helpers from here and
//! never hard-codes a colour, size, radius or spacing value.
//!
//! Mirrors `tokens.json` in the unified-ui skill. Written against egui 0.37.
//! If the project pins a different egui version and something here doesn't
//! compile, adapt the API calls but keep every value.

#![allow(dead_code)]

use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    Response, RichText, Shadow, Stroke, Style, TextStyle, Theme, Ui, Vec2, WidgetText,
};

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// Colour roles. Use these by role; never write a hex value in screen code.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub canvas: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub border_subtle: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_fill: Color32,
    pub accent_fill_hover: Color32,
    pub accent_fill_pressed: Color32,
    pub accent_subtle: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub success_subtle: Color32,
    pub warning: Color32,
    pub warning_subtle: Color32,
    pub danger: Color32,
    pub danger_fill: Color32,
    pub danger_subtle: Color32,
    pub on_danger: Color32,
    pub scrim: Color32,
    pub shadow: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Palette {
    pub const DARK: Self = Self {
        canvas: hex(0x0E0F11),
        surface: hex(0x17181B),
        raised: hex(0x1D1F23),
        hover: hex(0x23252A),
        pressed: hex(0x2A2D33),
        border_subtle: hex(0x26282D),
        border: hex(0x33363C),
        border_strong: hex(0x474B53),
        text: hex(0xE6E7EA),
        text_secondary: hex(0xA0A3AB),
        text_muted: hex(0x8B8E96),
        text_disabled: hex(0x6C6F77),
        accent: hex(0x7B89FF),
        accent_fill: hex(0x4F5EF5),
        accent_fill_hover: hex(0x5B6CFF),
        accent_fill_pressed: hex(0x4452DB),
        accent_subtle: hex(0x1B1F3D),
        on_accent: hex(0xFFFFFF),
        success: hex(0x3FB97A),
        success_subtle: hex(0x12281D),
        warning: hex(0xE5A63B),
        warning_subtle: hex(0x2C2210),
        danger: hex(0xF0566A),
        danger_fill: hex(0xD0364B),
        danger_subtle: hex(0x30141A),
        on_danger: hex(0xFFFFFF),
        scrim: Color32::from_black_alpha(153),
        shadow: Color32::from_black_alpha(102),
    };

    pub const LIGHT: Self = Self {
        canvas: hex(0xFAFAFB),
        surface: hex(0xFFFFFF),
        raised: hex(0xFFFFFF),
        hover: hex(0xF0F1F3),
        pressed: hex(0xE6E7EA),
        border_subtle: hex(0xE6E7EA),
        border: hex(0xD5D7DC),
        border_strong: hex(0xB4B7BE),
        text: hex(0x17181B),
        text_secondary: hex(0x50535B),
        text_muted: hex(0x6C6F77),
        text_disabled: hex(0x878B93),
        accent: hex(0x4352E0),
        accent_fill: hex(0x4F5EF5),
        accent_fill_hover: hex(0x4452DB),
        accent_fill_pressed: hex(0x3A46C2),
        accent_subtle: hex(0xECEEFF),
        on_accent: hex(0xFFFFFF),
        success: hex(0x17753F),
        success_subtle: hex(0xE6F5EC),
        warning: hex(0x946200),
        warning_subtle: hex(0xFBF1DC),
        danger: hex(0xBC283E),
        danger_fill: hex(0xD0364B),
        danger_subtle: hex(0xFCE9EC),
        on_danger: hex(0xFFFFFF),
        scrim: Color32::from_black_alpha(102),
        shadow: Color32::from_black_alpha(41),
    };

    pub const fn of(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::DARK,
            Theme::Light => Self::LIGHT,
        }
    }
}

/// The palette for whichever theme this `Ui` is currently drawn in.
pub fn palette(ui: &Ui) -> Palette {
    if ui.visuals().dark_mode {
        Palette::DARK
    } else {
        Palette::LIGHT
    }
}

// ---------------------------------------------------------------------------
// Spacing, shape, size (compact density)
// ---------------------------------------------------------------------------

/// The spacing scale. Nothing in between these values.
pub mod space {
    pub const S1: f32 = 2.0;
    pub const S2: f32 = 4.0;
    pub const S3: f32 = 8.0;
    pub const S4: f32 = 12.0;
    pub const S5: f32 = 16.0;
    pub const S6: f32 = 24.0;
    pub const S7: f32 = 32.0;
    pub const S8: f32 = 48.0;
}

pub mod radius {
    use egui::CornerRadius;
    /// Checkboxes, badges, tags.
    pub const SM: CornerRadius = CornerRadius::same(4);
    /// Controls and rows — the default.
    pub const MD: CornerRadius = CornerRadius::same(6);
    /// Cards, panels, menus.
    pub const LG: CornerRadius = CornerRadius::same(8);
    /// Dialogs.
    pub const XL: CornerRadius = CornerRadius::same(12);
}

pub mod size {
    pub const CONTROL_SM: f32 = 24.0;
    pub const CONTROL: f32 = 28.0;
    pub const CONTROL_LG: f32 = 32.0;
    pub const CONTROL_PAD_X: f32 = 10.0;
    pub const ROW: f32 = 32.0;
    pub const ICON: f32 = 16.0;
    pub const SIDEBAR: f32 = 220.0;
    pub const INSPECTOR: f32 = 280.0;
    pub const TOOLBAR: f32 = 40.0;
    pub const PAGE_PAD: f32 = 16.0;
    pub const CONTENT_MAX: f32 = 960.0;
    pub const FORM_MAX: f32 = 480.0;
    pub const DIALOG: f32 = 420.0;
    pub const BORDER: f32 = 1.0;
}

pub mod motion {
    pub const FAST: f32 = 0.12;
    pub const BASE: f32 = 0.18;
    pub const SLOW: f32 = 0.26;
}

// ---------------------------------------------------------------------------
// Typography
// ---------------------------------------------------------------------------

const FAMILY_MEDIUM: &str = "inter-medium";
const FAMILY_SEMIBOLD: &str = "inter-semibold";
const FAMILY_ICONS: &str = "lucide";

/// Font files to bundle. egui has no font-weight axis, so each weight is a
/// separate static font file.
///
/// ```ignore
/// graphite::Fonts {
///     regular: include_bytes!("../assets/fonts/Inter-Regular.ttf"),
///     medium: include_bytes!("../assets/fonts/Inter-Medium.ttf"),
///     semibold: include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
///     mono: include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
///     icons: Some(lucide_icons::LUCIDE_FONT_BYTES),
/// }
/// ```
pub struct Fonts {
    pub regular: &'static [u8],
    pub medium: &'static [u8],
    pub semibold: &'static [u8],
    pub mono: &'static [u8],
    pub icons: Option<&'static [u8]>,
}

/// Type roles from the Graphite scale (compact density).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Display,
    Title,
    Heading,
    Body,
    Label,
    Small,
    Caption,
    Mono,
}

impl Type {
    pub fn font(self) -> FontId {
        let medium = || FontFamily::Name(FAMILY_MEDIUM.into());
        let semibold = || FontFamily::Name(FAMILY_SEMIBOLD.into());
        match self {
            Self::Display => FontId::new(24.0, semibold()),
            Self::Title => FontId::new(18.0, semibold()),
            Self::Heading => FontId::new(15.0, semibold()),
            Self::Body => FontId::new(13.0, FontFamily::Proportional),
            Self::Label => FontId::new(13.0, medium()),
            Self::Small => FontId::new(12.0, FontFamily::Proportional),
            Self::Caption => FontId::new(11.0, medium()),
            Self::Mono => FontId::new(12.0, FontFamily::Monospace),
        }
    }
}

/// Text in a Graphite type role: `ui.label(graphite::text("Projects", Type::Heading))`.
pub fn text(s: impl Into<String>, role: Type) -> RichText {
    RichText::new(s).font(role.font())
}

/// A Lucide icon as text. Pass `lucide_icons::Icon::Folder.unicode()` (or any
/// `char` from the Lucide font). Colour defaults to the surrounding text colour;
/// use `.color(palette(ui).text_muted)` for icons at rest.
pub fn icon(glyph: char) -> RichText {
    RichText::new(glyph.to_string()).font(FontId::new(
        size::ICON,
        FontFamily::Name(FAMILY_ICONS.into()),
    ))
}

fn font_definitions(fonts: &Fonts) -> FontDefinitions {
    // Start from egui's defaults so they remain as fallbacks for glyphs
    // Inter lacks (emoji, symbols, CJK if the default set has them).
    let mut defs = FontDefinitions::default();
    let fallback_sans = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut add = |name: &str, bytes: &'static [u8]| {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add("Inter-Regular", fonts.regular);
    add("Inter-Medium", fonts.medium);
    add("Inter-SemiBold", fonts.semibold);
    add("JetBrainsMono-Regular", fonts.mono);
    if let Some(icons) = fonts.icons {
        add("Lucide", icons);
    }

    let with_fallback = |first: &str| {
        let mut v = vec![first.to_owned()];
        v.extend(fallback_sans.iter().cloned());
        v
    };

    defs.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter-Regular".to_owned());
    defs.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "JetBrainsMono-Regular".to_owned());
    defs.families.insert(
        FontFamily::Name(FAMILY_MEDIUM.into()),
        with_fallback("Inter-Medium"),
    );
    defs.families.insert(
        FontFamily::Name(FAMILY_SEMIBOLD.into()),
        with_fallback("Inter-SemiBold"),
    );
    // Always register the icon family so `icon()` never panics on a missing
    // family; without the Lucide font it just falls back to the text font.
    let icon_family = if fonts.icons.is_some() {
        with_fallback("Lucide")
    } else {
        fallback_sans.clone()
    };
    defs.families
        .insert(FontFamily::Name(FAMILY_ICONS.into()), icon_family);

    defs
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

/// The complete egui `Style` for one theme.
pub fn style(theme: Theme) -> Style {
    let p = Palette::of(theme);
    let mut style = Style::default();

    // --- text ---
    style.text_styles = [
        (TextStyle::Small, Type::Small.font()),
        (TextStyle::Body, Type::Body.font()),
        (TextStyle::Button, Type::Label.font()),
        (TextStyle::Heading, Type::Title.font()),
        (TextStyle::Monospace, Type::Mono.font()),
    ]
    .into();

    // --- spacing ---
    let s = &mut style.spacing;
    s.item_spacing = Vec2::new(space::S3, space::S3);
    s.button_padding = Vec2::new(size::CONTROL_PAD_X, 5.0);
    s.interact_size = Vec2::new(40.0, size::CONTROL);
    s.window_margin = Margin::same(16);
    s.menu_margin = Margin::same(4);
    s.indent = space::S5;
    s.icon_width = 16.0;
    s.icon_width_inner = 8.0;
    s.icon_spacing = space::S3;
    s.combo_height = 320.0;
    s.tooltip_width = 320.0;
    s.scroll.bar_width = 8.0;

    // --- interaction / motion ---
    style.animation_time = motion::FAST;
    style.interaction.tooltip_delay = 0.5;

    // --- visuals ---
    let hairline = |c: Color32| Stroke::new(size::BORDER, c);
    let v = &mut style.visuals;
    *v = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };

    v.override_text_color = None;
    v.weak_text_color = Some(p.text_muted);
    v.hyperlink_color = p.accent;
    v.faint_bg_color = p.surface;
    v.extreme_bg_color = p.canvas;
    v.text_edit_bg_color = Some(p.canvas);
    v.code_bg_color = p.surface;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;

    v.panel_fill = p.canvas;
    v.window_fill = p.raised;
    v.window_stroke = hairline(p.border);
    v.window_corner_radius = radius::XL;
    v.menu_corner_radius = radius::LG;
    let floating = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: p.shadow,
    };
    v.window_shadow = floating;
    v.popup_shadow = floating;

    v.selection.bg_fill = p.accent_subtle;
    v.selection.stroke = hairline(p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);

    v.striped = false;
    v.slider_trailing_fill = true;
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;

    // Widget states. `bg_fill` is used by checkboxes, sliders and the like;
    // `weak_bg_fill` by buttons. `fg_stroke` colours text and icons.
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = hairline(p.border_subtle);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius::MD;

    // Resting control = Graphite "secondary": transparent with a border.
    w.inactive.bg_fill = p.hover;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = hairline(p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius::MD;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.pressed;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = hairline(p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius::MD;
    w.hovered.expansion = 0.0;

    // egui uses `active` for both "pressed" and "has keyboard focus", so the
    // accent outline here doubles as the focus indicator.
    w.active.bg_fill = p.pressed;
    w.active.weak_bg_fill = p.pressed;
    w.active.bg_stroke = Stroke::new(1.5, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius::MD;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.hover;
    w.open.weak_bg_fill = p.hover;
    w.open.bg_stroke = hairline(p.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius::MD;
    w.open.expansion = 0.0;

    style
}

/// Install Graphite: fonts plus dark and light styles. Call once at startup.
/// The app then follows the OS theme; call `ctx.set_theme(..)` with
/// `egui::ThemePreference::{System, Dark, Light}` from the settings screen.
pub fn install(ctx: &Context, fonts: &Fonts) {
    ctx.set_fonts(font_definitions(fonts));
    ctx.set_style_of(Theme::Dark, style(Theme::Dark));
    ctx.set_style_of(Theme::Light, style(Theme::Light));
    ctx.set_theme(egui::ThemePreference::System);
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------
//
// The default `ui.button(..)` is already the Graphite *secondary* button, and
// default text edits, checkboxes, combo boxes and menus pick up the style.
// The helpers below cover the variants a style alone can't express.

fn filled_button(
    ui: &mut Ui,
    label: impl Into<String>,
    fill: Color32,
    fill_hover: Color32,
    fill_pressed: Color32,
    on: Color32,
) -> Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        for (state, bg) in [
            (&mut w.inactive, fill),
            (&mut w.hovered, fill_hover),
            (&mut w.active, fill_pressed),
        ] {
            state.weak_bg_fill = bg;
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0, on);
        }
        ui.add(egui::Button::new(text(label, Type::Label).color(on)))
    })
    .inner
}

/// The one main action of a view. At most one per view.
pub fn primary_button(ui: &mut Ui, label: impl Into<String>) -> Response {
    let p = palette(ui);
    filled_button(
        ui,
        label,
        p.accent_fill,
        p.accent_fill_hover,
        p.accent_fill_pressed,
        p.on_accent,
    )
}

/// Confirms a destructive action. Only inside its confirmation dialog.
pub fn danger_button(ui: &mut Ui, label: impl Into<String>) -> Response {
    let p = palette(ui);
    filled_button(ui, label, p.danger_fill, p.danger, p.danger_fill, p.on_danger)
}

/// Borderless, low-emphasis button: toolbars, "Cancel", icon buttons.
pub fn ghost_button(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    let p = palette(ui);
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, p.text_secondary);
        w.hovered.bg_stroke = Stroke::NONE;
        ui.add(egui::Button::new(label))
    })
    .inner
}

/// Square ghost button holding one Lucide icon. Always give it a tooltip.
pub fn icon_button(ui: &mut Ui, glyph: char, tooltip: &str) -> Response {
    let p = palette(ui);
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.bg_stroke = Stroke::NONE;
        ui.spacing_mut().button_padding = Vec2::splat(0.0);
        ui.add(
            egui::Button::new(icon(glyph).color(p.text_muted))
                .min_size(Vec2::splat(size::CONTROL)),
        )
    })
    .inner
    .on_hover_text(tooltip)
}

/// Frame for a card or panel: surface fill, subtle hairline, radius 8, padding 12.
pub fn card_frame(ui: &Ui) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(size::BORDER, p.border_subtle))
        .corner_radius(radius::LG)
        .inner_margin(Margin::same(12))
}

/// A card: `graphite::card(ui, |ui| { ... })`.
pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(ui).show(ui, add_contents).inner
}

/// Frame for the sidebar panel: surface fill, 8 padding.
/// egui draws the panel separator line itself. `dark` is
/// `ui.visuals().dark_mode` (or `ctx.style().visuals.dark_mode`).
pub fn sidebar_frame(dark: bool) -> Frame {
    let p = if dark { Palette::DARK } else { Palette::LIGHT };
    Frame::new().fill(p.surface).inner_margin(Margin::same(8))
}

/// Frame for the top toolbar panel: canvas fill, 16×6 padding.
pub fn toolbar_frame(dark: bool) -> Frame {
    let p = if dark { Palette::DARK } else { Palette::LIGHT };
    Frame::new()
        .fill(p.canvas)
        .inner_margin(Margin::symmetric(16, 6))
}

/// Frame for the central content area: canvas fill, page padding.
pub fn page_frame(dark: bool) -> Frame {
    let p = if dark { Palette::DARK } else { Palette::LIGHT };
    Frame::new().fill(p.canvas).inner_margin(Margin::same(16))
}

/// Full-width navigation / list row, 32 high, radius 6. Selected rows get the
/// hover fill and an accent icon. Returns the click response.
pub fn nav_row(ui: &mut Ui, glyph: Option<char>, label: &str, selected: bool) -> Response {
    let p = palette(ui);
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, size::ROW), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let fill = if selected || response.is_pointer_button_down_on() {
            p.hover
        } else if response.hovered() {
            p.hover.gamma_multiply(0.6)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, radius::MD, fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                radius::MD,
                Stroke::new(1.5, p.accent),
                egui::StrokeKind::Inside,
            );
        }

        let active = selected || response.hovered();
        let text_color = if active { p.text } else { p.text_secondary };
        let mut x = rect.left() + space::S3;
        if let Some(glyph) = glyph {
            let icon_color = if selected { p.accent } else { p.text_muted };
            ui.painter().text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                glyph,
                FontId::new(size::ICON, FontFamily::Name(FAMILY_ICONS.into())),
                icon_color,
            );
            x += size::ICON + space::S3;
        }
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            Type::Label.font(),
            text_color,
        );
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Group header above a set of rows: caption type, muted.
pub fn group_header(ui: &mut Ui, label: &str) {
    let p = palette(ui);
    ui.add_space(space::S3);
    ui.horizontal(|ui| {
        ui.add_space(space::S3);
        ui.label(text(label, Type::Caption).color(p.text_muted));
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

/// Small badge / tag: radius 4, caption type, tinted by tone.
pub fn badge(ui: &mut Ui, label: &str, tone: Tone) -> Response {
    let p = palette(ui);
    let (bg, fg) = match tone {
        Tone::Neutral => (p.hover, p.text_secondary),
        Tone::Accent => (p.accent_subtle, p.accent),
        Tone::Success => (p.success_subtle, p.success),
        Tone::Warning => (p.warning_subtle, p.warning),
        Tone::Danger => (p.danger_subtle, p.danger),
    };
    Frame::new()
        .fill(bg)
        .corner_radius(radius::SM)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| ui.label(text(label, Type::Caption).color(fg)))
        .response
}

/// Field label above a control, with the standard 4 gap.
pub fn field_label(ui: &mut Ui, label: &str) {
    ui.label(text(label, Type::Label));
    ui.add_space(-space::S2);
}

/// Helper or error text beneath a field.
pub fn field_hint(ui: &mut Ui, hint: &str, is_error: bool) {
    let p = palette(ui);
    let color = if is_error { p.danger } else { p.text_muted };
    ui.add_space(-space::S2);
    ui.label(text(hint, Type::Small).color(color));
}

/// Centred empty / error state: icon, heading, one line, one action.
/// Returns true when the action button was clicked.
pub fn empty_state(
    ui: &mut Ui,
    glyph: Option<char>,
    heading: &str,
    body: &str,
    action: Option<&str>,
) -> bool {
    let p = palette(ui);
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.set_max_width(320.0);
        if let Some(glyph) = glyph {
            ui.label(
                RichText::new(glyph.to_string())
                    .font(FontId::new(24.0, FontFamily::Name(FAMILY_ICONS.into())))
                    .color(p.text_muted),
            );
            ui.add_space(space::S2);
        }
        ui.label(text(heading, Type::Heading));
        ui.label(text(body, Type::Body).color(p.text_secondary));
        if let Some(action) = action {
            ui.add_space(space::S3);
            clicked = ui.button(action).clicked();
        }
    });
    clicked
}

/// 1px divider in the subtle border colour.
pub fn divider(ui: &mut Ui) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), size::BORDER),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.border_subtle);
}

// ---------------------------------------------------------------------------
// Additions for fizz-control (candidates to fold back into the skill)
// ---------------------------------------------------------------------------

/// Segmented control for 2–4 mutually exclusive choices. Returns true when
/// the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = palette(ui);
    let dark = ui.visuals().dark_mode;
    let mut changed = false;
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(size::BORDER, p.border_subtle))
        .corner_radius(radius::MD)
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            // `horizontal` inherits right-to-left from a setting row; draw the
            // options in reverse there so they still read left to right.
            let rtl = ui.layout().prefer_right_to_left();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = space::S1;
                let ordered: Vec<_> = if rtl {
                    options.iter().rev().collect()
                } else {
                    options.iter().collect()
                };
                for (option, label) in ordered {
                    let selected = *value == *option;
                    let (fill, stroke, color) = match (selected, dark) {
                        (true, true) => (p.hover, Stroke::NONE, p.text),
                        (true, false) => {
                            (p.surface, Stroke::new(size::BORDER, p.border), p.text)
                        }
                        (false, _) => (Color32::TRANSPARENT, Stroke::NONE, p.text_secondary),
                    };
                    let button = egui::Button::new(text(*label, Type::Label).color(color))
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(radius::SM)
                        .min_size(Vec2::new(0.0, size::CONTROL_SM));
                    if ui.add(button).clicked() && !selected {
                        *value = *option;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// One setting row: label (and optional description) on the left, the
/// control on the right. Put [`divider`] between rows.
pub fn setting_row(
    ui: &mut Ui,
    label: &str,
    description: Option<&str>,
    add_control: impl FnOnce(&mut Ui),
) {
    let p = palette(ui);
    Frame::new()
        .inner_margin(Margin::symmetric(0, 6))
        .show(ui, |ui| {
            // `Sides` centres the label block against the control and sizes
            // the row to the taller of the two.
            egui::Sides::new().height(size::ROW).show(
                ui,
                |ui| match description {
                    // A lone label is centred by `Sides`; a `vertical` block
                    // would pin it to the top.
                    None => {
                        ui.label(text(label, Type::Label));
                    }
                    Some(description) => {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = space::S1;
                            ui.label(text(label, Type::Label));
                            ui.label(text(description, Type::Small).color(p.text_muted));
                        });
                    }
                },
                add_control,
            );
        });
}

/// A read-only value for the right side of a setting row.
pub fn value_text(ui: &Ui, s: impl Into<String>, mono: bool) -> RichText {
    let role = if mono { Type::Mono } else { Type::Body };
    text(s, role).color(palette(ui).text_secondary)
}

/// Inline banner for persistent, in-context status.
pub fn banner(ui: &mut Ui, tone: Tone, glyph: char, message: &str) {
    let p = palette(ui);
    let (bg, fg) = match tone {
        Tone::Neutral => (p.surface, p.text_muted),
        Tone::Accent => (p.accent_subtle, p.accent),
        Tone::Success => (p.success_subtle, p.success),
        Tone::Warning => (p.warning_subtle, p.warning),
        Tone::Danger => (p.danger_subtle, p.danger),
    };
    Frame::new()
        .fill(bg)
        .stroke(Stroke::new(size::BORDER, p.border_subtle))
        .corner_radius(radius::LG)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(icon(glyph).color(fg));
                ui.label(text(message, Type::Body).color(p.text));
            });
        });
}
