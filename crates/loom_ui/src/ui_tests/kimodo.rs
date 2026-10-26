use super::*;

#[test]
fn kimodo_is_available_without_selection_and_exposes_local_api_settings() {
    let mut h = Harness::new();
    h.app.panel_tab = crate::Tab::Motion;
    h.frames(3);
    assert!(!h.has(egui::accesskit::Role::Button, "Generate"),"extra tools must not appear in the main Motion panel");
    h.click_text_anywhere("Animation");h.click_text_anywhere("Animation Editor...");h.frames(3);
    h.click_text_anywhere("Kimodo");h.frames(3);
    assert!(h.has(egui::accesskit::Role::Button, "Generate"));
    h.click_text_anywhere("Settings");
    h.frames(3);
    assert!(h.has(egui::accesskit::Role::Label, "API URL"));
    assert!(h.has(egui::accesskit::Role::Button, "Test Connection"));
    assert!(h.app.state.prefs.kimodo_api_url.is_empty());
}
