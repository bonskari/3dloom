//! Scene Explorer windows (Tools > New Scene Explorer): their setups (saved
//! with the scene), the rows each lists, its Find field and Advanced Search.
//!
//! As in the reference: every New Scene Explorer opens another window; all
//! explorers persist in the scene even when closed (Open Explorer reopens the
//! most recently used one, Saved Scene Explorers any); each has its own
//! columns, sort, display-type filters and options. With Sync Selection on
//! (the default) the highlighted rows are the scene selection.

use crate::{AppState, Command, SelectOp};
use loom_scene::{Node, NodeId, ObjectKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The object types the Display toolbar filters by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Category {
    Geometry,
    Shape,
    Light,
    Camera,
    Helper,
    Bone,
    Group,
}

impl Category {
    pub const ALL: [Category; 7] = [Category::Geometry, Category::Shape, Category::Light, Category::Camera, Category::Helper, Category::Bone, Category::Group];

    /// The Type column's word.
    pub fn label(self) -> &'static str {
        match self {
            Category::Geometry => "Geometry",
            Category::Shape => "Shape",
            Category::Light => "Light",
            Category::Camera => "Camera",
            Category::Helper => "Helper",
            Category::Bone => "Bone",
            Category::Group => "Group",
        }
    }

    /// The Display toolbar button / Display > Object Types entry.
    pub fn filter_label(self) -> &'static str {
        match self {
            Category::Geometry => "Geometry",
            Category::Shape => "Shapes",
            Category::Light => "Lights",
            Category::Camera => "Cameras",
            Category::Helper => "Helpers",
            Category::Bone => "Bones",
            Category::Group => "Groups",
        }
    }
}

pub fn category(o: &ObjectKind) -> Category {
    match o {
        ObjectKind::Group { .. } => Category::Group,
        ObjectKind::Camera { .. } => Category::Camera,
        ObjectKind::Light(_) => Category::Light,
        ObjectKind::Bone { .. } => Category::Bone,
        k if k.is_shape() => Category::Shape,
        k if k.is_helper() => Category::Helper,
        _ => Category::Geometry,
    }
}

/// A Scene Explorer column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Column {
    Name,
    Type,
    Color,
    Faces,
    Hidden,
    Frozen,
    Renderable,
    SeeThrough,
    DisplayAsBox,
    CullBackFaces,
    EdgesOnly,
    VertexTicks,
    Trajectory,
    IgnoreExtents,
    ShowFrozenInGray,
    DisplayVertexChannel,
    CastShadows,
    ReceiveShadows,
    HasMaterial,
    Material,
    Layer,
}

impl Column {
    /// Every column, in the Configure Columns order.
    pub const ALL: [Column; 21] = [
        Column::Name, Column::Type, Column::Color, Column::Faces, Column::Hidden, Column::Frozen, Column::Renderable,
        Column::SeeThrough, Column::DisplayAsBox, Column::CullBackFaces, Column::EdgesOnly, Column::VertexTicks,
        Column::Trajectory, Column::IgnoreExtents, Column::ShowFrozenInGray, Column::DisplayVertexChannel,
        Column::CastShadows, Column::ReceiveShadows, Column::HasMaterial, Column::Material, Column::Layer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Type => "Type",
            Column::Color => "Color",
            Column::Faces => "Faces",
            Column::Hidden => "Hidden",
            Column::Frozen => "Frozen",
            Column::Renderable => "Renderable",
            Column::SeeThrough => "See-Through",
            Column::DisplayAsBox => "Display as Box",
            Column::CullBackFaces => "Cull Back Faces",
            Column::EdgesOnly => "Edges Only",
            Column::VertexTicks => "Vertex Ticks",
            Column::Trajectory => "Trajectory",
            Column::IgnoreExtents => "Ignore Extents",
            Column::ShowFrozenInGray => "Show Frozen in Gray",
            Column::DisplayVertexChannel => "Display Vertex Channel",
            Column::CastShadows => "Cast Shadows",
            Column::ReceiveShadows => "Receive Shadows",
            Column::HasMaterial => "Has Material",
            Column::Material => "Material",
            Column::Layer => "Layer",
        }
    }

    pub fn from_label(s: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|c| c.label().eq_ignore_ascii_case(s.trim()))
    }

    /// The object property an on/off column edits (`SetNodesProperty` name).
    pub fn property(self) -> Option<&'static str> {
        Some(match self {
            Column::Hidden => "Hidden",
            Column::Frozen => "Frozen",
            Column::Renderable => "Renderable",
            Column::SeeThrough => "See-Through",
            Column::DisplayAsBox => "Display as Box",
            Column::CullBackFaces => "Backface Cull",
            Column::EdgesOnly => "Edges Only",
            Column::VertexTicks => "Vertex Ticks",
            Column::Trajectory => "Trajectory",
            Column::IgnoreExtents => "Ignore Extents",
            Column::ShowFrozenInGray => "Show Frozen in Gray",
            Column::DisplayVertexChannel => "Vertex Color",
            Column::CastShadows => "Cast Shadows",
            Column::ReceiveShadows => "Receive Shadows",
            _ => return None,
        })
    }
}

/// What a cell shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Bool(bool),
    Color([u8; 3]),
    Number(usize),
    Empty,
}

impl Cell {
    fn cmp(&self, other: &Cell) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Cell::Text(a), Cell::Text(b)) => a.cmp(b),
            (Cell::Bool(a), Cell::Bool(b)) => a.cmp(b),
            (Cell::Color(a), Cell::Color(b)) => a.cmp(b),
            (Cell::Number(a), Cell::Number(b)) => a.cmp(b),
            (Cell::Empty, Cell::Empty) => Ordering::Equal,
            (Cell::Empty, _) => Ordering::Less,
            (_, Cell::Empty) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}

/// The cell of `col` for node `n`.
pub fn cell(state: &AppState, n: &Node, col: Column) -> Cell {
    let s = &state.scene;
    match col {
        Column::Name => Cell::Text(n.name.clone()),
        Column::Type => Cell::Text(category(&n.object).label().into()),
        Column::Color => Cell::Color(n.wire_color),
        Column::Faces => {
            if category(&n.object) == Category::Geometry {
                Cell::Number(s.eval_mesh(n.id).value.mesh.face_count())
            } else {
                Cell::Empty
            }
        }
        Column::Hidden => Cell::Bool(n.hidden),
        Column::Frozen => Cell::Bool(n.frozen),
        Column::HasMaterial => Cell::Bool(s.node_material(n.id).is_some()),
        Column::Material => s.node_material(n.id).map(|m| Cell::Text(m.name.clone())).unwrap_or(Cell::Empty),
        Column::Layer => s.layer(n.layer).map(|l| Cell::Text(l.name.clone())).unwrap_or(Cell::Empty),
        c => {
            let p = c.property().expect("an on/off column");
            Cell::Bool(n.display.get(p).or_else(|| n.render.get(p)).unwrap_or(false))
        }
    }
}

/// One Scene Explorer's setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneExplorer {
    /// Stable id (the window's id while names change).
    pub id: u32,
    pub name: String,
    pub open: bool,
    pub columns: Vec<Column>,
    /// Sort levels, first the primary: (column, ascending). Empty: scene order.
    pub sort: Vec<(Column, bool)>,
    /// The object types listed (Display toolbar).
    pub shown: BTreeSet<Category>,
    pub show_frozen: bool,
    pub show_hidden: bool,
    pub display_children: bool,
    pub sync_selection: bool,
    pub select_children: bool,
    pub find_case_sensitive: bool,
    pub find_wildcards: bool,
    pub find_regex: bool,
    pub lock_cell_editing: bool,
    /// Collapsed hierarchy branches (node ids).
    pub collapsed: BTreeSet<u64>,
    /// The highlighted rows while Sync Selection is off.
    #[serde(skip)]
    pub highlighted: BTreeSet<u64>,
}

impl Default for SceneExplorer {
    fn default() -> Self {
        SceneExplorer {
            id: 0,
            name: String::new(),
            open: false,
            columns: vec![Column::Name],
            sort: Vec::new(),
            shown: Category::ALL.into_iter().collect(),
            show_frozen: true,
            show_hidden: true,
            display_children: true,
            sync_selection: true,
            select_children: false,
            find_case_sensitive: false,
            find_wildcards: false,
            find_regex: false,
            lock_cell_editing: false,
            collapsed: BTreeSet::new(),
            highlighted: BTreeSet::new(),
        }
    }
}

/// Every Scene Explorer of the scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Explorers {
    pub list: Vec<SceneExplorer>,
    /// The most recently used explorer (Open Explorer).
    pub last: Option<String>,
    /// Manage Scene Explorer > Set as Default: the setup new explorers take.
    pub default: Option<SceneExplorer>,
    pub next_id: u32,
}

impl Explorers {
    /// The explorer called `name`, or the most recently used one.
    pub fn index(&self, name: Option<&str>) -> Option<usize> {
        let name = name.or(self.last.as_deref())?;
        self.list.iter().position(|e| e.name == name)
    }

    pub fn get(&self, name: Option<&str>) -> Option<&SceneExplorer> {
        self.index(name).map(|i| &self.list[i])
    }

    fn get_mut(&mut self, name: Option<&str>) -> Result<&mut SceneExplorer, String> {
        let i = self.index(name).ok_or_else(|| match name {
            Some(n) => format!("no Scene Explorer `{n}`"),
            None => "no Scene Explorer".to_owned(),
        })?;
        Ok(&mut self.list[i])
    }

    /// Opens a new explorer named "Scene Explorer N" with the default setup.
    pub fn create(&mut self) -> &SceneExplorer {
        let mut n = 1;
        while self.list.iter().any(|e| e.name == format!("Scene Explorer {n}")) {
            n += 1;
        }
        let mut e = self.default.clone().unwrap_or_default();
        self.next_id += 1;
        e.id = self.next_id;
        e.name = format!("Scene Explorer {n}");
        e.open = true;
        e.highlighted.clear();
        self.last = Some(e.name.clone());
        self.list.push(e);
        self.list.last().expect("just pushed")
    }
}

/// The scene's parent → children table, children in scene order (a parent
/// that no longer exists puts the node at the top level).
pub fn children_of(scene: &loom_scene::Scene) -> BTreeMap<Option<NodeId>, Vec<NodeId>> {
    let mut map: BTreeMap<Option<NodeId>, Vec<NodeId>> = BTreeMap::new();
    for n in scene.nodes() {
        let parent = n.parent.filter(|p| scene.node(*p).is_some());
        map.entry(parent).or_default().push(n.id);
    }
    map
}

/// Whether the node is hidden (by itself, its layer or a hidden group head).
pub fn is_hidden(state: &AppState, n: &Node) -> bool {
    n.hidden || state.scene.layer(n.layer).is_some_and(|l| l.hidden)
}

pub fn is_frozen(state: &AppState, n: &Node) -> bool {
    n.frozen || state.scene.layer(n.layer).is_some_and(|l| l.frozen)
}

/// Whether a closed group holds the node (its members stay out of the list).
fn in_closed_group(state: &AppState, n: &Node) -> bool {
    let mut cur = n.parent;
    while let Some(p) = cur {
        let Some(pn) = state.scene.node(p) else { return false };
        if matches!(pn.object, ObjectKind::Group { open: false }) {
            return true;
        }
        cur = pn.parent;
    }
    false
}

/// Whether the explorer lists the node (type filters, Frozen / Hidden, closed groups).
pub fn listed(state: &AppState, ex: &SceneExplorer, n: &Node) -> bool {
    ex.shown.contains(&category(&n.object)) && (ex.show_hidden || !is_hidden(state, n)) && (ex.show_frozen || !is_frozen(state, n)) && !in_closed_group(state, n)
}

/// A row of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: NodeId,
    pub depth: usize,
    pub has_children: bool,
}

/// The rows in display order: with Display Children each under its nearest
/// listed ancestor, siblings sorted by the sort columns (hierarchies stay
/// together), collapsed branches left out.
pub fn rows(state: &AppState, ex: &SceneExplorer) -> Vec<Row> {
    let s = &state.scene;
    let ids: BTreeSet<NodeId> = s.nodes().filter(|n| listed(state, ex, n)).map(|n| n.id).collect();
    let shown_parent = |n: &Node| -> Option<NodeId> {
        let mut cur = n.parent;
        while let Some(p) = cur {
            if ids.contains(&p) {
                return Some(p);
            }
            cur = s.node(p).and_then(|pn| pn.parent);
        }
        None
    };
    let mut children: BTreeMap<Option<NodeId>, Vec<NodeId>> = BTreeMap::new();
    for n in s.nodes().filter(|n| ids.contains(&n.id)) {
        let parent = if ex.display_children { shown_parent(n) } else { None };
        children.entry(parent).or_default().push(n.id);
    }
    if !ex.sort.is_empty() {
        for kids in children.values_mut() {
            kids.sort_by(|a, b| {
                let (na, nb) = (s.node(*a).expect("listed"), s.node(*b).expect("listed"));
                for (col, asc) in &ex.sort {
                    let o = cell(state, na, *col).cmp(&cell(state, nb, *col));
                    let o = if *asc { o } else { o.reverse() };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
    }
    fn walk(children: &BTreeMap<Option<NodeId>, Vec<NodeId>>, collapsed: &BTreeSet<u64>, parent: Option<NodeId>, depth: usize, out: &mut Vec<Row>) {
        for id in children.get(&parent).map(|v| v.as_slice()).unwrap_or(&[]) {
            let has_children = children.get(&Some(*id)).is_some_and(|c| !c.is_empty());
            out.push(Row { id: *id, depth, has_children });
            if has_children && !collapsed.contains(&id.0) {
                walk(children, collapsed, Some(*id), depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&children, &ex.collapsed, None, 0, &mut out);
    out
}

/// The highlighted rows: the scene selection with Sync Selection, else the explorer's own.
pub fn highlighted(state: &AppState, ex: &SceneExplorer) -> BTreeSet<NodeId> {
    if ex.sync_selection {
        state.selection.iter().copied().collect()
    } else {
        ex.highlighted.iter().map(|i| NodeId(*i)).collect()
    }
}

/// The Find field: the name starts with the phrase (`*` and `?` as
/// wildcards with Find Using Wildcards), or matches a regular expression
/// from its start with Find Using Regular Expressions; case-insensitive
/// unless Find Case Sensitive.
pub fn find_matches(ex: &SceneExplorer, pattern: &str, name: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if ex.find_regex {
        let re = regex::RegexBuilder::new(&format!("^(?:{pattern})")).case_insensitive(!ex.find_case_sensitive).build();
        return re.is_ok_and(|r| r.is_match(name));
    }
    let (p, n): (Vec<char>, Vec<char>) = if ex.find_case_sensitive { (pattern.chars().collect(), name.chars().collect()) } else { (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect()) };
    if !ex.find_wildcards {
        return n.starts_with(&p);
    }
    fn glob(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, _) => true,
            (Some('*'), _) => glob(&p[1..], n) || (!n.is_empty() && glob(p, &n[1..])),
            (Some('?'), Some(_)) => glob(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a == b && glob(&p[1..], &n[1..]),
            (Some(_), None) => false,
        }
    }
    glob(&p, &n)
}

/// An Advanced Search phrase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchTerm {
    pub property: String,
    pub condition: String,
    pub value: String,
}

impl crate::command::ArgSchema for SearchTerm {
    fn schema() -> Value {
        json!({"type":"object","properties":{"property":{"type":"string"},"condition":{"type":"string"},"value":{"type":"string"}},"required":["property","condition","value"]})
    }
}

/// The Advanced Search properties and the conditions each offers.
pub const SEARCH_PROPERTIES: &[(&str, &[&str])] = &[
    ("Name", &["Starts With", "Contains", "Ends With", "Is", "Is Not", "Does Not Contain"]),
    ("Type", &["Is", "Is Not"]),
    ("Hidden", &["Is"]),
    ("Frozen", &["Is"]),
    ("Renderable", &["Is"]),
    ("Has Material", &["Is"]),
    ("Faces", &["Is Greater Than", "Is Less Than", "Is"]),
    ("Layer", &["Is", "Is Not"]),
];

/// Whether node `n` meets the phrase (names compare case-insensitively).
pub fn term_matches(state: &AppState, n: &Node, t: &SearchTerm) -> Result<bool, String> {
    let yes = |v: &str| matches!(v.to_ascii_lowercase().as_str(), "yes" | "true" | "on" | "1");
    let text = |actual: &str| -> Result<bool, String> {
        let (a, v) = (actual.to_lowercase(), t.value.to_lowercase());
        Ok(match t.condition.as_str() {
            "Starts With" => a.starts_with(&v),
            "Contains" => a.contains(&v),
            "Ends With" => a.ends_with(&v),
            "Is" => a == v,
            "Is Not" => a != v,
            "Does Not Contain" => !a.contains(&v),
            c => return Err(format!("no condition `{c}` for {}", t.property)),
        })
    };
    let flag = |b: bool| Ok(b == yes(&t.value));
    match t.property.as_str() {
        "Name" => text(&n.name),
        "Type" => text(category(&n.object).label()),
        "Layer" => text(&state.scene.layer(n.layer).map(|l| l.name.clone()).unwrap_or_default()),
        "Hidden" => flag(n.hidden),
        "Frozen" => flag(n.frozen),
        "Renderable" => flag(n.render.renderable),
        "Has Material" => flag(state.scene.node_material(n.id).is_some()),
        "Faces" => {
            let Cell::Number(f) = cell(state, n, Column::Faces) else { return Ok(false) };
            let v: usize = t.value.trim().parse().map_err(|_| format!("Faces needs a number, not `{}`", t.value))?;
            Ok(match t.condition.as_str() {
                "Is Greater Than" => f > v,
                "Is Less Than" => f < v,
                "Is" => f == v,
                c => return Err(format!("no condition `{c}` for Faces")),
            })
        }
        p => Err(format!("no search property `{p}`")),
    }
}

/// Advanced Search: the listed objects meeting all (or any) of the terms, in scene order.
pub fn search(state: &AppState, ex: &SceneExplorer, terms: &[SearchTerm], match_all: bool) -> Result<Vec<NodeId>, String> {
    let mut out = Vec::new();
    for n in state.scene.nodes().filter(|n| listed(state, ex, n)) {
        let mut hits = Vec::with_capacity(terms.len());
        for t in terms {
            hits.push(term_matches(state, n, t)?);
        }
        let ok = if match_all { hits.iter().all(|h| *h) } else { hits.iter().any(|h| *h) };
        if ok && !terms.is_empty() {
            out.push(n.id);
        }
    }
    Ok(out)
}

/// The names the Scene Explorer Option command takes besides the type filters.
pub const OPTIONS: &[&str] = &[
    "Frozen", "Hidden", "Display All", "Display None", "Invert Display", "Display Children", "Sync Selection", "Select Children",
    "Find Case Sensitive", "Find Using Wildcards", "Find Using Regular Expressions", "Lock Cell Editing",
];

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(
        c,
        C::NewSceneExplorer
            | C::OpenSceneExplorer { .. }
            | C::CloseSceneExplorer { .. }
            | C::SceneExplorerOption { .. }
            | C::SceneExplorerColumns { .. }
            | C::SceneExplorerSort { .. }
            | C::SceneExplorerSearch { .. }
            | C::RenameSceneExplorer { .. }
            | C::DeleteSceneExplorer { .. }
            | C::SetDefaultSceneExplorer { .. }
            | C::ManageSceneExplorer
    )
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    let ex = &mut state.explorers;
    match cmd {
        C::NewSceneExplorer => {
            let e = ex.create();
            Ok(json!({ "name": e.name }))
        }
        C::OpenSceneExplorer { name } => {
            if name.is_none() && ex.index(None).is_none() {
                let e = ex.create();
                return Ok(json!({ "name": e.name }));
            }
            let e = ex.get_mut(name.as_deref())?;
            e.open = true;
            let n = e.name.clone();
            ex.last = Some(n.clone());
            Ok(json!({ "name": n }))
        }
        C::CloseSceneExplorer { name } => {
            ex.get_mut(name.as_deref())?.open = false;
            Ok(json!({}))
        }
        C::SceneExplorerOption { explorer, option, on } => {
            let selection: BTreeSet<u64> = state.selection.iter().map(|n| n.0).collect();
            let e = state.explorers.get_mut(explorer.as_deref())?;
            if let Some(c) = Category::ALL.into_iter().find(|c| c.filter_label().eq_ignore_ascii_case(&option) || c.label().eq_ignore_ascii_case(&option)) {
                if on {
                    e.shown.insert(c);
                } else {
                    e.shown.remove(&c);
                }
                return Ok(json!({}));
            }
            match option.as_str() {
                "Frozen" => e.show_frozen = on,
                "Hidden" => e.show_hidden = on,
                "Display All" => {
                    e.shown = Category::ALL.into_iter().collect();
                    e.show_frozen = true;
                    e.show_hidden = true;
                }
                "Display None" => {
                    e.shown.clear();
                    e.show_frozen = false;
                    e.show_hidden = false;
                }
                "Invert Display" => {
                    e.shown = Category::ALL.into_iter().filter(|c| !e.shown.contains(c)).collect();
                    e.show_frozen = !e.show_frozen;
                    e.show_hidden = !e.show_hidden;
                }
                "Display Children" => e.display_children = on,
                "Sync Selection" => {
                    // Turning it off keeps what was highlighted as the explorer's own highlight.
                    if e.sync_selection && !on {
                        e.highlighted = selection;
                    }
                    e.sync_selection = on;
                }
                "Select Children" => e.select_children = on,
                "Find Case Sensitive" => e.find_case_sensitive = on,
                "Find Using Wildcards" => {
                    e.find_wildcards = on;
                    if on {
                        e.find_regex = false;
                    }
                }
                "Find Using Regular Expressions" => {
                    e.find_regex = on;
                    if on {
                        e.find_wildcards = false;
                    }
                }
                "Lock Cell Editing" => e.lock_cell_editing = on,
                o => return Err(format!("no Scene Explorer option `{o}`")),
            }
            Ok(json!({}))
        }
        C::SceneExplorerColumns { explorer, columns } => {
            let mut cols = Vec::new();
            for c in &columns {
                let col = Column::from_label(c).ok_or_else(|| format!("no Scene Explorer column `{c}`"))?;
                if !cols.contains(&col) {
                    cols.push(col);
                }
            }
            if cols.is_empty() {
                return Err("a Scene Explorer keeps at least one column".into());
            }
            let e = ex.get_mut(explorer.as_deref())?;
            e.sort.retain(|(c, _)| cols.contains(c));
            e.columns = cols;
            Ok(json!({}))
        }
        C::SceneExplorerSort { explorer, column, ascending, add } => {
            let col = Column::from_label(&column).ok_or_else(|| format!("no Scene Explorer column `{column}`"))?;
            let e = ex.get_mut(explorer.as_deref())?;
            if add.unwrap_or(false) {
                if let Some(level) = e.sort.iter_mut().find(|(c, _)| *c == col) {
                    level.1 = ascending;
                } else {
                    e.sort.push((col, ascending));
                }
            } else {
                e.sort = vec![(col, ascending)];
            }
            Ok(json!({}))
        }
        C::SceneExplorerSearch { explorer, terms, match_all } => {
            let i = ex.index(explorer.as_deref()).ok_or("no Scene Explorer")?;
            let found = search(state, &state.explorers.list[i], &terms, match_all)?;
            let ids: Vec<u64> = found.iter().map(|n| n.0).collect();
            if state.explorers.list[i].sync_selection {
                crate::execute::execute(state, Command::SelectNodes { ids: ids.clone(), op: SelectOp::Replace })?;
            } else {
                state.explorers.list[i].highlighted = ids.iter().copied().collect();
            }
            Ok(json!({ "found": ids }))
        }
        C::RenameSceneExplorer { explorer, name } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("a Scene Explorer needs a name".into());
            }
            let i = ex.index(explorer.as_deref()).ok_or("no Scene Explorer")?;
            if ex.list.iter().enumerate().any(|(j, e)| j != i && e.name == name) {
                return Err(format!("a Scene Explorer is already called `{name}`"));
            }
            let old = std::mem::replace(&mut ex.list[i].name, name.clone());
            if ex.last.as_deref() == Some(old.as_str()) || explorer.is_none() {
                ex.last = Some(name);
            }
            Ok(json!({}))
        }
        C::DeleteSceneExplorer { explorer } => {
            let i = ex.index(Some(&explorer)).ok_or_else(|| format!("no Scene Explorer `{explorer}`"))?;
            ex.list.remove(i);
            if ex.last.as_deref() == Some(explorer.as_str()) {
                ex.last = ex.list.last().map(|e| e.name.clone());
            }
            Ok(json!({}))
        }
        C::SetDefaultSceneExplorer { explorer } => {
            let mut e = ex.get(Some(&explorer)).cloned().ok_or_else(|| format!("no Scene Explorer `{explorer}`"))?;
            e.open = false;
            e.name.clear();
            e.collapsed.clear();
            ex.default = Some(e);
            Ok(json!({}))
        }
        C::ManageSceneExplorer => {
            state.dialog = Some(crate::Dialog::ManageSceneExplorer);
            Ok(json!({}))
        }
        _ => Err("not a Scene Explorer command".into()),
    }
}
