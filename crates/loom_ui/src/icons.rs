//! Toolbar icons: glyphs of the Phosphor icon font (MIT, one weight embedded),
//! named here by what they stand for in the reference UI, and the one button
//! helper that draws them with their tooltip and chord.

use crate::shortcuts::tip;
use eframe::egui;
use egui_phosphor::regular as ph;
use loom_commands::AppState;

/// The icon font's name in the font definitions.
pub const FONT: &str = "phosphor";

thread_local! {
    /// Icon button glyph size (Preferences > Large Toolbar Buttons), set each frame.
    static ICON_SIZE: std::cell::Cell<f32> = const { std::cell::Cell::new(SIZE) };
    /// Flyout Time in seconds (Preferences), set each frame.
    static FLYOUT_TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.3) };
}

/// The icon button glyph size this frame.
pub fn size() -> f32 {
    ICON_SIZE.with(|s| s.get())
}

/// Preferences > General > UI Display: Large Toolbar Buttons and Flyout Time.
pub fn set_prefs(large: bool, flyout_ms: f32) {
    ICON_SIZE.with(|s| s.set(if large { 24.0 } else { SIZE }));
    FLYOUT_TIME.with(|t| t.set(flyout_ms as f64 / 1000.0));
}

/// A flyout button's click-and-hold: true on the frame the press has lasted
/// the Flyout Time (the flyout opens then); `took` tells whether that press
/// opened it, so its release does not also run the button.
pub fn hold_opens(ui: &egui::Ui, r: &egui::Response) -> (bool, bool) {
    let start_id = r.id.with("hold_start");
    let took_id = r.id.with("hold_took");
    let now = ui.input(|i| i.time);
    let took: bool = ui.ctx().data(|d| d.get_temp(took_id)).unwrap_or(false);
    if r.is_pointer_button_down_on() && !r.dragged() {
        let start: f64 = ui.ctx().data_mut(|d| *d.get_temp_mut_or_insert_with(start_id, || now));
        if took {
            return (false, true);
        }
        if now - start >= FLYOUT_TIME.with(|t| t.get()) {
            ui.ctx().data_mut(|d| d.insert_temp(took_id, true));
            return (true, true);
        }
        ui.ctx().request_repaint();
        (false, false)
    } else if took && ui.input(|i| i.pointer.any_down()) {
        // Still held after moving off the button toward the strip.
        (false, true)
    } else {
        // The release frame still reports the hold, so its click is ignored.
        ui.ctx().data_mut(|d| {
            d.remove::<f64>(start_id);
            d.remove::<bool>(took_id);
        });
        (false, took)
    }
}

/// egui memory: the tooltip of the button under the pointer, which the prompt line shows.
pub const PROMPT_TIP: &str = "prompt_line_tooltip";

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
pub const SCALE_NON_UNIFORM: &str = ph::ARROWS_OUT_LINE_HORIZONTAL;
pub const SQUASH: &str = ph::ARROWS_IN_LINE_VERTICAL;
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
pub const RENDER_SETUP: &str = ph::SLIDERS_HORIZONTAL;
pub const RENDERED_FRAME_WINDOW: &str = ph::IMAGE_SQUARE;
/// Render Production; Render Iterative is the same glyph in the Sub-Object variant colour.
pub const RENDER: &str = ph::APERTURE;

// Quick Access toolbar.
pub const NEW_SCENE: &str = ph::FILE;
pub const OPEN_FILE: &str = ph::FOLDER_OPEN;
pub const SAVE_FILE: &str = ph::FLOPPY_DISK;

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
/// The Object Type rollout's Pin (Keep Creating).
pub const KEEP_CREATING: &str = ph::PUSH_PIN;

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
/// Small up / down arrows (show / hide a set, ring and loop steps, the ribbon).
pub const DOWN: &str = ph::CARET_DOWN;
pub const UP: &str = ph::CARET_UP;
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
pub const FIELD_OF_VIEW: &str = ph::BINOCULARS;
pub const PERSPECTIVE_MODE: &str = ph::PERSPECTIVE;
pub const DOLLY: &str = ph::ARROWS_DOWN_UP;
pub const ROLL: &str = ph::ARROWS_COUNTER_CLOCKWISE;
pub const TRUCK: &str = ph::HAND_GRABBING;
pub const PAN_CAMERA: &str = ph::COMPASS;
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

// Graphite Modeling Tools ribbon.
pub const LEVEL_BORDER: &str = ph::CIRCLE_DASHED;
pub const LEVEL_POLYGON: &str = ph::SQUARE;
pub const LEVEL_ELEMENT: &str = ph::CUBE;
pub const MODIFY_MODE: &str = ph::RAINBOW;
pub const PREVIOUS_MODIFIER: &str = ph::ARROW_DOWN;
pub const NEXT_MODIFIER: &str = ph::ARROW_UP;
pub const TOGGLE_COMMAND_PANEL: &str = ph::SIDEBAR_SIMPLE;
pub const PREVIEW_SELECTION: &str = ph::CURSOR_CLICK;
pub const IGNORE_BACKFACING: &str = ph::EYE_CLOSED;
pub const SOFT_SELECTION: &str = ph::DROP_HALF;
pub const COLLAPSE_STACK: &str = ph::STACK_SIMPLE;
pub const CONVERT_TO_POLY: &str = ph::POLYGON;
pub const APPLY_EDIT_POLY: &str = ph::PLUS_SQUARE;
pub const REPEAT_LAST: &str = ph::REPEAT_ONCE;
pub const QUICKSLICE: &str = ph::KNIFE;
pub const SWIFT_LOOP: &str = ph::LINE_VERTICAL;
pub const CUT: &str = ph::SCISSORS;
pub const USE_NURMS: &str = ph::DROP;
pub const CONSTRAIN_NONE: &str = ph::PROHIBIT;
pub const CONSTRAIN_EDGE: &str = ph::LINE_SEGMENT;
pub const CONSTRAIN_FACE: &str = ph::SQUARE_HALF;
pub const CONSTRAIN_NORMAL: &str = ph::ARROW_UP_RIGHT;
pub const GROW: &str = ph::PLUS;
pub const SHRINK: &str = ph::MINUS;
pub const LOOP: &str = ph::DOTS_THREE;
pub const RING: &str = ph::CIRCLE_NOTCH;
pub const LOOP_MODE: &str = ph::ARROWS_CLOCKWISE;
pub const RELAX: &str = ph::WAVE_SINE;
pub const CREATE: &str = ph::PLUS_CIRCLE;
pub const ATTACH: &str = ph::PAPERCLIP;
pub const COLLAPSE_SUB: &str = ph::ARROWS_IN;
pub const DETACH: &str = ph::ARROWS_SPLIT;
pub const SLICE_PLANE: &str = ph::SELECTION_SLASH;
pub const EXTRUDE: &str = ph::ARROW_FAT_UP;
pub const CHAMFER: &str = ph::CORNERS_IN;
pub const WELD: &str = ph::ARROWS_IN_SIMPLE;
pub const REMOVE: &str = ph::ERASER;
pub const BREAK: &str = ph::LINK_BREAK;
pub const TARGET_WELD: &str = ph::TARGET;
pub const BRIDGE: &str = ph::BRIDGE;
pub const SPLIT: &str = ph::ARROWS_MERGE;
pub const INSERT_VERTEX: &str = ph::DOT_OUTLINE;
pub const CONNECT: &str = ph::LINE_SEGMENTS;
pub const CAP: &str = ph::CIRCLE_HALF_TILT;
pub const BEVEL: &str = ph::ARROW_FAT_LINE_UP;
pub const INSET: &str = ph::FRAME_CORNERS;
pub const OUTLINE: &str = ph::SELECTION_FOREGROUND;
pub const HINGE: &str = ph::DOOR_OPEN;
pub const FLIP: &str = ph::FLIP_VERTICAL;
pub const EXTRUDE_ON_SPLINE: &str = ph::PATH;
pub const CREATE_SHAPE: &str = ph::BEZIER_CURVE;
pub const MSMOOTH: &str = ph::CIRCLES_THREE;
pub const TESSELLATE: &str = ph::GRID_FOUR;
pub const MAKE_PLANAR: &str = ph::ALIGN_CENTER_VERTICAL;
pub const GRID_ALIGN: &str = ph::GRID_NINE;
pub const AUTO_SMOOTH: &str = ph::MAGIC_WAND;
pub const CLEAR_SMOOTHING: &str = ph::ERASER;
pub const BRANCHES: &str = ph::TREE;
pub const PAINT_DEFORM: &str = ph::PAINT_BRUSH_BROAD;
pub const BY_HALF: &str = ph::CIRCLE_HALF;
pub const INVERT_AXIS: &str = ph::SWAP;
pub const MATERIAL_ID: &str = ph::PUZZLE_PIECE;
pub const POLYDRAW: &str = ph::PENCIL_SIMPLE;

// Scene Explorer.
pub const CHECKED: &str = ph::CHECK;
pub const DISPLAY_ALL: &str = ph::CHECK_SQUARE;
pub const DISPLAY_NONE: &str = ph::X_SQUARE;
pub const INVERT_DISPLAY: &str = ph::SWAP;
pub const LOCK_CELL_EDITING: &str = ph::LOCK_SIMPLE;
pub const SELECT_ALL: &str = ph::SELECTION_ALL;
pub const SELECT_NONE: &str = ph::SELECTION_SLASH;
pub const SELECT_INVERT: &str = ph::SELECTION_INVERSE;
pub const SYNC_SELECTION: &str = ph::ARROWS_CLOCKWISE;
pub const SORT_ASCENDING: &str = ph::SORT_ASCENDING;
pub const SORT_DESCENDING: &str = ph::SORT_DESCENDING;

// Schematic View.
pub const SCHEMATIC_VIEW: &str = ph::GRAPH;
pub const DISPLAY_FLOATER: &str = ph::FUNNEL;
pub const CONNECT_TOOL: &str = ph::PLUGS_CONNECTED;
pub const DELETE_OBJECTS: &str = ph::TRASH;
pub const HIERARCHY_MODE: &str = ph::TREE_STRUCTURE;
pub const REFERENCES_MODE: &str = ph::SHARE_NETWORK;
pub const ALWAYS_ARRANGE: &str = ph::GRID_FOUR;
pub const ARRANGE_SELECTED: &str = ph::TEXT_INDENT;
pub const FREE_ALL: &str = ph::SQUARES_FOUR;
pub const FREE_SELECTED: &str = ph::SELECTION_FOREGROUND;
pub const MOVE_CHILDREN: &str = ph::FLOW_ARROW;
pub const PAN_TO_SELECTED: &str = ph::CROSSHAIR;

// Small dialog buttons.
pub const CLEAR: &str = ph::X;
pub const HIGHLIGHT: &str = ph::CARET_RIGHT;
/// The arrow after a menu entry that opens a submenu.
pub const SUBMENU: &str = ph::CARET_RIGHT;

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
    label: Option<&'a str>,
}

impl<'a> Icon<'a> {
    /// `glyph` for the command named `action`; the tooltip is the action and its chord.
    pub fn new(glyph: &'a str, action: &'a str) -> Self {
        Icon { glyph, action, text: None, hint: None, on: false, enabled: true, color: None, label: None }
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
    /// Text drawn after the glyph (the ribbon's buttons carry both).
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
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
        let mut text = match self.label {
            Some(l) => egui::RichText::new(format!("{}  {l}", self.glyph)),
            None => egui::RichText::new(self.glyph).size(size()),
        };
        if let Some(c) = self.color {
            text = text.color(c);
        }
        let hover = self.hover_text(state);
        let r = ui.add_enabled(self.enabled, egui::Button::new(text).selected(self.on));
        if r.hovered() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PROMPT_TIP), hover.clone()));
        }
        // Screen readers (and the UI tests) know the button by its name, not its glyph.
        let name = self.text.unwrap_or(self.action);
        r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, self.enabled, self.on, name));
        r.on_hover_text(&hover).on_disabled_hover_text(hover)
    }
}

/// Gives a glyph-only widget its name for screen readers (and the UI tests).
pub fn named(r: &egui::Response, typ: egui::WidgetType, name: &str) {
    r.widget_info(|| egui::WidgetInfo::labeled(typ, r.enabled(), name));
}

/// A small glyph button (a rollout's settings box, a step arrow) named and tooltipped `name`.
pub fn small(ui: &mut egui::Ui, glyph: &str, name: &str) -> egui::Response {
    let r = ui.small_button(glyph);
    named(&r, egui::WidgetType::Button, name);
    r.on_hover_text(name)
}

/// The icons actions have where the program shows them on a button: the
/// Customize User Interface Action lists show them, and an action dropped on
/// a toolbar becomes this icon (actions without one become text buttons).
pub const ACTION_ICONS: &[(&str, &str)] = &[
    ("Undo Scene Operation", UNDO),
    ("Redo Scene Operation", REDO),
    ("Select and Link", LINK),
    ("Unlink Selection", UNLINK),
    ("Select Object", SELECT_OBJECT),
    ("Select By Name", SELECT_BY_NAME),
    ("Window/Crossing Selection Toggle", CROSSING),
    ("Select and Move", MOVE),
    ("Select and Rotate", ROTATE),
    ("Select and Scale", SCALE),
    ("Select and Manipulate", MANIPULATE),
    ("Keyboard Shortcut Override Toggle", KEYBOARD_OVERRIDE),
    ("Snaps Toggle", SNAP),
    ("Angle Snap Toggle", ANGLE_SNAP),
    ("Percent Snap Toggle", PERCENT_SNAP),
    ("Spinner Snap Toggle", SPINNER_SNAP),
    ("Edit Named Selection Sets", NAMED_SETS),
    ("Mirror", MIRROR),
    ("Align", ALIGN),
    ("Quick Align", QUICK_ALIGN),
    ("Normal Align ", NORMAL_ALIGN),
    ("Align Camera", ALIGN_CAMERA),
    ("Align to View Dialog", ALIGN_TO_VIEW),
    ("Layer Manager", LAYERS),
    ("Show Ribbon Toggle", GRAPHITE),
    ("Track View Floater", CURVE_EDITOR),
    ("Material Editor Toggle", MATERIAL_EDITOR),
    ("Render SetUp...", RENDER_SETUP),
    ("Rendered Frame Window", RENDERED_FRAME_WINDOW),
    ("Render", RENDER),
    ("New Scene", NEW_SCENE),
    ("Open File", OPEN_FILE),
    ("Save File", SAVE_FILE),
    ("Selection Lock Toggle", LOCKED),
    ("Set Keys", SET_KEYS),
    ("Next/Previous Key Mode Toggle", KEY_MODE),
    ("Time Configuration", TIME_CONFIGURATION),
    ("Go to Start Frame", GO_TO_START),
    ("Backup Time One Unit", PREVIOUS_FRAME),
    ("Play Animation", PLAY),
    ("Forward Time One Unit", NEXT_FRAME),
    ("Go to End Frame", GO_TO_END),
    ("Zoom Mode", ZOOM),
    ("Zoom All", ZOOM_ALL),
    ("Zoom Extents", ZOOM_EXTENTS),
    ("Zoom Extents All", ZOOM_EXTENTS_ALL),
    ("Zoom Region Mode", ZOOM_REGION),
    ("Field-of-View Mode", FIELD_OF_VIEW),
    ("Pan View", PAN),
    ("Orbit View Mode", ORBIT),
    ("Maximize Viewport Toggle", MAXIMIZE),
    ("Hide Selection", HIDE),
    ("Unhide All", UNHIDE_ALL),
    ("Freeze Selection", FREEZE),
    ("Unfreeze All", UNFREEZE_ALL),
    ("Repeat Last Operation", REPEAT_LAST),
    ("Quickslice Mode", QUICKSLICE),
    ("Swift Loop Mode", SWIFT_LOOP),
    ("Cut", CUT),
    ("Grow Selection", GROW),
    ("Shrink Selection", SHRINK),
    ("Select Edge Loop", LOOP),
    ("Select Edge Ring", RING),
    ("Relax", RELAX),
    ("Create", CREATE),
    ("Attach", ATTACH),
    ("Detach", DETACH),
    ("Slice Plane Mode", SLICE_PLANE),
    ("Extrude Mode", EXTRUDE),
    ("Chamfer Mode", CHAMFER),
    ("Weld Settings", WELD),
    ("Remove", REMOVE),
    ("Break", BREAK),
    ("Target Weld Mode", TARGET_WELD),
    ("Bridge Settings", BRIDGE),
    ("Split Edges", SPLIT),
    ("Insert Vertex Mode", INSERT_VERTEX),
    ("Connect", CONNECT),
    ("Cap", CAP),
    ("Bevel Mode", BEVEL),
    ("Inset Mode", INSET),
    ("Outline Mode", OUTLINE),
    ("Hinge from Edge Mode", HINGE),
    ("Flip Normals", FLIP),
    ("MeshSmooth", MSMOOTH),
    ("Tessellate", TESSELLATE),
    ("Make Planar", MAKE_PLANAR),
    ("Convert to Editable Poly", CONVERT_TO_POLY),
];

/// The icon an action shows on a button, if it has one.
pub fn for_action(action: &str) -> Option<&'static str> {
    ACTION_ICONS.iter().find(|(a, _)| *a == action).map(|(_, g)| *g)
}

/// Edit Button Appearance > Image Button: the icons to choose from, by name.
pub const GALLERY: &[(&str, &str)] = &[
    ("Undo", UNDO), ("Redo", REDO), ("Link", LINK), ("Unlink", UNLINK), ("Select", SELECT_OBJECT), ("Select By Name", SELECT_BY_NAME),
    ("Move", MOVE), ("Rotate", ROTATE), ("Scale", SCALE), ("Manipulate", MANIPULATE), ("Keyboard", KEYBOARD_OVERRIDE), ("Snap", SNAP),
    ("Angle Snap", ANGLE_SNAP), ("Percent Snap", PERCENT_SNAP), ("Spinner Snap", SPINNER_SNAP), ("Named Sets", NAMED_SETS), ("Mirror", MIRROR),
    ("Align", ALIGN), ("Quick Align", QUICK_ALIGN), ("Normal Align", NORMAL_ALIGN), ("Camera", ALIGN_CAMERA), ("View", ALIGN_TO_VIEW),
    ("Layers", LAYERS), ("Ribbon", GRAPHITE), ("Curve Editor", CURVE_EDITOR), ("Material Editor", MATERIAL_EDITOR), ("Render Setup", RENDER_SETUP),
    ("Frame Window", RENDERED_FRAME_WINDOW), ("Render", RENDER), ("New", NEW_SCENE), ("Open", OPEN_FILE), ("Save", SAVE_FILE),
    ("Geometry", GEOMETRY), ("Shapes", SHAPES), ("Lights", LIGHTS), ("Cameras", CAMERAS), ("Helpers", HELPERS), ("Systems", SYSTEMS),
    ("Pin", PIN_STACK), ("Settings", SETTINGS), ("Lock", LOCKED), ("Key", SET_KEYS), ("Clock", TIME_CONFIGURATION), ("Play", PLAY),
    ("Zoom", ZOOM), ("Zoom All", ZOOM_ALL), ("Zoom Extents", ZOOM_EXTENTS), ("Zoom Region", ZOOM_REGION), ("Pan", PAN), ("Orbit", ORBIT),
    ("Maximize", MAXIMIZE), ("Hide", HIDE), ("Unhide", UNHIDE_ALL), ("Freeze", FREEZE), ("Unfreeze", UNFREEZE_ALL), ("Repeat", REPEAT_LAST),
    ("Cut", CUT), ("Grow", GROW), ("Shrink", SHRINK), ("Loop", LOOP), ("Ring", RING), ("Relax", RELAX), ("Create", CREATE), ("Attach", ATTACH),
    ("Detach", DETACH), ("Extrude", EXTRUDE), ("Chamfer", CHAMFER), ("Weld", WELD), ("Bridge", BRIDGE), ("Connect", CONNECT), ("Bevel", BEVEL),
    ("Inset", INSET), ("Outline", OUTLINE), ("Flip", FLIP), ("Smooth", MSMOOTH), ("Tessellate", TESSELLATE), ("Bone", KIND_BONE), ("Group", KIND_GROUP),
];

/// A gallery icon by its name.
pub fn by_name(name: &str) -> Option<&'static str> {
    GALLERY.iter().find(|(n, _)| *n == name).map(|(_, g)| *g)
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
            assert!(f.lock().fonts.definitions().font_data.contains_key(FONT), "the icon font is loaded");
            // The icon font only follows the text font: letters stay Ubuntu's.
            assert_eq!(f.lock().fonts.definitions().families[&egui::FontFamily::Proportional][0], "Ubuntu-Light");
        });
    }

    /// The non-ASCII characters inside string literals of `src` (comments skipped).
    fn literal_chars(src: &str) -> Vec<char> {
        let mut out = Vec::new();
        for line in src.lines() {
            let code = line.split("//").next().unwrap_or("");
            let mut in_str = false;
            let mut escaped = false;
            for c in code.chars() {
                match (in_str, escaped, c) {
                    (true, true, _) => escaped = false,
                    (true, false, '\\') => escaped = true,
                    (_, _, '"') => in_str = !in_str,
                    (true, false, c) if !c.is_ascii() => out.push(c),
                    _ => {}
                }
            }
        }
        out
    }

    #[test]
    fn every_character_the_ui_draws_has_a_glyph() {
        // Without this an icon or symbol that no loaded font has shows as an empty box.
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(Default::default(), install);
        }
        let mut files = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut missing = Vec::new();
        while let Some(p) = files.pop() {
            if p.is_dir() {
                files.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
            } else if p.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&p).unwrap();
                for c in literal_chars(&src) {
                    if !ctx.fonts(|f| f.has_glyph(&egui::FontId::proportional(14.0), c)) {
                        missing.push(format!("{c:?} (U+{:04X}) in {}", c as u32, p.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "characters no loaded font draws: {missing:#?}");
        assert_eq!(literal_chars("let a = \"\u{21b6} x\"; // \"\u{23da}\""), vec!['\u{21b6}']);
    }

    #[test]
    fn hover_text_carries_the_chord_and_the_hint() {
        let s = AppState::new();
        assert_eq!(Icon::new(MOVE, "Select and Move").hover_text(&s), "Select and Move (W)");
        assert_eq!(Icon::new(ANGLE_SNAP, "Angle Snap Toggle").hint("right-click: Grid and Snap Settings").hover_text(&s), "Angle Snap Toggle (A); right-click: Grid and Snap Settings");
        assert_eq!(Icon::new(SELECT_BY_NAME, "Select By Name").tip("Select From Scene").hover_text(&s), "Select From Scene (H)");
    }
}
