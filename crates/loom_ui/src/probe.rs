//! Where labelled widgets were drawn in the last pass, so the headless UI
//! tests can click them by label. Compiled out of the app.

use eframe::egui;

#[cfg(test)]
fn id() -> egui::Id {
    egui::Id::new("loom_ui_probe")
}

/// Records `response`'s rect under `label`.
#[cfg(test)]
pub(crate) fn note(ui: &egui::Ui, label: &str, response: &egui::Response) {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| {
        let (at, list) = d.get_temp_mut_or_default::<(u64, Vec<(String, egui::Rect)>)>(id());
        if *at != pass {
            *at = pass;
            list.clear();
        }
        list.push((label.to_owned(), response.rect));
    });
}

#[cfg(not(test))]
#[inline(always)]
pub(crate) fn note(_ui: &egui::Ui, _label: &str, _response: &egui::Response) {}

/// The widgets noted in the last pass.
#[cfg(test)]
pub(crate) fn take(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    let last = ctx.cumulative_pass_nr().saturating_sub(1);
    ctx.data(|d| d.get_temp::<(u64, Vec<(String, egui::Rect)>)>(id())).filter(|(at, _)| *at == last).map(|(_, l)| l).unwrap_or_default()
}
