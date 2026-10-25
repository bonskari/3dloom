//! Customize User Interface beyond the Keyboard panel: toolbars (the user's
//! own toolbars, where every toolbar and the command panel docks or floats,
//! button looks), quad menus, menus, colours, and the command panel's
//! rollout order. All of it is saved in the settings file and in UI
//! schemes; each part also has its own file in 3dLoom's JSON format: .cui
//! (toolbars), .mnu (menus and quad menus), .clr (colours), .qop (quad menu
//! options).

use crate::command::Command as C;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Where a toolbar or the command panel sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Dock {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
    /// A floating window.
    Float,
}

impl Dock {
    pub const ALL: [Dock; 5] = [Dock::Top, Dock::Bottom, Dock::Left, Dock::Right, Dock::Float];
    pub fn label(self) -> &'static str {
        match self {
            Dock::Top => "Top",
            Dock::Bottom => "Bottom",
            Dock::Left => "Left",
            Dock::Right => "Right",
            Dock::Float => "Float",
        }
    }
    pub fn parse(s: &str) -> Result<Dock, String> {
        Dock::ALL.into_iter().find(|d| d.label().eq_ignore_ascii_case(s.trim())).ok_or_else(|| format!("unknown dock {s:?} (Top, Bottom, Left, Right or Float)"))
    }
}

/// The toolbars that come with the program, in the Customize Display menu's order.
pub const BUILTIN_TOOLBARS: [&str; 6] = ["Command Panel", "Main Toolbar", "Axis Constraints", "Layers", "Extras", "Snaps"];

/// A toolbar item that is a separator line.
pub const SEPARATOR: &str = "-";
/// A menu or quad item that opens a menu: `menu:<name>`.
pub const MENU_PREFIX: &str = "menu:";

/// A toolbar the user made (Toolbars panel > New).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toolbar {
    pub name: String,
    /// Action names, or `SEPARATOR`.
    pub items: Vec<String>,
    pub shown: bool,
    pub dock: Dock,
    /// The floating window's top-left corner.
    pub pos: [f32; 2],
}

impl Default for Toolbar {
    fn default() -> Self {
        Toolbar { name: String::new(), items: Vec::new(), shown: true, dock: Dock::Float, pos: [200.0, 160.0] }
    }
}

/// Edit Button Appearance: a button as text or as an icon, with its tooltip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ButtonLook {
    /// Image Button (the icon) rather than Text Button.
    pub image: bool,
    /// The Text Button's label (empty: the action name).
    pub text: String,
    /// The icon's name (see the UI's icon list; empty: the action's own icon).
    pub icon: String,
    /// The tooltip (empty: the action name).
    pub tooltip: String,
}

/// A menu or quad entry the user added: an action, a separator or a menu,
/// with the name it shows (Edit Menu Item Name; empty: its own name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomItem {
    /// An action name, `SEPARATOR`, or `menu:<name>`.
    pub item: String,
    pub name: String,
}

impl CustomItem {
    pub fn new(item: &str) -> Self {
        CustomItem { item: item.to_owned(), name: String::new() }
    }
    /// The menu it opens, if it is a menu.
    pub fn menu(&self) -> Option<&str> {
        self.item.strip_prefix(MENU_PREFIX)
    }
    pub fn is_separator(&self) -> bool {
        self.item == SEPARATOR
    }
    /// What it shows: the custom name, else the action or menu name.
    pub fn label(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else {
            self.menu().unwrap_or(&self.item)
        }
    }
}

/// The quad sets that come with the program (the right-click with Shift,
/// Ctrl, Alt or Ctrl+Alt opens the others).
pub const BUILTIN_QUADS: [&str; 5] = ["Default Viewport Quad", "Snap", "Modeling", "Animation", "Lighting | Render"];

/// Quadrant names in the panel's order: upper right, lower right, upper left, lower left.
pub const QUADRANTS: [&str; 4] = ["Upper Right", "Lower Right", "Upper Left", "Lower Left"];

/// One quad set's customizations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuadSet {
    /// Quadrant labels (empty: the program's title).
    pub labels: [String; 4],
    /// Entries added to each quadrant, after the program's.
    pub items: [Vec<CustomItem>; 4],
    /// Labels of the program's entries the user deleted from each quadrant.
    pub hidden: [Vec<String>; 4],
    /// Show All Quads: off shows only the quadrant under the pointer.
    pub show_all: bool,
    /// A set the user made (only these can be deleted).
    pub user: bool,
    /// Quad Shortcut: the chord (chart spelling) that opens the set at the pointer; empty: none.
    pub shortcut: String,
}

impl Default for QuadSet {
    fn default() -> Self {
        QuadSet { labels: Default::default(), items: Default::default(), hidden: Default::default(), show_all: true, user: false, shortcut: String::new() }
    }
}

/// Advanced Quad Menu Options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuadOptions {
    /// Per quadrant colours of each element (`QUAD_COLOR_ELEMENTS` order).
    pub colors: BTreeMap<String, [[u8; 3]; 4]>,
    /// Elements whose four quadrants share one colour (the L button).
    pub locked: BTreeMap<String, bool>,
    /// The quadrant the pointer starts in (index into `QUADRANTS`).
    pub starting_quadrant: u32,
    pub uniform_width: bool,
    /// Text justified to the menu's inner edge.
    pub mirror: bool,
    /// Extra spacing between entries, in pixels.
    pub vertical_margins: f32,
    /// 0–100.
    pub opacity: f32,
    pub reposition_off_screen: bool,
    pub move_cursor_when_repositioned: bool,
    pub return_cursor_after: bool,
    pub title_font: String,
    pub title_size: f32,
    pub menu_font: String,
    pub menu_size: f32,
    /// None, Stretch or Fade.
    pub animation: String,
    pub steps: u32,
    /// Milliseconds between steps.
    pub pause: u32,
}

/// Quad menu colour elements and their defaults (the program's colours).
pub const QUAD_COLOR_ELEMENTS: [(&str, [u8; 3]); 7] = [
    ("Title Bar Background", [45, 45, 45]),
    ("Title Bar Text", [255, 200, 80]),
    ("Background", [40, 40, 40]),
    ("Text", [220, 220, 220]),
    ("Highlighted Text", [255, 255, 255]),
    ("Disabled Text", [120, 120, 120]),
    ("Border", [90, 90, 90]),
];

impl Default for QuadOptions {
    fn default() -> Self {
        QuadOptions {
            colors: QUAD_COLOR_ELEMENTS.iter().map(|(n, c)| ((*n).to_owned(), [*c; 4])).collect(),
            locked: QUAD_COLOR_ELEMENTS.iter().map(|(n, _)| ((*n).to_owned(), true)).collect(),
            starting_quadrant: 1,
            uniform_width: true,
            mirror: false,
            vertical_margins: 1.0,
            opacity: 100.0,
            reposition_off_screen: true,
            move_cursor_when_repositioned: true,
            return_cursor_after: false,
            title_font: "Proportional".into(),
            title_size: 11.0,
            menu_font: "Proportional".into(),
            menu_size: 13.0,
            animation: "None".into(),
            steps: 4,
            pause: 20,
        }
    }
}

impl QuadOptions {
    /// An element's colour in quadrant `q` (0..4).
    pub fn color(&self, element: &str, q: usize) -> [u8; 3] {
        self.colors.get(element).map(|c| c[q.min(3)]).or_else(|| QUAD_COLOR_ELEMENTS.iter().find(|(n, _)| *n == element).map(|(_, c)| *c)).unwrap_or([255, 255, 255])
    }
}

/// The menu bar's menus, in order (MAXScript is not part of 3dLoom).
pub const BUILTIN_MENUS: [&str; 12] = ["File", "Edit", "Tools", "Group", "Views", "Create", "Modifiers", "Animation", "Graph Editors", "Rendering", "Customize", "Help"];

/// The default titles of the menu bar's menus; `&` marks the Alt accelerator.
pub const DEFAULT_TITLES: [&str; 12] = ["&File", "&Edit", "&Tools", "&Group", "&Views", "&Create", "Mo&difiers", "A&nimation", "Gr&aph Editors", "&Rendering", "C&ustomize", "&Help"];

/// A menu the user made (Menus panel > New).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserMenu {
    pub name: String,
    pub items: Vec<CustomItem>,
    /// Shown on the menu bar (before Help).
    pub on_bar: bool,
}

/// Menus panel edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Menus {
    /// Titles the user gave the program's menus (`&` marks the accelerator).
    pub titles: BTreeMap<String, String>,
    /// Entries added after a menu's own.
    pub extra: BTreeMap<String, Vec<CustomItem>>,
    /// Labels of a menu's own entries the user deleted.
    pub hidden: BTreeMap<String, Vec<String>>,
    pub user: Vec<UserMenu>,
}

impl Menus {
    /// A menu's title with its `&`: the user's, else the default.
    pub fn title(&self, menu: &str) -> String {
        if let Some(t) = self.titles.get(menu) {
            return t.clone();
        }
        BUILTIN_MENUS.iter().position(|m| *m == menu).map(|i| DEFAULT_TITLES[i].to_owned()).unwrap_or_else(|| menu.to_owned())
    }
    pub fn is_hidden(&self, menu: &str, label: &str) -> bool {
        self.hidden.get(menu).is_some_and(|v| v.iter().any(|l| l == label))
    }
    pub fn user_menu(&self, name: &str) -> Option<&UserMenu> {
        self.user.iter().find(|m| m.name == name)
    }
    fn exists(&self, name: &str) -> bool {
        BUILTIN_MENUS.contains(&name) || self.user_menu(name).is_some()
    }
}

/// A title without its `&` marker, and the accelerator letter (lowercase).
pub fn accelerator(title: &str) -> (String, Option<char>) {
    let mut out = String::new();
    let mut key = None;
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => out.push('&'),
                Some(n) => {
                    if key.is_none() {
                        key = Some(n.to_ascii_lowercase());
                    }
                    out.push(n);
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    (out, key)
}

/// Colors panel: (category, element, default colour). The viewports, grids,
/// gizmos and track bar draw with these.
pub const COLOR_ELEMENTS: &[(&str, &str, [u8; 3])] = &[
    ("Viewports", "Viewport Background", [80, 80, 80]),
    ("Viewports", "Perspective Background Top", [38, 38, 38]),
    ("Viewports", "Perspective Background Bottom", [96, 96, 96]),
    ("Viewports", "Active Viewport Border", [255, 230, 0]),
    ("Viewports", "Viewport Label Text", [225, 225, 225]),
    ("Viewports", "Viewport Statistics Text", [230, 230, 230]),
    ("Grids", "Home Grid", [97, 97, 97]),
    ("Grids", "Home Grid Major Lines", [140, 140, 140]),
    ("Grids", "Home Grid World Axes", [0, 0, 0]),
    ("Geometry", "Selection", [255, 255, 255]),
    ("Geometry", "Frozen", [89, 89, 89]),
    ("Gizmos", "X Axis", [230, 60, 60]),
    ("Gizmos", "Y Axis", [70, 200, 70]),
    ("Gizmos", "Z Axis", [70, 110, 255]),
    ("Gizmos", "Active Axis", [255, 255, 0]),
    ("Track Bar", "Position Keys", [220, 60, 60]),
    ("Track Bar", "Rotation Keys", [70, 200, 70]),
    ("Track Bar", "Scale Keys", [80, 120, 255]),
    ("Track Bar", "Other Keys", [170, 170, 170]),
    ("Track Bar", "Selected Keys", [255, 255, 255]),
    ("Time Slider", "Auto Key Background", [200, 50, 50]),
];

/// Colors panel > UI Appearance elements of the Custom Colors scheme and
/// their defaults (the dark scheme's colours).
pub const APPEARANCE_ELEMENTS: &[(&str, [u8; 3])] = &[
    ("Panel Background", [27, 27, 27]),
    ("Window Background", [27, 27, 27]),
    ("Text", [190, 190, 190]),
    ("Button", [60, 60, 60]),
    ("Button Hovered", [70, 70, 70]),
    ("Button Pressed", [55, 55, 55]),
    ("Selection Highlight", [0, 92, 128]),
    ("Hyperlink", [90, 170, 255]),
    ("Text Field Background", [10, 10, 10]),
];

/// Colors panel > Scheme.
pub const SCHEMES: [&str; 3] = ["Dark", "Light", "Custom Colors"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiColors {
    /// Colours that differ from `COLOR_ELEMENTS`' defaults.
    pub elements: BTreeMap<String, [u8; 3]>,
    /// Dark, Light or Custom Colors (then `appearance` sets the UI colours).
    pub scheme: String,
    pub appearance: BTreeMap<String, [u8; 3]>,
}

impl Default for UiColors {
    fn default() -> Self {
        UiColors { elements: BTreeMap::new(), scheme: "Dark".into(), appearance: BTreeMap::new() }
    }
}

impl UiColors {
    /// An element's colour (its default unless set).
    pub fn get(&self, element: &str) -> [u8; 3] {
        self.elements.get(element).copied().or_else(|| default_color(element)).unwrap_or([255, 0, 255])
    }
    pub fn appearance(&self, element: &str) -> [u8; 3] {
        self.appearance.get(element).copied().or_else(|| APPEARANCE_ELEMENTS.iter().find(|(n, _)| *n == element).map(|(_, c)| *c)).unwrap_or([255, 0, 255])
    }
}

pub fn default_color(element: &str) -> Option<[u8; 3]> {
    COLOR_ELEMENTS.iter().find(|(_, n, _)| *n == element).map(|(_, _, c)| *c)
}

/// Everything the Toolbars, Quads, Menus and Colors panels edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UiCustom {
    pub toolbars: Vec<Toolbar>,
    /// Where the program's toolbars and the command panel sit (default: their own place).
    pub docks: BTreeMap<String, Dock>,
    /// Floating positions of the program's toolbars.
    pub float_pos: BTreeMap<String, [f32; 2]>,
    /// Shown state of the program's toolbars without a Show UI toggle of their own.
    pub shown: BTreeMap<String, bool>,
    /// Edit Button Appearance, by action.
    pub looks: BTreeMap<String, ButtonLook>,
    pub quads: BTreeMap<String, QuadSet>,
    pub quad_options: QuadOptions,
    pub menus: Menus,
    pub colors: UiColors,
    /// The command panel's rollout order, by panel.
    pub rollout_order: BTreeMap<String, Vec<String>>,
}

impl UiCustom {
    /// Where a toolbar sits: the user's dock, else its default.
    pub fn dock(&self, name: &str) -> Dock {
        if let Some(t) = self.toolbar(name) {
            return t.dock;
        }
        self.docks.get(name).copied().unwrap_or(match name {
            "Command Panel" => Dock::Right,
            "Main Toolbar" | "Layers" => Dock::Top,
            "Axis Constraints" | "Extras" | "Snaps" => Dock::Float,
            _ => Dock::Float,
        })
    }
    pub fn float_pos(&self, name: &str) -> [f32; 2] {
        if let Some(t) = self.toolbar(name) {
            return t.pos;
        }
        self.float_pos.get(name).copied().unwrap_or(match name {
            "Axis Constraints" => [300.0, 120.0],
            "Extras" => [460.0, 120.0],
            "Render Shortcuts" => [600.0, 120.0],
            "Snaps" => [300.0, 170.0],
            "Command Panel" => [800.0, 100.0],
            _ => [200.0, 120.0],
        })
    }
    pub fn toolbar(&self, name: &str) -> Option<&Toolbar> {
        self.toolbars.iter().find(|t| t.name == name)
    }
    fn toolbar_mut(&mut self, name: &str) -> Result<&mut Toolbar, String> {
        self.toolbars.iter_mut().find(|t| t.name == name).ok_or_else(|| format!("no toolbar {name:?} of yours"))
    }
    /// The quad set a Quad Shortcut chord opens.
    pub fn quad_for_shortcut(&self, chord: &str) -> Option<&str> {
        self.quads.iter().find(|(_, q)| !q.shortcut.is_empty() && q.shortcut == chord).map(|(n, _)| n.as_str())
    }
    pub fn quad(&self, set: &str) -> QuadSet {
        self.quads.get(set).cloned().unwrap_or_default()
    }
    /// Every quad set: the program's, then the user's.
    pub fn quad_sets(&self) -> Vec<String> {
        let mut v: Vec<String> = BUILTIN_QUADS.iter().map(|s| (*s).to_owned()).collect();
        v.extend(self.quads.iter().filter(|(_, q)| q.user).map(|(n, _)| n.clone()));
        v
    }
}

/// Is `name` a registered action?
fn known_action(name: &str) -> bool {
    crate::registry().iter().any(|i| i.name == name)
}

/// Checks a toolbar / menu / quad item: an action, a separator or `menu:<name>`.
fn check_item(state: &AppState, item: &str) -> Result<(), String> {
    if item == SEPARATOR || known_action(item) {
        return Ok(());
    }
    if let Some(m) = item.strip_prefix(MENU_PREFIX) {
        return if state.ui_custom.menus.exists(m) { Ok(()) } else { Err(format!("no menu {m:?}")) };
    }
    Err(format!("unknown action {item:?}"))
}

fn quadrant(q: u32) -> Result<usize, String> {
    if q < 4 { Ok(q as usize) } else { Err(format!("quadrant {q} (0 upper right, 1 lower right, 2 upper left, 3 lower left)")) }
}

fn quad_set_mut<'a>(state: &'a mut AppState, set: &str) -> Result<&'a mut QuadSet, String> {
    if !state.ui_custom.quad_sets().iter().any(|s| s == set) {
        return Err(format!("no quad set {set:?}"));
    }
    Ok(state.ui_custom.quads.entry(set.to_owned()).or_default())
}

/// The list of a menu's added entries (a user menu's whole list).
fn menu_items_mut<'a>(state: &'a mut AppState, menu: &str) -> Result<&'a mut Vec<CustomItem>, String> {
    let m = &mut state.ui_custom.menus;
    if let Some(u) = m.user.iter_mut().find(|u| u.name == menu) {
        return Ok(&mut u.items);
    }
    if BUILTIN_MENUS.contains(&menu) {
        return Ok(m.extra.entry(menu.to_owned()).or_default());
    }
    Err(format!("no menu {menu:?}"))
}

/// Sets a program toolbar's or the command panel's shown state.
fn set_shown(state: &mut AppState, name: &str, on: Option<bool>) -> Result<bool, String> {
    let flip = |v: &mut bool| {
        *v = on.unwrap_or(!*v);
        *v
    };
    Ok(match name {
        "Command Panel" => flip(&mut state.show_command_panel),
        "Main Toolbar" => flip(&mut state.show_main_toolbar),
        "Layers" => flip(&mut state.show_layers_toolbar),
        "Axis Constraints" | "Extras" | "Snaps" => {
            let v = state.ui_custom.shown.entry(name.to_owned()).or_insert(false);
            flip(v)
        }
        _ => flip(&mut state.ui_custom.toolbar_mut(name)?.shown),
    })
}

/// Is a toolbar shown (program or user toolbar)?
pub fn is_shown(state: &AppState, name: &str) -> bool {
    match name {
        "Command Panel" => state.show_command_panel,
        "Main Toolbar" => state.show_main_toolbar,
        "Layers" => state.show_layers_toolbar,
        "Axis Constraints" | "Extras" | "Snaps" => state.ui_custom.shown.get(name).copied().unwrap_or(false),
        _ => state.ui_custom.toolbar(name).is_some_and(|t| t.shown),
    }
}

/// The .cui part: user toolbars, docks, button looks, shown flags.
#[derive(Serialize, Deserialize)]
struct ToolbarFile {
    format: String,
    toolbars: Vec<Toolbar>,
    docks: BTreeMap<String, Dock>,
    float_pos: BTreeMap<String, [f32; 2]>,
    shown: BTreeMap<String, bool>,
    looks: BTreeMap<String, ButtonLook>,
}

/// The .mnu part: menus and quad sets (one file for both, as in the reference).
#[derive(Serialize, Deserialize)]
struct MenuFile {
    format: String,
    menus: Menus,
    quads: BTreeMap<String, QuadSet>,
}

#[derive(Serialize, Deserialize)]
struct ColorFile {
    format: String,
    colors: UiColors,
}

#[derive(Serialize, Deserialize)]
struct QuadOptionsFile {
    format: String,
    options: QuadOptions,
}

const CUI_FORMAT: &str = "3dloom-toolbars-1";
const MNU_FORMAT: &str = "3dloom-menus-1";
const CLR_FORMAT: &str = "3dloom-colors-1";
const QOP_FORMAT: &str = "3dloom-quad-options-1";

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn check_format(found: &str, want: &str) -> Result<(), String> {
    if found == want { Ok(()) } else { Err(format!("not a {want} file (format {found:?})")) }
}

pub fn handles(c: &C) -> bool {
    matches!(
        c,
        C::NewToolbar { .. }
            | C::DeleteToolbar { .. }
            | C::RenameToolbar { .. }
            | C::ShowToolbar { .. }
            | C::DockToolbar { .. }
            | C::AddToolbarButton { .. }
            | C::RemoveToolbarButton { .. }
            | C::MoveToolbarButton { .. }
            | C::SetButtonAppearance { .. }
            | C::SaveToolbars { .. }
            | C::LoadToolbars { .. }
            | C::ResetToolbars
            | C::NewQuadSet { .. }
            | C::DeleteQuadSet { .. }
            | C::RenameQuadSet { .. }
            | C::SetQuadLabel { .. }
            | C::AddQuadItem { .. }
            | C::RemoveQuadItem { .. }
            | C::HideQuadItem { .. }
            | C::RenameQuadItem { .. }
            | C::SetShowAllQuads { .. }
            | C::SetQuadShortcut { .. }
            | C::ShowQuadMenu { .. }
            | C::SetQuadOption { .. }
            | C::SetQuadColor { .. }
            | C::SaveQuadOptions { .. }
            | C::LoadQuadOptions { .. }
            | C::ResetQuadOptions
            | C::NewMenu { .. }
            | C::DeleteMenu { .. }
            | C::RenameMenu { .. }
            | C::AddMenuItem { .. }
            | C::RemoveMenuItem { .. }
            | C::HideMenuItem { .. }
            | C::RenameMenuItem { .. }
            | C::SetMenuOnBar { .. }
            | C::SaveMenus { .. }
            | C::LoadMenus { .. }
            | C::ResetMenus
            | C::SetUiColor { .. }
            | C::SetColorScheme { .. }
            | C::ResetUiColors
            | C::SaveUiColors { .. }
            | C::LoadUiColors { .. }
            | C::SetRolloutOrder { .. }
    )
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    use crate::viewport::{Dialog, UiFileKind};
    let file_dialog = |state: &mut AppState, kind: UiFileKind, save: bool| -> Result<Value, String> {
        state.dialog = Some(Dialog::UiFile { kind, save });
        Ok(json!({ "dialog": format!("{} {}", if save { "Save" } else { "Load" }, kind.label()) }))
    };
    match cmd {
        C::NewToolbar { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("a toolbar needs a name".into());
            }
            if BUILTIN_TOOLBARS.contains(&name.as_str()) || state.ui_custom.toolbar(&name).is_some() {
                return Err(format!("a toolbar named {name:?} exists"));
            }
            let n = state.ui_custom.toolbars.len() as f32;
            state.ui_custom.toolbars.push(Toolbar { name: name.clone(), pos: [200.0 + 24.0 * n, 160.0 + 24.0 * n], ..Default::default() });
            Ok(json!({ "toolbar": name }))
        }
        C::DeleteToolbar { name } => {
            let before = state.ui_custom.toolbars.len();
            state.ui_custom.toolbars.retain(|t| t.name != name);
            if state.ui_custom.toolbars.len() == before {
                return Err(if BUILTIN_TOOLBARS.contains(&name.as_str()) { format!("{name} comes with 3dLoom: hide it instead") } else { format!("no toolbar {name:?} of yours") });
            }
            Ok(json!({ "deleted": name }))
        }
        C::RenameToolbar { name, to } => {
            let to = to.trim().to_owned();
            if to.is_empty() || BUILTIN_TOOLBARS.contains(&to.as_str()) || state.ui_custom.toolbar(&to).is_some() {
                return Err(format!("cannot rename to {to:?}"));
            }
            state.ui_custom.toolbar_mut(&name)?.name = to.clone();
            Ok(json!({ "toolbar": to }))
        }
        C::ShowToolbar { name, on } => {
            let shown = set_shown(state, &name, on)?;
            Ok(json!({ "toolbar": name, "shown": shown }))
        }
        C::DockToolbar { name, dock, pos } => {
            let d = Dock::parse(&dock)?;
            if name == "Command Panel" && matches!(d, Dock::Top | Dock::Bottom) {
                return Err("the command panel docks left or right, or floats".into());
            }
            if let Ok(t) = state.ui_custom.toolbar_mut(&name) {
                t.dock = d;
                if let Some(p) = pos {
                    t.pos = p;
                }
            } else if BUILTIN_TOOLBARS.contains(&name.as_str()) {
                state.ui_custom.docks.insert(name.clone(), d);
                if let Some(p) = pos {
                    state.ui_custom.float_pos.insert(name.clone(), p);
                }
            } else {
                return Err(format!("no toolbar {name:?}"));
            }
            if !is_shown(state, &name) {
                set_shown(state, &name, Some(true))?;
            }
            Ok(json!({ "toolbar": name, "dock": d.label() }))
        }
        C::AddToolbarButton { toolbar, action, index } => {
            check_item(state, &action)?;
            if action.starts_with(MENU_PREFIX) {
                return Err("toolbars hold actions and separators".into());
            }
            let t = state.ui_custom.toolbar_mut(&toolbar)?;
            let i = index.map(|i| (i as usize).min(t.items.len())).unwrap_or(t.items.len());
            t.items.insert(i, action);
            Ok(json!({ "toolbar": toolbar, "index": i, "items": t.items }))
        }
        C::RemoveToolbarButton { toolbar, index } => {
            let t = state.ui_custom.toolbar_mut(&toolbar)?;
            if (index as usize) >= t.items.len() {
                return Err(format!("{toolbar} has {} buttons", t.items.len()));
            }
            let removed = t.items.remove(index as usize);
            Ok(json!({ "removed": removed, "items": t.items }))
        }
        C::MoveToolbarButton { toolbar, index, to_toolbar, to_index, copy } => {
            let item = state.ui_custom.toolbar(&toolbar).and_then(|t| t.items.get(index as usize).cloned()).ok_or_else(|| format!("{toolbar} has no button {index}"))?;
            state.ui_custom.toolbar_mut(&to_toolbar)?;
            if !copy {
                state.ui_custom.toolbar_mut(&toolbar)?.items.remove(index as usize);
            }
            let t = state.ui_custom.toolbar_mut(&to_toolbar)?;
            let i = to_index.map(|i| (i as usize).min(t.items.len())).unwrap_or(t.items.len());
            t.items.insert(i, item);
            Ok(json!({ "toolbar": to_toolbar, "items": t.items }))
        }
        C::SetButtonAppearance { action, image, text, icon, tooltip } => {
            if !known_action(&action) {
                return Err(format!("unknown action {action:?}"));
            }
            let look = state.ui_custom.looks.entry(action.clone()).or_default();
            if let Some(v) = image {
                look.image = v;
            }
            if let Some(v) = text {
                look.text = v;
            }
            if let Some(v) = icon {
                look.icon = v;
            }
            if let Some(v) = tooltip {
                look.tooltip = v;
            }
            let look = look.clone();
            if look == ButtonLook::default() {
                state.ui_custom.looks.remove(&action);
            }
            Ok(json!({ "action": action, "image": look.image, "text": look.text, "icon": look.icon, "tooltip": look.tooltip }))
        }
        C::SaveToolbars { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Toolbars, true) };
            let u = &state.ui_custom;
            let f = ToolbarFile { format: CUI_FORMAT.into(), toolbars: u.toolbars.clone(), docks: u.docks.clone(), float_pos: u.float_pos.clone(), shown: u.shown.clone(), looks: u.looks.clone() };
            crate::execute::write_text(&path, &serde_json::to_string_pretty(&f).expect("plain data"))?;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::LoadToolbars { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Toolbars, false) };
            let f: ToolbarFile = serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            check_format(&f.format, CUI_FORMAT)?;
            let u = &mut state.ui_custom;
            (u.toolbars, u.docks, u.float_pos, u.shown, u.looks) = (f.toolbars, f.docks, f.float_pos, f.shown, f.looks);
            back_to_customize(state);
            Ok(json!({ "path": path, "toolbars": state.ui_custom.toolbars.len() }))
        }
        C::ResetToolbars => {
            let u = &mut state.ui_custom;
            u.toolbars.clear();
            u.docks.clear();
            u.float_pos.clear();
            u.shown.clear();
            u.looks.clear();
            state.show_main_toolbar = true;
            state.show_command_panel = true;
            state.show_layers_toolbar = false;
            Ok(json!({}))
        }
        C::NewQuadSet { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() || state.ui_custom.quad_sets().contains(&name) {
                return Err(format!("cannot make a quad set named {name:?}"));
            }
            state.ui_custom.quads.insert(name.clone(), QuadSet { user: true, ..Default::default() });
            Ok(json!({ "quad_set": name }))
        }
        C::DeleteQuadSet { name } => {
            if !state.ui_custom.quads.get(&name).is_some_and(|q| q.user) {
                return Err(format!("{name:?} is not a quad set of yours (the program's sets cannot be deleted)"));
            }
            state.ui_custom.quads.remove(&name);
            Ok(json!({ "deleted": name }))
        }
        C::RenameQuadSet { name, to } => {
            let to = to.trim().to_owned();
            if to.is_empty() || state.ui_custom.quad_sets().contains(&to) || !state.ui_custom.quads.get(&name).is_some_and(|q| q.user) {
                return Err(format!("cannot rename {name:?} to {to:?}"));
            }
            let q = state.ui_custom.quads.remove(&name).expect("checked above");
            state.ui_custom.quads.insert(to.clone(), q);
            Ok(json!({ "quad_set": to }))
        }
        C::SetQuadLabel { set, quadrant: q, label } => {
            let i = quadrant(q)?;
            quad_set_mut(state, &set)?.labels[i] = label.trim().to_owned();
            Ok(json!({ "set": set, "quadrant": QUADRANTS[i], "label": label.trim() }))
        }
        C::AddQuadItem { set, quadrant: q, item, index } => {
            let i = quadrant(q)?;
            check_item(state, &item)?;
            let items = &mut quad_set_mut(state, &set)?.items[i];
            let at = index.map(|x| (x as usize).min(items.len())).unwrap_or(items.len());
            items.insert(at, CustomItem::new(&item));
            Ok(json!({ "set": set, "quadrant": QUADRANTS[i], "index": at }))
        }
        C::RemoveQuadItem { set, quadrant: q, index } => {
            let i = quadrant(q)?;
            let items = &mut quad_set_mut(state, &set)?.items[i];
            if (index as usize) >= items.len() {
                return Err(format!("that quadrant has {} added entries", items.len()));
            }
            let removed = items.remove(index as usize);
            Ok(json!({ "removed": removed.item }))
        }
        C::HideQuadItem { set, quadrant: q, label, hidden } => {
            let i = quadrant(q)?;
            let list = &mut quad_set_mut(state, &set)?.hidden[i];
            list.retain(|l| *l != label);
            if hidden {
                list.push(label.clone());
            }
            Ok(json!({ "label": label, "hidden": hidden }))
        }
        C::RenameQuadItem { set, quadrant: q, index, name } => {
            let i = quadrant(q)?;
            let items = &mut quad_set_mut(state, &set)?.items[i];
            let it = items.get_mut(index as usize).ok_or_else(|| format!("no added entry {index}"))?;
            it.name = name.trim().to_owned();
            Ok(json!({ "item": it.item, "name": it.name }))
        }
        C::SetShowAllQuads { set, on } => {
            quad_set_mut(state, &set)?.show_all = on;
            Ok(json!({ "set": set, "show_all": on }))
        }
        C::SetQuadShortcut { set, chord } => {
            let text = if chord.trim().is_empty() { String::new() } else { crate::keymap::parse_chord(&chord)?.chart_text() };
            // A chord opens one quad set only.
            if !text.is_empty() {
                for q in state.ui_custom.quads.values_mut() {
                    if q.shortcut == text {
                        q.shortcut.clear();
                    }
                }
            }
            quad_set_mut(state, &set)?.shortcut = text.clone();
            Ok(json!({ "set": set, "shortcut": text }))
        }
        C::ShowQuadMenu { set } => {
            if !state.ui_custom.quad_sets().contains(&set) {
                return Err(format!("no quad set {set:?}"));
            }
            state.quad_request = Some(set.clone());
            Ok(json!({ "quad_set": set }))
        }
        C::SetQuadOption { name, value } => {
            let o = &mut state.ui_custom.quad_options;
            let num = || value.parse::<f32>().map_err(|_| format!("{name} takes a number, not {value:?}"));
            let flag = || match value.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Ok(true),
                "0" | "false" | "off" | "no" => Ok(false),
                _ => Err(format!("{name} takes on or off, not {value:?}")),
            };
            match name.as_str() {
                "Starting Quadrant" => o.starting_quadrant = QUADRANTS.iter().position(|q| q.eq_ignore_ascii_case(&value)).map(|p| p as u32).or_else(|| value.parse().ok().filter(|v| *v < 4)).ok_or_else(|| format!("unknown quadrant {value:?}"))?,
                "Uniform Quad Width" => o.uniform_width = flag()?,
                "Mirror Quads" => o.mirror = flag()?,
                "Vertical Margins" => o.vertical_margins = num()?.clamp(0.0, 20.0),
                "Opacity Amount" => o.opacity = num()?.clamp(10.0, 100.0),
                "Reposition Quad When Off Screen" => o.reposition_off_screen = flag()?,
                "Move Cursor When Repositioned" => o.move_cursor_when_repositioned = flag()?,
                "Return Cursor After Repositioned" => o.return_cursor_after = flag()?,
                "Title Font" | "Menu Font" => {
                    if !["Proportional", "Monospace"].contains(&value.as_str()) {
                        return Err(format!("{name}: Proportional or Monospace, not {value:?}"));
                    }
                    if name == "Title Font" { o.title_font = value.clone() } else { o.menu_font = value.clone() }
                }
                "Title Size" => o.title_size = num()?.clamp(6.0, 32.0),
                "Menu Size" => o.menu_size = num()?.clamp(6.0, 32.0),
                "Animation Type" => {
                    if !["None", "Stretch", "Fade"].contains(&value.as_str()) {
                        return Err(format!("Animation Type: None, Stretch or Fade, not {value:?}"));
                    }
                    o.animation = value.clone();
                }
                "Steps" => o.steps = num()?.clamp(1.0, 30.0) as u32,
                "Pause" => o.pause = num()?.clamp(0.0, 500.0) as u32,
                _ => return Err(format!("unknown quad option {name:?}")),
            }
            Ok(json!({ "name": name, "value": value }))
        }
        C::SetQuadColor { element, quadrant: q, color, locked } => {
            if !QUAD_COLOR_ELEMENTS.iter().any(|(n, _)| *n == element) {
                return Err(format!("unknown quad colour {element:?}"));
            }
            let i = quadrant(q)?;
            let o = &mut state.ui_custom.quad_options;
            if let Some(l) = locked {
                o.locked.insert(element.clone(), l);
            }
            if let Some(c) = color {
                let lock = o.locked.get(&element).copied().unwrap_or(true);
                let entry = o.colors.entry(element.clone()).or_insert([c; 4]);
                if lock {
                    *entry = [c; 4];
                } else {
                    entry[i] = c;
                }
            }
            Ok(json!({ "element": element, "colors": o.colors.get(&element) }))
        }
        C::SaveQuadOptions { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::QuadOptions, true) };
            let f = QuadOptionsFile { format: QOP_FORMAT.into(), options: state.ui_custom.quad_options.clone() };
            crate::execute::write_text(&path, &serde_json::to_string_pretty(&f).expect("plain data"))?;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::LoadQuadOptions { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::QuadOptions, false) };
            let f: QuadOptionsFile = serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            check_format(&f.format, QOP_FORMAT)?;
            state.ui_custom.quad_options = f.options;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::ResetQuadOptions => {
            state.ui_custom.quad_options = QuadOptions::default();
            Ok(json!({}))
        }
        C::NewMenu { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() || state.ui_custom.menus.exists(&name) {
                return Err(format!("cannot make a menu named {name:?}"));
            }
            state.ui_custom.menus.user.push(UserMenu { name: name.clone(), items: Vec::new(), on_bar: false });
            Ok(json!({ "menu": name }))
        }
        C::DeleteMenu { name } => {
            let m = &mut state.ui_custom.menus;
            let before = m.user.len();
            m.user.retain(|u| u.name != name);
            if m.user.len() == before {
                return Err(format!("{name:?} is not a menu of yours"));
            }
            // Entries that opened it go too.
            let gone = format!("{MENU_PREFIX}{name}");
            for list in m.extra.values_mut().chain(m.user.iter_mut().map(|u| &mut u.items)) {
                list.retain(|i| i.item != gone);
            }
            for q in state.ui_custom.quads.values_mut() {
                for list in q.items.iter_mut() {
                    list.retain(|i| i.item != gone);
                }
            }
            Ok(json!({ "deleted": name }))
        }
        C::RenameMenu { menu, title } => {
            if !state.ui_custom.menus.exists(&menu) {
                return Err(format!("no menu {menu:?}"));
            }
            let title = title.trim().to_owned();
            if accelerator(&title).0.is_empty() {
                state.ui_custom.menus.titles.remove(&menu);
            } else {
                state.ui_custom.menus.titles.insert(menu.clone(), title.clone());
            }
            Ok(json!({ "menu": menu, "title": state.ui_custom.menus.title(&menu) }))
        }
        C::AddMenuItem { menu, item, index } => {
            check_item(state, &item)?;
            if item.strip_prefix(MENU_PREFIX) == Some(menu.as_str()) {
                return Err("a menu cannot hold itself".into());
            }
            let items = menu_items_mut(state, &menu)?;
            let at = index.map(|x| (x as usize).min(items.len())).unwrap_or(items.len());
            items.insert(at, CustomItem::new(&item));
            Ok(json!({ "menu": menu, "index": at }))
        }
        C::RemoveMenuItem { menu, index } => {
            let items = menu_items_mut(state, &menu)?;
            if (index as usize) >= items.len() {
                return Err(format!("{menu} has {} added entries", items.len()));
            }
            let removed = items.remove(index as usize);
            Ok(json!({ "removed": removed.item }))
        }
        C::HideMenuItem { menu, label, hidden } => {
            if !BUILTIN_MENUS.contains(&menu.as_str()) {
                return Err(format!("{menu:?} is not one of the program's menus"));
            }
            let list = state.ui_custom.menus.hidden.entry(menu.clone()).or_default();
            list.retain(|l| *l != label);
            if hidden {
                list.push(label.clone());
            }
            Ok(json!({ "menu": menu, "label": label, "hidden": hidden }))
        }
        C::RenameMenuItem { menu, index, name } => {
            let items = menu_items_mut(state, &menu)?;
            let it = items.get_mut(index as usize).ok_or_else(|| format!("no added entry {index}"))?;
            it.name = name.trim().to_owned();
            Ok(json!({ "item": it.item, "name": it.name }))
        }
        C::SetMenuOnBar { menu, on } => {
            let u = state.ui_custom.menus.user.iter_mut().find(|u| u.name == menu).ok_or_else(|| format!("{menu:?} is not a menu of yours"))?;
            u.on_bar = on;
            Ok(json!({ "menu": menu, "on_bar": on }))
        }
        C::SaveMenus { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Menus, true) };
            let f = MenuFile { format: MNU_FORMAT.into(), menus: state.ui_custom.menus.clone(), quads: state.ui_custom.quads.clone() };
            crate::execute::write_text(&path, &serde_json::to_string_pretty(&f).expect("plain data"))?;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::LoadMenus { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Menus, false) };
            let f: MenuFile = serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            check_format(&f.format, MNU_FORMAT)?;
            state.ui_custom.menus = f.menus;
            state.ui_custom.quads = f.quads;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::ResetMenus => {
            state.ui_custom.menus = Menus::default();
            state.ui_custom.quads.clear();
            Ok(json!({}))
        }
        C::SetUiColor { element, color } => {
            if let Some(d) = default_color(&element) {
                if d == color {
                    state.ui_custom.colors.elements.remove(&element);
                } else {
                    state.ui_custom.colors.elements.insert(element.clone(), color);
                }
            } else if APPEARANCE_ELEMENTS.iter().any(|(n, _)| *n == element) {
                state.ui_custom.colors.appearance.insert(element.clone(), color);
            } else {
                return Err(format!("unknown colour element {element:?}"));
            }
            state.grid_dirty = true;
            state.mesh_dirty = true;
            Ok(json!({ "element": element, "color": color }))
        }
        C::SetColorScheme { scheme } => {
            let s = SCHEMES.iter().find(|s| s.eq_ignore_ascii_case(&scheme)).ok_or_else(|| format!("unknown scheme {scheme:?} (Dark, Light, Custom Colors)"))?;
            state.ui_custom.colors.scheme = (*s).to_owned();
            Ok(json!({ "scheme": s }))
        }
        C::ResetUiColors => {
            state.ui_custom.colors = UiColors::default();
            state.grid_dirty = true;
            state.mesh_dirty = true;
            Ok(json!({}))
        }
        C::SaveUiColors { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Colors, true) };
            let f = ColorFile { format: CLR_FORMAT.into(), colors: state.ui_custom.colors.clone() };
            crate::execute::write_text(&path, &serde_json::to_string_pretty(&f).expect("plain data"))?;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::LoadUiColors { path } => {
            let Some(path) = path else { return file_dialog(state, UiFileKind::Colors, false) };
            let f: ColorFile = serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            check_format(&f.format, CLR_FORMAT)?;
            state.ui_custom.colors = f.colors;
            state.grid_dirty = true;
            state.mesh_dirty = true;
            back_to_customize(state);
            Ok(json!({ "path": path }))
        }
        C::SetRolloutOrder { panel, order } => {
            if order.is_empty() {
                state.ui_custom.rollout_order.remove(&panel);
            } else {
                state.ui_custom.rollout_order.insert(panel.clone(), order.clone());
            }
            Ok(json!({ "panel": panel, "order": order }))
        }
        other => Err(format!("{} is not a Customize User Interface command", other.name())),
    }
}

/// A Save / Load file dialog opened from the Customize User Interface dialog returns to it.
fn back_to_customize(state: &mut AppState) {
    if matches!(state.dialog, Some(crate::viewport::Dialog::UiFile { .. })) {
        state.dialog = Some(crate::viewport::Dialog::CustomizeUI);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerators_come_from_the_ampersand() {
        assert_eq!(accelerator("&File"), ("File".into(), Some('f')));
        assert_eq!(accelerator("Mo&difiers"), ("Modifiers".into(), Some('d')));
        assert_eq!(accelerator("Fish && Chips"), ("Fish & Chips".into(), None));
        assert_eq!(accelerator("Plain"), ("Plain".into(), None));
    }

    #[test]
    fn default_titles_have_distinct_accelerators() {
        let keys: Vec<char> = DEFAULT_TITLES.iter().map(|t| accelerator(t).1.unwrap()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "{keys:?}");
        for (t, m) in DEFAULT_TITLES.iter().zip(BUILTIN_MENUS) {
            assert_eq!(accelerator(t).0, m);
        }
    }
}
