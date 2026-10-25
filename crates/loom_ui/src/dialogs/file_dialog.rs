//! File dialogs (Open / Save As / Save Copy As / Import / Export / Merge…).
//! A person at the desktop gets the system dialog (xdg-desktop-portal, the
//! KDE one under Plasma). A remote-controlled app (agents and tests run with
//! LOOM_CONTROL_PORT on the virtual display) keeps the in-app dialog: a
//! directory listing, a file-name field and OK, which they can drive, and a
//! portal dialog would open on the person's screen instead.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, Dialog};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct St {
    dir: PathBuf,
    /// The "Look in" field while it is being typed (Enter applies it).
    dir_text: String,
    name: String,
}

/// A system dialog running on its own thread; the inner Option fills in when it returns
/// (None inside = cancelled).
#[derive(Clone, Default)]
struct Native(std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>>);

/// Whether this run uses the system file dialogs: yes unless the app is
/// remote-controlled (`LOOM_CONTROL_PORT` set to a port, as in `loom_app`).
pub fn use_native(control_port: Option<&str>) -> bool {
    control_port.and_then(|p| p.trim().parse::<u16>().ok()).is_none_or(|p| p == 0)
}

/// Forgets the dialog's listing state (called when any dialog closes).
pub fn clear_state(d: &mut egui::util::IdTypeMap) {
    d.remove::<St>(egui::Id::new("file_dialog"));
    d.remove::<Native>(egui::Id::new("file_dialog_native"));
}

/// Where the dialog starts and the file name it offers.
fn defaults(app: &LoomApp, dialog: Dialog, ext: &str) -> (PathBuf, String) {
    let name = match dialog {
        Dialog::SaveFileAs | Dialog::SaveFileAsCopy => app.state.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled.loom".into()),
        Dialog::ExportFile | Dialog::ExportSelected | Dialog::ArchiveFile => app.state.file.as_ref().and_then(|f| f.file_stem()).map(|n| format!("{}.{ext}", n.to_string_lossy())).unwrap_or_else(|| format!("Untitled.{ext}")),
        _ => String::new(),
    };
    let dir = app.state.file.as_ref().and_then(|f| f.parent()).filter(|d| d.is_dir()).map(|d| d.to_path_buf()).unwrap_or_else(|| app.file_dir.clone());
    (dir, name)
}

/// The system dialog: started on the first frame, its answer applied when it arrives.
fn show_native(ctx: &egui::Context, app: &mut LoomApp, dialog: Dialog, open: &mut bool) {
    let (title, exts, ok_label) = purpose(dialog);
    let id = egui::Id::new("file_dialog_native");
    let Some(slot) = ctx.data(|d| d.get_temp::<Native>(id)) else {
        let ext_list: Vec<&str> = exts.split(',').collect();
        let (dir, name) = defaults(app, dialog, ext_list[0]);
        let filter = ext_list.iter().map(|x| format!("*.{x}")).collect::<Vec<_>>().join(", ");
        let mut d = rfd::FileDialog::new().set_title(title).add_filter(filter, &ext_list).set_directory(dir);
        let save = ok_label == "Save";
        if save && !name.is_empty() {
            d = d.set_file_name(name);
        }
        let slot = Native::default();
        let out = slot.0.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let picked = if save { d.save_file() } else { d.pick_file() };
            *out.lock().unwrap() = Some(picked);
            repaint.request_repaint();
        });
        ctx.data_mut(|d| d.insert_temp(id, slot));
        return;
    };
    let Some(answer) = slot.0.lock().unwrap().take() else { return };
    ctx.data_mut(|d| d.remove::<Native>(id));
    match answer {
        Some(path) => {
            if apply(ctx, app, dialog, path) {
                *open = false;
            }
        }
        None => *open = false,
    }
}

/// Bitmap formats that load (`loom_commands::render::IMAGE_READ_FORMATS`) and that save (`IMAGE_FORMATS`).
const IMAGES_READ: &str = "png,jpg,jpeg,bmp,tga,tif,tiff,exr,hdr,gif";
const IMAGES_WRITE: &str = "png,jpg,jpeg,bmp,tga,tif,tiff,exr,hdr";

/// Title, extension, "Open"/"Save" button text and the command to run.
fn purpose(dialog: Dialog) -> (&'static str, &'static str, &'static str) {
    match dialog {
        Dialog::OpenFile => ("Open File", "loom", "Open"),
        Dialog::SaveFileAs => ("Save File As", "loom", "Save"),
        Dialog::SaveFileAsCopy => ("Save Copy As", "loom", "Save"),
        Dialog::ImportFile => ("Import File", "fbx,obj,max", "Open"),
        Dialog::ExportFile => ("Export File", "fbx,obj", "Save"),
        Dialog::ExportSelected => ("Export Selected", "fbx,obj", "Save"),
        Dialog::SaveSelected => ("Save Selected", "loom", "Save"),
        Dialog::ArchiveFile => ("Archive", "zip,txt", "Save"),
        Dialog::MergeFile => ("Merge File", "loom,max", "Open"),
        Dialog::PickTexture => ("Select Bitmap Image File", IMAGES_READ, "Open"),
        Dialog::BackgroundFile => ("Select Background Image", IMAGES_READ, "Open"),
        Dialog::MaterialMap { .. } | Dialog::MaterialBitmap => ("Select Bitmap Image File", IMAGES_READ, "Open"),
        Dialog::PickUvTexture => ("Pick Texture", IMAGES_READ, "Open"),
        Dialog::SaveUvw => ("Save UVW", "uvw", "Save"),
        Dialog::RenderFile => ("Render Output File", IMAGES_WRITE, "Save"),
        Dialog::SaveRenderedImage => ("Save Image", IMAGES_WRITE, "Save"),
        Dialog::LoadUvw => ("Load UVW", "uvw", "Open"),
        Dialog::BitmapFit => ("Bitmap Fit", IMAGES_READ, "Open"),
        Dialog::SaveShortcuts => ("Save Shortcut File As", "kbd", "Save"),
        Dialog::LoadShortcuts => ("Load Shortcut File", "kbd", "Open"),
        Dialog::SaveCustomUiScheme => ("Save Custom UI Scheme", "ui", "Save"),
        Dialog::LoadCustomUiScheme => ("Load Custom UI Scheme", "ui", "Open"),
        Dialog::SaveAnimation => ("Save Animation", "lxaf", "Save"),
        Dialog::LoadAnimation => ("Load Animation", "lxaf", "Open"),
        Dialog::PointCacheRecord { .. } => ("Save Points", "pc2", "Save"),
        Dialog::PointCacheLoad { .. } => ("Load Points", "pc2", "Open"),
        _ => ("File", "loom", "OK"),
    }
}

fn command(dialog: Dialog, path: &Path) -> Option<Command> {
    let p = path.to_string_lossy().into_owned();
    Some(match dialog {
        Dialog::OpenFile => Command::OpenFile { path: Some(p) },
        Dialog::SaveFileAs => Command::SaveFileAs { path: Some(p) },
        Dialog::SaveFileAsCopy => Command::SaveFileAsCopy { path: Some(p) },
        Dialog::ImportFile => Command::ImportFile { path: Some(p) },
        Dialog::ExportFile => Command::ExportFile { path: Some(p) },
        Dialog::ExportSelected => Command::ExportSelected { path: Some(p) },
        Dialog::SaveSelected => Command::SaveSelected { path: Some(p) },
        Dialog::ArchiveFile => Command::ArchiveFile { path: Some(p) },
        Dialog::BitmapFit => Command::UvwMapBitmapFit { path: p },
        Dialog::PickUvTexture => Command::PickTexture { path: p },
        Dialog::MaterialMap { channel } => Command::SetMaterialMap {
            slot: None,
            map: loom_scene::material::MAP_CHANNELS.get(channel as usize).map(|(n, _)| (*n).to_owned()).unwrap_or_default(),
            path: Some(p),
            amount: None,
        },
        Dialog::MaterialBitmap => Command::SetMapBitmap { path: p },
        Dialog::SaveUvw => Command::SaveUvw { path: Some(p) },
        Dialog::RenderFile => Command::RenderSetup { width: None, height: None, image_aspect: None, pixel_aspect: None, lock_image_aspect: None, lock_pixel_aspect: None, render_hidden: None, force_two_sided: None, save_file: None, output: Some(p) },
        Dialog::SaveRenderedImage => Command::SaveRenderedImage { path: Some(p) },
        Dialog::LoadUvw => Command::LoadUvw { path: Some(p) },
        Dialog::SaveShortcuts => Command::SaveShortcuts { path: Some(p) },
        Dialog::LoadShortcuts => Command::LoadShortcuts { path: Some(p) },
        Dialog::SaveCustomUiScheme => Command::SaveCustomUiScheme { path: Some(p) },
        Dialog::LoadCustomUiScheme => Command::LoadCustomUiScheme { path: Some(p) },
        Dialog::SaveAnimation => Command::SaveAnimation { path: Some(p) },
        Dialog::LoadAnimation => Command::LoadAnimation { path: Some(p), offset: None },
        Dialog::PointCacheRecord { index, start, end } => Command::PointCacheRecord { index, path: p, start, end },
        Dialog::PointCacheLoad { index } => Command::PointCacheLoad { index, path: p },
        _ => return None,
    })
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, dialog: Dialog, open: &mut bool) {
    if app.native_file_dialogs {
        return show_native(ctx, app, dialog, open);
    }
    let (title, exts, ok_label) = purpose(dialog);
    // "fbx,obj": every listed extension shows; the first is the default for a bare name.
    let ext_list: Vec<&str> = exts.split(',').collect();
    let ext = ext_list[0];
    let id = egui::Id::new("file_dialog");
    let mut st: St = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| {
        let (dir, name) = defaults(app, dialog, ext);
        St { dir_text: dir.to_string_lossy().into_owned(), dir, name }
    });
    let mut done = false;
    let mut chosen: Option<PathBuf> = None;
    egui::Window::new(title).open(open).resizable(true).default_width(520.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Look in:");
            let r = ui.add(egui::TextEdit::singleline(&mut st.dir_text).desired_width(380.0));
            if r.lost_focus() {
                let p = PathBuf::from(st.dir_text.trim());
                if p.is_dir() {
                    st.dir = p;
                }
                st.dir_text = st.dir.to_string_lossy().into_owned();
            }
            if ui.button("Up").clicked() {
                if let Some(parent) = st.dir.parent() {
                    st.dir = parent.to_path_buf();
                    st.dir_text = st.dir.to_string_lossy().into_owned();
                }
            }
        });
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&st.dir) {
            for e in rd.flatten() {
                let p = e.path();
                let hidden = p.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(true);
                if hidden {
                    continue;
                }
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().map(|e| ext_list.iter().any(|x| e.eq_ignore_ascii_case(x))).unwrap_or(false) {
                    files.push(p);
                }
            }
        }
        dirs.sort();
        files.sort();
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for d in &dirs {
                let label = format!("[{}]", d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                let r = ui.selectable_label(false, label);
                if r.double_clicked() {
                    st.dir = d.clone();
                    st.dir_text = st.dir.to_string_lossy().into_owned();
                }
            }
            for f in &files {
                let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let r = ui.selectable_label(st.name == name, &name);
                if r.clicked() {
                    st.name = name.clone();
                }
                if r.double_clicked() {
                    chosen = Some(f.clone());
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("File name:");
            let r = ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(300.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !st.name.trim().is_empty() {
                chosen = Some(st.dir.join(st.name.trim()));
            }
            ui.label(ext_list.iter().map(|x| format!("*.{x}")).collect::<Vec<_>>().join(", "));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!st.name.trim().is_empty(), egui::Button::new(ok_label)).clicked() {
                chosen = Some(st.dir.join(st.name.trim()));
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if let Some(path) = chosen {
        if apply(ctx, app, dialog, path) {
            done = true;
        } else if app.state.dialog != Some(dialog) {
            // Another dialog took over (Merge's object list, the FBX / OBJ options).
            ctx.data_mut(|d| d.remove::<St>(id));
            return;
        }
    }
    if done {
        *open = false;
        ctx.data_mut(|d| d.remove::<St>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}

/// Runs what the chosen file is for. True when the dialog can close; false
/// when it stays (an error went to the prompt) or another dialog took over
/// (Merge shows the file's objects first).
fn apply(ctx: &egui::Context, app: &mut LoomApp, dialog: Dialog, mut path: PathBuf) -> bool {
    let _ = ctx;
    let (_, exts, _) = purpose(dialog);
    let ext = exts.split(',').next().unwrap_or("loom");
    if path.extension().is_none() {
        path.set_extension(ext);
    }
    if let Some(parent) = path.parent() {
        app.file_dir = parent.to_path_buf();
    }
    if dialog == Dialog::BackgroundFile {
        // Back to the Viewport Background dialog with the file in its field.
        app.background_pick = Some(path.to_string_lossy().into_owned());
        app.state.dialog = Some(Dialog::ViewportBackground);
        return true;
    }
    if dialog == Dialog::PickTexture {
        // Show Map in Viewport > Bitmap on the selected object.
        let Some(id) = app.state.selection.iter().next().copied() else { return true };
        return match app.dispatch(Command::SetTextureDisplay { id: id.0, kind: "bitmap".into(), path: Some(path.to_string_lossy().into_owned()) }) {
            Ok(_) => true,
            Err(e) => {
                app.state.status_prompt = e;
                false
            }
        };
    }
    // FBX and OBJ: the format's options dialog first; its OK imports or exports.
    if matches!(dialog, Dialog::ImportFile | Dialog::ExportFile | Dialog::ExportSelected) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let options = match (dialog, ext.as_str()) {
            (Dialog::ImportFile, "fbx") => Some(Dialog::FbxImportOptions),
            (Dialog::ImportFile, "obj") => Some(Dialog::ObjImportOptions),
            (Dialog::ExportFile | Dialog::ExportSelected, "fbx") => Some(Dialog::FbxExportOptions),
            (Dialog::ExportFile | Dialog::ExportSelected, "obj") => Some(Dialog::ObjExportOptions),
            _ => None,
        };
        if let Some(d) = options {
            app.state.exchange_pending = Some((path, dialog == Dialog::ExportSelected));
            app.state.dialog = Some(d);
            return false;
        }
    }
    if dialog == Dialog::MergeFile {
        return match loom_commands::file::read_any(&path) {
            Ok((doc, _)) => {
                let names: Vec<String> = doc.scene.nodes().map(|n| n.name.clone()).collect();
                app.state.merge_pending = Some((path, names));
                app.state.dialog = Some(Dialog::MergeObjects);
                false
            }
            Err(e) => {
                app.state.status_prompt = e;
                false
            }
        };
    }
    match command(dialog, &path).map(|cmd| app.dispatch(cmd)) {
        Some(Ok(_)) | None => true,
        Some(Err(e)) => {
            app.state.status_prompt = e;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::use_native;

    #[test]
    fn system_dialogs_unless_remote_controlled() {
        assert!(use_native(None));
        assert!(use_native(Some("")));
        assert!(use_native(Some("0")));
        assert!(!use_native(Some("7841")));
    }
}
