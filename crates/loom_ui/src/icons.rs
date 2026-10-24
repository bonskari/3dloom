//! Toolbar icons: glyphs of the Phosphor icon font (MIT, one weight embedded),
//! named here by what they stand for in the reference UI, and the one button
//! helper that draws them with their tooltip and chord.

use crate::shortcuts::tip;
use eframe::egui;
use egui_phosphor::regular as ph;
use loom_commands::AppState;

/// The icon font's name in the font definitions.
pub const FONT: &str = "phosphor";

/// Glyph size of an icon button (the reference toolbars use about 16 px icons).
pub const SIZE: f32 = 16.0;

/// Adds the icon font as the fallback after the text font (once per context):
/// text keeps its own glyphs, the icon code points come from the icon font.
pub fn install(ctx: &egui::Context) {
    let id = egui::Id::new("loom_icon_font");
    if ctx.data(|d| d.get_temp::<bool>(id)).is_some() {
        return;
    }
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        mono.push(FONT.into());
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| d.insert_temp(id, true));
}

// Main Toolbar.
pub const UNDO: &str = ph::ARROW_U_UP_LEFT;
pub const REDO: &str = ph::ARROW_U_UP_RIGHT;
pub const LINK: &str = ph::LINK;
pub const UNLINK: &str = ph::LINK_BREAK;
pub const SELECT_OBJECT: &str = ph::CURSOR;
pub const SELECT_BY_NAME: &str = ph::LIST_MAGNIFYING_GLASS;
pub const REGION_RECTANGULAR: &str = ph::SELECTION;
pub const REGION_CIRCULAR: &str = ph::CIRCLE_DASHED;
pub const REGION_FENCE: &str = ph::POLYGON;
pub const REGION_LASSO: &str = ph::LASSO;
pub const REGION_PAINT: &str = ph::PAINT_BRUSH;
pub const WINDOW: &str = ph::SELECTION_ALL;
pub const CROSSING: &str = ph::INTERSECT_SQUARE;
pub const MOVE: &str = ph::ARROWS_OUT_CARDINAL;
pub const ROTATE: &str = ph::ARROW_CLOCKWISE;
pub const SCALE: &str = ph::ARROWS_OUT;
pub const PIVOT_CENTER: &str = ph::CROSSHAIR;
pub const SELECTION_CENTER: &str = ph::BOUNDING_BOX;
pub const COORDINATE_CENTER: &str = ph::GPS_FIX;
pub const MANIPULATE: &str = ph::HAND_POINTING;
pub const KEYBOARD_OVERRIDE: &str = ph::KEYBOARD;
pub const SNAP: &str = ph::MAGNET;
pub const ANGLE_SNAP: &str = ph::ANGLE;
pub const PERCENT_SNAP: &str = ph::PERCENT;
pub const SPINNER_SNAP: &str = ph::CARET_UP_DOWN;
pub const NAMED_SETS: &str = ph::BRACKETS_CURLY;
pub const MIRROR: &str = ph::FLIP_HORIZONTAL;
pub const ALIGN: &str = ph::ALIGN_BOTTOM;
pub const QUICK_ALIGN: &str = ph::LIGHTNING;
pub const NORMAL_ALIGN: &str = ph::ARROW_LINE_UP;
pub const ALIGN_CAMERA: &str = ph::VIDEO_CAMERA;
pub const ALIGN_TO_VIEW: &str = ph::EYE;
pub const LAYERS: &str = ph::STACK;
pub const GRAPHITE: &str = ph::ROWS;
pub const CURVE_EDITOR: &str = ph::CHART_LINE;
pub const MATERIAL_EDITOR: &str = ph::CIRCLES_FOUR;

// Command Panel tabs.
pub const TAB_SCENE: &str = ph::TREE_VIEW;
pub const TAB_CREATE: &str = ph::SPARKLE;
pub const TAB_MODIFY: &str = ph::RAINBOW;
pub const TAB_HIERARCHY: &str = ph::TREE_STRUCTURE;
pub const TAB_MOTION: &str = ph::PERSON_SIMPLE_RUN;
pub const TAB_DISPLAY: &str = ph::MONITOR;
pub const TAB_UTILITIES: &str = ph::HAMMER;

// Create panel categories.
pub const GEOMETRY: &str = ph::SPHERE;
pub const SHAPES: &str = ph::BEZIER_CURVE;
pub const LIGHTS: &str = ph::FLASHLIGHT;
pub const CAMERAS: &str = ph::VIDEO_CAMERA;
pub const HELPERS: &str = ph::RULER;
pub const SPACE_WARPS: &str = ph::WAVES;
pub const SYSTEMS: &str = ph::GEAR_SIX;

// Scene tab object kinds.
pub const KIND_GEOMETRY: &str = ph::CUBE;
pub const KIND_SHAPE: &str = ph::BEZIER_CURVE;
pub const KIND_CAMERA: &str = ph::VIDEO_CAMERA;
pub const KIND_HELPER: &str = ph::CUBE_TRANSPARENT;
pub const KIND_BONE: &str = ph::BONE;
pub const KIND_GROUP: &str = ph::BRACKETS_SQUARE;
pub const KIND_COMPOUND: &str = ph::INTERSECT;

// Modifier stack.
pub const PIN_STACK: &str = ph::PUSH_PIN;
pub const SHOW_END_RESULT: &str = ph::ARROW_LINE_DOWN;
pub const MAKE_UNIQUE: &str = ph::COPY_SIMPLE;
pub const REMOVE_MODIFIER: &str = ph::TRASH;
pub const CONFIGURE_SETS: &str = ph::GEAR;
pub const MODIFIER_ON: &str = ph::LIGHTBULB;
pub const EXPAND: &str = ph::PLUS_SQUARE;
pub const COLLAPSE: &str = ph::MINUS_SQUARE;
pub const SHOW_SET: &str = ph::CARET_DOWN;
pub const HIDE_SET: &str = ph::CARET_UP;
/// The small settings button next to an Edit Poly / Edit Spline operation.
pub const SETTINGS: &str = ph::APP_WINDOW;

// Status bar.
pub const LOCKED: &str = ph::LOCK;
pub const UNLOCKED: &str = ph::LOCK_OPEN;
pub const ABSOLUTE: &str = ph::MAP_PIN_SIMPLE;
pub const OFFSET: &str = ph::PLUS_MINUS;
pub const SET_KEYS: &str = ph::KEY;
pub const KEY_MODE: &str = ph::ARROWS_IN_LINE_HORIZONTAL;
pub const TIME_CONFIGURATION: &str = ph::CLOCK;
pub const GO_TO_START: &str = ph::SKIP_BACK;
pub const PREVIOUS_FRAME: &str = ph::CARET_LEFT;
pub const PREVIOUS_KEY: &str = ph::CARET_LINE_LEFT;
pub const PLAY: &str = ph::PLAY;
pub const STOP: &str = ph::PAUSE;
pub const NEXT_FRAME: &str = ph::CARET_RIGHT;
pub const NEXT_KEY: &str = ph::CARET_LINE_RIGHT;
pub const GO_TO_END: &str = ph::SKIP_FORWARD;
pub const ZOOM: &str = ph::MAGNIFYING_GLASS;
pub const ZOOM_ALL: &str = ph::MAGNIFYING_GLASS_PLUS;
pub const ZOOM_EXTENTS: &str = ph::FRAME_CORNERS;
pub const ZOOM_EXTENTS_ALL: &str = ph::SQUARES_FOUR;
pub const ZOOM_REGION: &str = ph::SCAN;
pub const FIELD_OF_VIEW: &str = ph::PERSPECTIVE;
pub const PAN: &str = ph::HAND;
pub const ORBIT: &str = ph::PLANET;
pub const MAXIMIZE: &str = ph::CORNERS_OUT;

// Track View toolbar.
pub const FILTERS: &str = ph::FUNNEL;
pub const EDIT_KEYS: &str = ph::KEY;
pub const EDIT_RANGES: &str = ph::LINE_SEGMENT;
pub const MOVE_KEYS: &str = ph::ARROWS_OUT_CARDINAL;
pub const MOVE_KEYS_HORIZONTAL: &str = ph::ARROWS_HORIZONTAL;
pub const MOVE_KEYS_VERTICAL: &str = ph::ARROWS_VERTICAL;
pub const SLIDE_KEYS: &str = ph::ARROW_FAT_LINES_RIGHT;
pub const SCALE_KEYS: &str = ph::ARROWS_OUT_LINE_HORIZONTAL;
pub const SCALE_VALUES: &str = ph::ARROWS_OUT_LINE_VERTICAL;
pub const ADD_KEYS: &str = ph::PLUS_CIRCLE;
pub const DRAW_CURVES: &str = ph::PENCIL_SIMPLE;
pub const REGION_TOOL: &str = ph::SELECTION;
pub const REDUCE_KEYS: &str = ph::TREND_DOWN;
pub const TANGENT_AUTO: &str = ph::MAGIC_WAND;
pub const TANGENT_SPLINE: &str = ph::BEZIER_CURVE;
pub const TANGENT_FAST: &str = ph::LIGHTNING;
pub const TANGENT_SLOW: &str = ph::HOURGLASS_SIMPLE;
pub const TANGENT_STEP: &str = ph::WAVE_SQUARE;
pub const TANGENT_LINEAR: &str = ph::WAVE_TRIANGLE;
pub const TANGENT_SMOOTH: &str = ph::WAVE_SINE;
pub const BREAK_TANGENTS: &str = ph::LINK_BREAK;
pub const UNIFY_TANGENTS: &str = ph::LINK;
pub const SELECT_TIME: &str = ph::BRACKETS_SQUARE;
pub const DELETE_TIME: &str = ph::TRASH;
pub const REVERSE_TIME: &str = ph::ARROWS_LEFT_RIGHT;
pub const SCALE_TIME: &str = ph::ARROWS_OUT_SIMPLE;
pub const INSERT_TIME: &str = ph::COLUMNS_PLUS_RIGHT;
pub const CUT_TIME: &str = ph::SCISSORS;
pub const COPY_TIME: &str = ph::COPY;
pub const PASTE_TIME: &str = ph::CLIPBOARD;
pub const SNAP_FRAMES: &str = ph::MAGNET;
pub const LOCK_SELECTION: &str = ph::LOCK;
pub const OUT_OF_RANGE: &str = ph::REPEAT;
pub const LOCK_TANGENTS: &str = ph::LOCK_KEY;
pub const SHOW_TANGENTS: &str = ph::LINE_SEGMENTS;
pub const MODIFY_SUBTREE: &str = ph::TREE_STRUCTURE;
pub const TV_PAN: &str = ph::HAND;
pub const HORIZONTAL_EXTENTS: &str = ph::FRAME_CORNERS;
pub const HORIZONTAL_EXTENTS_KEYS: &str = ph::SQUARE_SPLIT_HORIZONTAL;
pub const VALUE_EXTENTS: &str = ph::SQUARE_SPLIT_VERTICAL;
pub const TV_ZOOM: &str = ph::MAGNIFYING_GLASS;
pub const TV_ZOOM_REGION: &str = ph::SCAN;

// Edit UVWs toolbars.
pub const UV_FREEFORM: &str = ph::BOUNDING_BOX;
pub const UV_SCALE_HORIZONTAL: &str = ph::ARROWS_OUT_LINE_HORIZONTAL;
pub const UV_SCALE_VERTICAL: &str = ph::ARROWS_OUT_LINE_VERTICAL;
pub const HIDE: &str = ph::EYE_SLASH;
pub const UNHIDE_ALL: &str = ph::EYE;
pub const FREEZE: &str = ph::SNOWFLAKE;
pub const UNFREEZE_ALL: &str = ph::SUN;
pub const FILTER_SELECTED: &str = ph::FUNNEL_SIMPLE;
pub const ZOOM_SELECTED: &str = ph::SELECTION_PLUS;
pub const LEVEL_VERTEX: &str = ph::DOTS_NINE;
pub const LEVEL_EDGE: &str = ph::LINE_SEGMENTS;
pub const LEVEL_FACE: &str = ph::SQUARE;
pub const EXPAND_SELECTION: &str = ph::PLUS;
pub const CONTRACT_SELECTION: &str = ph::MINUS;

// Small dialog buttons.
pub const CLEAR: &str = ph::X;
pub const HIGHLIGHT: &str = ph::CARET_RIGHT;

/// The reference tells flyout variants apart by colour: the plain tool is grey,
/// the Selected variant white, the Sub-Object variant yellow.
pub const SELECTED_VARIANT: egui::Color32 = egui::Color32::WHITE;
pub const SUB_OBJECT_VARIANT: egui::Color32 = egui::Color32::from_rgb(240, 200, 60);

/// An icon button with the tooltip of the command it runs, chord included.
#[must_use]
pub struct Icon<'a> {
    glyph: &'a str,
    action: &'a str,
    text: Option<&'a str>,
    hint: Option<&'a str>,
    on: bool,
    enabled: bool,
    color: Option<egui::Color32>,
}

impl<'a> Icon<'a> {
    /// `glyph` for the command named `action`; the tooltip is the action and its chord.
    pub fn new(glyph: &'a str, action: &'a str) -> Self {
        Icon { glyph, action, text: None, hint: None, on: false, enabled: true, color: None }
    }
    /// Tooltip text in place of the action name (the chord of `action` still follows it).
    pub fn tip(mut self, text: &'a str) -> Self {
        self.text = Some(text);
        self
    }
    /// A note after the tooltip, e.g. "right-click: Grid and Snap Settings".
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }
    /// Drawn pressed (a toggle that is on, the active tool).
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// The hover text: "Select and Move (W)", with the hint after a semicolon.
    pub fn hover_text(&self, state: &AppState) -> String {
        let t = tip(state, self.text.unwrap_or(self.action), self.action);
        match self.hint {
            Some(h) => format!("{t}; {h}"),
            None => t,
        }
    }

    pub fn show(self, ui: &mut egui::Ui, state: &AppState) -> egui::Response {
        let mut text = egui::RichText::new(self.glyph).size(SIZE);
        if let Some(c) = self.color {
            text = text.color(c);
        }
        let hover = self.hover_text(state);
        ui.add_enabled(self.enabled, egui::Button::new(text).selected(self.on)).on_hover_text(&hover).on_disabled_hover_text(hover)
    }
}

/// A menu or list row: the icon, then its name.
pub fn labeled(glyph: &str, label: &str) -> String {
    format!("{glyph}  {label}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_font_has_every_glyph_and_text_keeps_its_own_font() {
        let ctx = egui::Context::default();
        let mut has = Vec::new();
        for _ in 0..2 {
            let _ = ctx.run(Default::default(), |ctx| {
                install(ctx);
                let font = egui::FontId::proportional(SIZE);
                has = [UNDO, MOVE, SNAP, TAB_MODIFY, GEOMETRY, KIND_BONE, PIN_STACK, SETTINGS, LOCKED, ORBIT, CLEAR].iter().map(|g| ctx.fonts(|f| f.has_glyphs(&font, g))).collect();
            });
        }
        assert!(has.iter().all(|h| *h), "every icon resolves to a glyph: {has:?}");
        ctx.fonts(|f| {
            assert!(f.definitions().font_data.contains_key(FONT), "the icon font is loaded");
            // The icon font only follows the text font: letters stay Ubuntu's.
            assert_eq!(f.definitions().families[&egui::FontFamily::Proportional][0], "Ubuntu-Light");
        });
    }

    #[test]
    fn hover_text_carries_the_chord_and_the_hint() {
        let s = AppState::new();
        assert_eq!(Icon::new(MOVE, "Select and Move").hover_text(&s), "Select and Move (W)");
        assert_eq!(Icon::new(ANGLE_SNAP, "Angle Snap Toggle").hint("right-click: Grid and Snap Settings").hover_text(&s), "Angle Snap Toggle (A); right-click: Grid and Snap Settings");
        assert_eq!(Icon::new(SELECT_BY_NAME, "Select By Name").tip("Select From Scene").hover_text(&s), "Select From Scene (H)");
    }
}
