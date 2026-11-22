//! Utilities > Color Clipboard: colour swatches kept for copying from one
//! map or material to another. Board 0 is the utility's own (Utilities
//! panel); New Floater opens another board, gone with its floater. A board
//! saves to and opens from a .ccb file: one "R G B" line (0-255) per swatch.

use crate::{AppState, Command};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Swatches on a board.
pub const SLOTS: usize = 12;

pub type Board = [[u8; 3]; SLOTS];

/// The boards: 0 the utility's, the rest open floaters.
#[derive(Debug, Clone)]
pub struct ColorClipboards {
    pub boards: BTreeMap<u32, Board>,
}

impl Default for ColorClipboards {
    fn default() -> Self {
        ColorClipboards { boards: [(0, [[255, 255, 255]; SLOTS])].into() }
    }
}

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(c, C::ColorClipboardSet { .. } | C::ColorClipboardNewFloater | C::ColorClipboardCloseFloater { .. } | C::ColorClipboardSave { .. } | C::ColorClipboardOpen { .. })
}

/// The .ccb text of a board.
pub fn to_ccb(b: &Board) -> String {
    b.iter().map(|c| format!("{} {} {}\n", c[0], c[1], c[2])).collect()
}

/// A board from .ccb text: its first twelve lines of three numbers.
pub fn from_ccb(text: &str) -> Result<Board, String> {
    let mut b = [[0u8; 3]; SLOTS];
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    for (i, slot) in b.iter_mut().enumerate() {
        let line = lines.next().ok_or_else(|| format!("a color clipboard file has {SLOTS} lines of R G B; line {} is missing", i + 1))?;
        let v: Vec<f32> = line.split_whitespace().map(|x| x.parse::<f32>().map_err(|_| format!("line {}: {x:?} is not a number", i + 1))).collect::<Result<_, _>>()?;
        if v.len() < 3 {
            return Err(format!("line {}: three numbers (R G B) expected", i + 1));
        }
        *slot = [0, 1, 2].map(|k| v[k].round().clamp(0.0, 255.0) as u8);
    }
    Ok(b)
}

fn board_mut(state: &mut AppState, board: u32) -> Result<&mut Board, String> {
    state.color_clipboards.boards.get_mut(&board).ok_or_else(|| format!("no Color Clipboard {board} (0 is the utility's; floaters come from New Floater)"))
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::ColorClipboardSet { board, slot, rgb } => {
            let b = board_mut(state, board)?;
            let s = b.get_mut(slot as usize).ok_or_else(|| format!("no swatch {slot} (0-{})", SLOTS - 1))?;
            *s = rgb;
            Ok(json!({ "board": board, "slot": slot, "rgb": rgb }))
        }
        C::ColorClipboardNewFloater => {
            let id = state.color_clipboards.boards.keys().next_back().map_or(1, |k| k + 1);
            // A floater starts with the utility's swatches.
            let start = state.color_clipboards.boards.get(&0).copied().unwrap_or([[255; 3]; SLOTS]);
            state.color_clipboards.boards.insert(id, start);
            Ok(json!({ "board": id }))
        }
        C::ColorClipboardCloseFloater { board } => {
            if board == 0 {
                return Err("board 0 is the utility's own, not a floater".into());
            }
            state.color_clipboards.boards.remove(&board).ok_or_else(|| format!("no Color Clipboard floater {board}"))?;
            Ok(json!({ "closed": board }))
        }
        C::ColorClipboardSave { board, path } => {
            let b = *board_mut(state, board)?;
            std::fs::write(&path, to_ccb(&b)).map_err(|e| format!("{path}: {e}"))?;
            Ok(json!({ "board": board, "path": path }))
        }
        C::ColorClipboardOpen { board, path } => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let new = from_ccb(&text)?;
            *board_mut(state, board)? = new;
            Ok(json!({ "board": board, "path": path }))
        }
        _ => Err("not a Color Clipboard command".into()),
    }
}
