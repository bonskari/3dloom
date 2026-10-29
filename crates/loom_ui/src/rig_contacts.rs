//! Contact authoring belongs to the separately opened Animation Editor.
use crate::{loomrig_panel::run, LoomApp};
use eframe::egui;
use glam::{Mat4, Quat, Vec3};
use loom_commands::Command;
use loom_scene::{loomrig::{LoomRig,RigLimb},loomrig_contact::{ContactReference,ContactSpace,ContactTrack}};

pub(crate) fn show(ui:&mut egui::Ui, app:&mut LoomApp, rig:&LoomRig, limb:&RigLimb) {
    ui.push_id(("contacts",app.state.scene.instance_id(),rig.id,&limb.mapping.name),|ui| {
        ui.horizontal(|ui| {
            for (label,preset) in [("Planted","planted"),("Sliding","sliding"),("Free","free")] {
                if ui.button(label).on_hover_text("Capture the current limb pose at this frame; replace this frame's contact preset if it already has a key.").clicked() {
                    run(app,Command::SetLoomRigContactPreset {rig:rig.id,limb:limb.mapping.name.clone(),time:app.state.current_frame as f32,preset:preset.into()});
                }
            }
        });
        egui::CollapsingHeader::new("Contact Keys").default_open(true).show(ui,|ui| {
            if limb.contacts.keys().is_empty() {
                ui.label(if limb.contacts.constant_state().is_some() {"Constant contact"} else {"No contact keys"});
            }
            egui::ScrollArea::vertical().max_height(150.).show_rows(ui,24.,limb.contacts.keys().len(),|ui,range| {
                for index in range {
                    let old = &limb.contacts.keys()[index];
                    ui.push_id(old.time.to_bits(),|ui|ui.horizontal(|ui| {
                        if ui.button(format!("Frame {}",old.time)).clicked() {run(app,Command::SetFrame {frame:old.time.round() as i32});}
                        let mut key = old.clone();
                        let mut changed = ui.add(egui::Slider::new(&mut key.blend,0.0..=1.0).text("IK")).changed();
                        egui::ComboBox::from_id_salt("anchor").selected_text(format!("Anchor {}",key.reference)).show_ui(ui,|ui| {
                            for id in limb.contacts.references().keys() {
                                changed |= ui.selectable_value(&mut key.reference,*id,format!("Anchor {id}")).changed();
                            }
                        });
                        if changed {run(app,Command::EditLoomRigContactKey {rig:rig.id,limb:limb.mapping.name.clone(),key});}
                        if ui.button("Delete Contact Key").clicked() {run(app,Command::DeleteLoomRigContactKey {rig:rig.id,limb:limb.mapping.name.clone(),time:old.time});}
                    }));
                }
            });
        });
        egui::CollapsingHeader::new("Contact Anchors").show(ui,|ui| {
            let Some((&first,_)) = limb.contacts.references().first_key_value() else {ui.label("No contact anchors");return};
            let id = ui.id().with("selected_anchor");
            let frame = app.state.current_frame;
            let active = limb.contacts.active_key(frame as f32).expect("finite editor frame").map(|key|key.reference)
                .or_else(||limb.contacts.constant_state().map(|state|state.reference)).unwrap_or(first);
            let mut selected = ui.ctx().data_mut(|d|d.get_temp::<(i32,u64,u64)>(id))
                .filter(|(old_frame,old_active,_)|*old_frame == frame && *old_active == active).map(|(_,_,selected)|selected).unwrap_or(active);
            if !limb.contacts.references().contains_key(&selected) {selected=active;}
            egui::ComboBox::from_id_salt("edit_anchor").selected_text(format!("Anchor {selected}")).show_ui(ui,|ui| {
                for reference in limb.contacts.references().keys() {ui.selectable_value(&mut selected,*reference,format!("Anchor {reference}"));}
            });
            ui.ctx().data_mut(|d|d.insert_temp(id,(frame,active,selected)));
            let old = &limb.contacts.references()[&selected];
            let mut pose = old.clone();
            egui::ComboBox::from_id_salt("anchor_space").selected_text(space_name(&pose.space)).show_ui(ui,|ui| {
                for space in [ContactSpace::World,ContactSpace::Body] {let name=space_name(&space);ui.selectable_value(&mut pose.space,space,name);}
            });
            if pose.space != old.space {
                let frame = app.state.scene.world_matrix(rig.mapping.root);
                match change_space(old,pose.space,frame) {
                    Ok(pose)=>run(app,Command::EditLoomRigContactReference {rig:rig.id,limb:limb.mapping.name.clone(),reference:selected,pose}),
                    Err(error)=>app.state.status_prompt=error,
                }
                return;
            }
            let mut changed = vector(ui,"Anchor position",&mut pose.position);
            let (x,y,z) = pose.rotation.to_euler(glam::EulerRot::XYZ);
            let mut rotation = Vec3::new(x.to_degrees(),y.to_degrees(),z.to_degrees());
            if vector(ui,"Anchor rotation",&mut rotation) {
                pose.rotation = Quat::from_euler(glam::EulerRot::XYZ,rotation.x.to_radians(),rotation.y.to_radians(),rotation.z.to_radians()).normalize();
                changed=true;
            }
            changed |= vector(ui,"Pole position",&mut pose.pole_position);
            if changed {run(app,Command::EditLoomRigContactReference {rig:rig.id,limb:limb.mapping.name.clone(),reference:selected,pose});}
            ui.small("Joined keys share their anchor. Changing its space keeps its current world pose.");
        });
    });
}

fn space_name(space:&ContactSpace) -> &'static str {match space {ContactSpace::World=>"World",ContactSpace::Body=>"Body"}}

fn change_space(reference:&ContactReference,space:ContactSpace,frame:Mat4) -> Result<ContactReference,String> {
    let sample = ContactTrack::constant(reference.clone(),1.)?.sample(0.,frame)?.ok_or("contact anchor is missing")?;
    ContactReference::capture(space,frame,Mat4::from_rotation_translation(sample.rotation,sample.position),sample.pole_position)
}

fn vector(ui:&mut egui::Ui,label:&str,value:&mut Vec3) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut changed=false;
        for (index,prefix) in ["X ","Y ","Z "].into_iter().enumerate() {
            changed |= ui.add(egui::DragValue::new(&mut value[index]).prefix(prefix).speed(0.1)).changed();
        }
        changed
    }).inner
}
