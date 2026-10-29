//! Character controls over existing skeletons; edits dispatch native commands.
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};
use loom_scene::{
    loomrig::{HumanoidMapping, LimbMapping},
    NodeId,
};

struct BoneCache {scene:u64,revision:u64,bones:Vec<(NodeId,String)>,roots:Vec<(NodeId,String)>}
#[derive(Clone)]
struct Detection {scene:u64,revision:u64,root:NodeId,mapping:Result<HumanoidMapping,String>}

#[derive(Clone, Default)]
struct Draft {
    root: Option<NodeId>,
    pelvis: Option<NodeId>,
    head: Option<NodeId>,
    spine: Vec<Option<NodeId>>,
    limbs: Vec<(String, [Option<NodeId>; 3])>,
}
impl Draft {
    fn from_mapping(m: &HumanoidMapping) -> Self {
        Self {
            root: Some(m.root),
            pelvis: Some(m.pelvis),
            head: Some(m.head),
            spine: m.spine.iter().map(|n| Some(*n)).collect(),
            limbs: m
                .limbs
                .iter()
                .map(|l| (l.name.clone(), [Some(l.start), Some(l.middle), Some(l.end)]))
                .collect(),
        }
    }
    fn empty(root: NodeId) -> Self {
        Self {
            root: Some(root),
            spine: vec![None],
            limbs: ["Left Arm", "Right Arm", "Left Leg", "Right Leg"]
                .into_iter()
                .map(|n| (n.into(), [None; 3]))
                .collect(),
            ..Default::default()
        }
    }
    fn mapping(&self) -> Result<HumanoidMapping, String> {
        Ok(HumanoidMapping {
            root: self.root.ok_or("Choose root")?,
            pelvis: self.pelvis.ok_or("Choose pelvis")?,
            head: self.head.ok_or("Choose head")?,
            spine: self
                .spine
                .iter()
                .map(|n| n.ok_or("Map all spine joints"))
                .collect::<Result<_, _>>()?,
            limbs: self
                .limbs
                .iter()
                .map(|(name, n)| {
                    Ok(LimbMapping {
                        name: name.clone(),
                        start: n[0].ok_or("Map limb start")?,
                        middle: n[1].ok_or("Map limb middle")?,
                        end: n[2].ok_or("Map limb end")?,
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}
fn run(app: &mut LoomApp, command: Command) {
    if let Err(e) = app.dispatch(command) {
        app.state.status_prompt = e;
    }
}
fn select(app: &mut LoomApp, id: NodeId) {
    run(
        app,
        Command::SelectNodes {
            ids: vec![id.0],
            op: SelectOp::Replace,
        },
    );
}
fn bone_picker(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<NodeId>,
    bones: &[(NodeId, String)],
) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .width(125.)
            .selected_text(
                value
                    .and_then(|id| bones.iter().find(|b| b.0 == id))
                    .map(|b| b.1.as_str())
                    .unwrap_or("Unassigned"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(value, None, "Unassigned");
                for (id, name) in bones {
                    ui.selectable_value(value, Some(*id), name);
                }
            });
    });
}
fn mapping_editor(ui: &mut egui::Ui, draft: &mut Draft, bones: &[(NodeId, String)]) {
    bone_picker(ui, "Root", &mut draft.root, bones);
    bone_picker(ui, "Pelvis", &mut draft.pelvis, bones);
    bone_picker(ui, "Head", &mut draft.head, bones);
    for (i, bone) in draft.spine.iter_mut().enumerate() {
        bone_picker(ui, &format!("Spine {}", i + 1), bone, bones);
    }
    ui.horizontal(|ui| {
        if ui.small_button("Add spine joint").clicked() {
            draft.spine.push(None);
        }
        if ui
            .add_enabled(
                draft.spine.len() > 1,
                egui::Button::new("Remove last").small(),
            )
            .clicked()
        {
            draft.spine.pop();
        }
    });
    let mut remove=None;
    for (index,(name, joints)) in draft.limbs.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.text_edit_singleline(name);
            if ui.small_button("Remove chain").clicked(){remove=Some(index);}
            for (label, joint) in ["Start", "Middle", "End"].into_iter().zip(joints) {
                bone_picker(ui, label, joint, bones);
            }
        });
    }
    if let Some(index)=remove{draft.limbs.remove(index);}
    if ui.button("Add limb chain").clicked(){draft.limbs.push((format!("Chain {}",draft.limbs.len()+1),[None;3]));}
}

pub(crate) fn show(ui: &mut egui::Ui, app: &mut LoomApp) {
    let scene_id = app.state.scene.instance_id();
    let revision=app.state.scene.revision();
    let key=egui::Id::new("loomrig_bone_cache");
    let cached=ui.ctx().data(|d|d.get_temp::<std::sync::Arc<BoneCache>>(key));
    let cache=match cached.filter(|c|c.scene==scene_id&&c.revision==revision) {
        Some(cache)=>cache,
        None=>{
            let mut bones:Vec<_>=app.state.scene.nodes().filter(|n|n.object.is_bone()).map(|n|(n.id,n.name.clone())).collect();
            bones.sort_by(|a,b|a.1.cmp(&b.1));
            let roots=bones.iter().filter(|(id,_)|app.state.scene.node(*id).and_then(|n|n.parent).and_then(|p|app.state.scene.node(p)).is_none_or(|p|!p.object.is_bone())).cloned().collect();
            let cache=std::sync::Arc::new(BoneCache{scene:scene_id,revision,bones,roots});
            ui.ctx().data_mut(|d|d.insert_temp(key,cache.clone()));cache
        }
    };
    let bones=&cache.bones;
    if bones.is_empty(){return;}
    let rigs = app.state.scene.loom_rigs.clone();
    egui::CollapsingHeader::new("LoomRig").default_open(true).show(ui,|ui| {
        egui::CollapsingHeader::new("Create Character Rig").default_open(rigs.is_empty()).show(ui,|ui| {
            let candidates=&cache.roots;
            let key=egui::Id::new(("loomrig_root",scene_id));
            let selected=app.state.selection.iter().find_map(|id|candidates.iter().find(|(root,_)|*root==*id||app.state.scene.ancestors(*id).contains(root)).map(|(root,_)|*root));
            let mut root=ui.ctx().data_mut(|d|*d.get_temp_mut_or_insert_with(key,||selected.or_else(||candidates.first().map(|c|c.0))));
            let selection_key=egui::Id::new(("loomrig_root_selection",scene_id));
            let previous=ui.ctx().data(|d|d.get_temp::<Option<NodeId>>(selection_key)).flatten();
            if selected!=previous&&selected.is_some(){root=selected;}
            if root.is_some_and(|id|!candidates.iter().any(|c|c.0==id)){root=candidates.first().map(|c|c.0);}
            ui.ctx().data_mut(|d|d.insert_temp(selection_key,selected));
            bone_picker(ui,"Character root",&mut root,&candidates);
            ui.ctx().data_mut(|d|d.insert_temp(key,root));
            if let Some(root)=root {
                let key=egui::Id::new("loomrig_detection");
                let memo=ui.ctx().data(|d|d.get_temp::<Detection>(key));
                let detected=match memo.filter(|m|m.scene==scene_id&&m.revision==revision&&m.root==root) {
                    Some(m)=>m.mapping,
                    None=>{let mapping=HumanoidMapping::metahuman(&app.state.scene,root);ui.ctx().data_mut(|d|d.insert_temp(key,Detection{scene:scene_id,revision,root,mapping:mapping.clone()}));mapping}
                };
                if ui.add_enabled(detected.is_ok(),egui::Button::new("Create LoomRig")).clicked(){run(app,Command::CreateLoomRig{root:root.0,mapping:None});}
                if detected.is_err(){ui.small("Map this skeleton's joints below.");}
                egui::CollapsingHeader::new("Characterization").show(ui,|ui| {
                    let key=egui::Id::new(("loomrig_draft",scene_id,root));
                    let mut draft=ui.ctx().data_mut(|d|d.get_temp_mut_or_insert_with(key,||detected.as_ref().map(Draft::from_mapping).unwrap_or_else(|_|Draft::empty(root))).clone());
                    let members:Vec<_>=bones.iter().filter(|(id,_)|*id==root||app.state.scene.ancestors(*id).contains(&root)).cloned().collect();
                    mapping_editor(ui,&mut draft,&members);
                    let mapping=draft.mapping().and_then(|m|{m.validate(&app.state.scene)?;Ok(m)});
                    if ui.add_enabled(mapping.is_ok(),egui::Button::new("Create Mapped LoomRig")).clicked(){let mapping=mapping.unwrap();run(app,Command::CreateLoomRig{root:mapping.root.0,mapping:Some(mapping)});}
                    ui.ctx().data_mut(|d|d.insert_temp(key,draft));
                });
            }
        });
        if rigs.is_empty(){return;}
        let key=egui::Id::new(("loomrig_active",scene_id));
        let mut active=ui.ctx().data_mut(|d|*d.get_temp_mut_or_insert_with(key,||rigs[0].id));
        if !rigs.iter().any(|r|r.id==active){active=rigs[0].id;}
        egui::ComboBox::from_id_salt("active_loomrig").selected_text(rigs.iter().find(|r|r.id==active).unwrap().name.clone()).show_ui(ui,|ui|{for r in &rigs {ui.selectable_value(&mut active,r.id,&r.name);}});
        ui.ctx().data_mut(|d|d.insert_temp(key,active));
        let rig=rigs.iter().find(|r|r.id==active).unwrap();
        if ui.button("Select Master").clicked(){select(app,rig.master);}
        retarget_panel(ui,app,rig.mapping.root);
        ui.small("FK controls add keyed corrections to the source motion. Enable Auto Key to animate them.");
        egui::CollapsingHeader::new("FK Controls").show(ui,|ui| {
            for binding in &rig.bindings {
                let name=app.state.scene.node(binding.bone).map(|n|n.name.clone()).unwrap_or_else(||binding.bone.0.to_string());
                if ui.button(name).clicked(){select(app,binding.control);}
            }
        });
        let key=egui::Id::new(("loomrig_limb",scene_id,rig.id));
        let mut index=ui.ctx().data_mut(|d|*d.get_temp_mut_or_insert_with(key,||0usize));
        index=index.min(rig.limbs.len().saturating_sub(1));
        egui::ComboBox::from_id_salt("loomrig_limb").selected_text(rig.limbs.get(index).map(|l|l.mapping.name.as_str()).unwrap_or("No limbs")).show_ui(ui,|ui| {for (i,l) in rig.limbs.iter().enumerate(){ui.selectable_value(&mut index,i,&l.mapping.name);}});
        ui.ctx().data_mut(|d|d.insert_temp(key,index));
        if let Some(limb)=rig.limbs.get(index) {
            ui.horizontal(|ui| {if ui.button("Select Goal").clicked(){select(app,limb.goal);}if ui.button("Select Pole").clicked(){select(app,limb.pole);}});
            ui.label("IK / FK");
            let mut blend=match limb.contacts.sample_blend(app.state.current_frame as f32) {
                Ok(value)=>value.unwrap_or_else(||limb.blend.get()),
                Err(error)=>{ui.colored_label(egui::Color32::RED,error);return;}
            };
            let mut solver=limb.solver;
            let changed=ui.add(egui::Slider::new(&mut blend,0.0..=1.0).text("IK weight")).changed();
            egui::ComboBox::from_id_salt("loomrig_solver").selected_text(solver.label()).show_ui(ui,|ui|{for s in [loom_scene::ik::IkSolver::Limb,loom_scene::ik::IkSolver::HI]{ui.selectable_value(&mut solver,s,s.label());}});
            if changed||solver!=limb.solver {run(app,Command::SetLoomRigLimb{rig:rig.id,limb:limb.mapping.name.clone(),solver:match solver{loom_scene::ik::IkSolver::HI=>"hi",_=>"limb"}.into(),blend});}
            if ui.button("Key IK / FK").clicked(){run(app,Command::SetLoomRigBlendKey{rig:rig.id,limb:limb.mapping.name.clone(),time:app.state.current_frame as f32,value:Some(limb.blend.get())});}
            ui.small("IK goals stay in world space for planted feet. Switching matches the current pose.");
        }
        egui::CollapsingHeader::new("Edit Characterization").show(ui,|ui| {
            let key=egui::Id::new(("loomrig_mapping",scene_id,rig.id));
            let mut draft=ui.ctx().data_mut(|d|d.get_temp_mut_or_insert_with(key,||Draft::from_mapping(&rig.mapping)).clone());
            mapping_editor(ui,&mut draft,&bones);
            let mapping=draft.mapping().and_then(|m|{m.validate(&app.state.scene)?;Ok(m)});
            ui.small("Applying a new mapping rebuilds this rig's controls; the original skeleton and source keys stay intact.");
            if ui.add_enabled(mapping.is_ok(),egui::Button::new("Apply Characterization")).clicked(){run(app,Command::SetLoomRigMapping{rig:rig.id,mapping:mapping.unwrap()});}
            ui.ctx().data_mut(|d|d.insert_temp(key,draft));
        });
    });
}

fn retarget_panel(ui: &mut egui::Ui, app: &mut LoomApp, target: NodeId) {
    let clips: Vec<_> = app
        .state
        .scene
        .animation_clips
        .iter()
        .filter(|c| {
            !c.nodes
                .iter()
                .any(|id| *id == target || app.state.scene.ancestors(*id).contains(&target))
        })
        .cloned()
        .collect();
    if clips.is_empty() {
        ui.small("Import or generate a source motion to apply it to this character.");
        return;
    }
    ui.separator();
    ui.strong("Source Motion");
    let scene = app.state.scene.instance_id();
    let key = egui::Id::new(("retarget_source", scene, target));
    let mut source = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_insert_with(key, || clips[0].id));
    if !clips.iter().any(|c| c.id == source) {
        source = clips[0].id;
    }
    egui::ComboBox::from_id_salt("retarget_source")
        .selected_text(&clips.iter().find(|c| c.id == source).unwrap().name)
        .show_ui(ui, |ui| {
            for c in &clips {
                ui.selectable_value(&mut source, c.id, &c.name);
            }
        });
    ui.ctx().data_mut(|d| d.insert_temp(key, source));
    let key = egui::Id::new(("retarget_mapping", scene, source, target));
    let mut mapping = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(key, || {
            loom_commands::retarget::auto_mapping(&app.state, source, target).unwrap_or_default()
        })
        .clone()
    });
    ui.small(format!("{} mapped body joints", mapping.len()));
    egui::CollapsingHeader::new("Retarget Mapping").show(ui, |ui| {
        let source_bones: Vec<_> = clips
            .iter()
            .find(|c| c.id == source)
            .unwrap()
            .nodes
            .iter()
            .filter_map(|id| {
                app.state
                    .scene
                    .node(*id)
                    .filter(|n| n.object.is_bone())
                    .map(|n| (*id, n.name.clone()))
            })
            .collect();
        let target_bones: Vec<_> = app
            .state
            .scene
            .nodes()
            .filter(|n| {
                n.object.is_bone()
                    && (n.id == target || app.state.scene.ancestors(n.id).contains(&target))
            })
            .map(|n| (n.id, n.name.clone()))
            .collect();
        let mut remove = None;
        for (i, pair) in mapping.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let mut a = Some(pair.source);
                let mut b = Some(pair.target);
                bone_picker(ui, "Source", &mut a, &source_bones);
                bone_picker(ui, "Target", &mut b, &target_bones);
                if let Some(id) = a {
                    pair.source = id;
                }
                if let Some(id) = b {
                    pair.target = id;
                }
                if ui.small_button("Remove pair").clicked() {
                    remove = Some(i);
                }
                ui.separator();
            });
        }
        if let Some(i) = remove {
            mapping.remove(i);
        }
        if ui.button("Add bone pair").clicked() {
            if let (Some(a), Some(b)) = (source_bones.first(), target_bones.first()) {
                mapping.push(loom_commands::retarget::BoneMapping {
                    source: a.0,
                    target: b.0,
                });
            }
        }
        if ui.button("Detect Kimodo Mapping").clicked() {
            match loom_commands::retarget::auto_mapping(&app.state, source, target) {
                Ok(m) => mapping = m,
                Err(e) => app.state.status_prompt = e,
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, mapping.clone()));
    if ui
        .add_enabled(
            !mapping.is_empty(),
            egui::Button::new("Apply Motion to Character"),
        )
        .clicked()
    {
        match app.dispatch(Command::RetargetAnimationClip {
            source_clip: source,
            target_root: target.0,
            mapping: Some(mapping),
        }) {
            Ok(result) => {
                if let Err(e) = crate::clip_timeline::preview_import(app, &result) {
                    app.state.status_prompt = e;
                }
            }
            Err(e) => app.state.status_prompt = e,
        }
    }
}
