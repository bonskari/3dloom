//! Editable Poly > Detach dialog: name, Detach To Element, Detach As Clone.

use crate::LoomApp;
use eframe::egui;
use loom_commands::Command;

#[derive(Clone, Default)]
struct St {
    name: String,
    to_element: bool,
    as_clone: bool,
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp, open: &mut bool) {
    let id = egui::Id::new("detach_dialog");
    let mut st: St = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| St { name: app.state.scene.next_name("Object"), ..Default::default() });
    let mut done = false;
    egui::Window::new("Detach").open(open).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Detach as:");
            ui.text_edit_singleline(&mut st.name);
        });
        ui.checkbox(&mut st.to_element, "Detach To Element");
        ui.checkbox(&mut st.as_clone, "Detach As Clone");
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                let _ = app.dispatch(Command::DetachSelection { name: Some(st.name.clone()), to_element: st.to_element, as_clone: st.as_clone });
                done = true;
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        *open = false;
        ctx.data_mut(|d| d.remove::<St>(id));
    } else {
        ctx.data_mut(|d| d.insert_temp(id, st));
    }
}
