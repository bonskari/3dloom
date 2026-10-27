use super::SkinData;
use std::sync::{Arc,atomic::{AtomicBool,AtomicUsize,Ordering}};
pub(crate) static ENABLED:AtomicBool=AtomicBool::new(true);
pub(crate) static HITS:AtomicUsize=AtomicUsize::new(0);
pub(crate) static MISSES:AtomicUsize=AtomicUsize::new(0);
pub(crate) static GUARD_NS:std::sync::Mutex<std::time::Duration>=std::sync::Mutex::new(std::time::Duration::ZERO);
#[derive(Clone,Debug,Default)]
pub(crate) struct ExplicitWeightCache{snapshot:Option<Snapshot>,effective:Option<Arc<Vec<Vec<(u32,f32)>>>>}
#[derive(Clone,Debug)]
struct Snapshot{weights:Vec<Vec<(u32,u32)>>,explicit:Vec<bool>,rigid:Vec<bool>,rigid_all:bool,limit:u32,bones:usize}
impl Snapshot{
 fn new(data:&SkinData)->Self{Self{weights:data.weights.iter().map(|w|w.iter().map(|(b,w)|(*b,w.to_bits())).collect()).collect(),explicit:data.explicit.clone(),rigid:data.rigid.clone(),rigid_all:data.rigid_all,limit:data.bone_affect_limit,bones:data.bones.len()}}
 fn matches(&self,data:&SkinData)->bool{
  self.bones==data.bones.len()&&self.limit==data.bone_affect_limit&&self.rigid_all==data.rigid_all&&self.explicit==data.explicit&&self.rigid==data.rigid&&self.weights.len()==data.weights.len()&&self.weights.iter().zip(&data.weights).all(|(a,b)|a.len()==b.len()&&a.iter().zip(b).all(|((ab,aw),(bb,bw))|ab==bb&&*aw==bw.to_bits()))
 }
}
impl ExplicitWeightCache{
 pub(crate) fn refresh(&mut self,data:&SkinData,count:usize)->Option<Arc<Vec<Vec<(u32,f32)>>>>{
  let begin=std::time::Instant::now();
  let eligible=data.weights.len()==count&&data.explicit.len()>=count&&data.explicit[..count].iter().all(|v|*v);
  if !eligible{self.snapshot=None;self.effective=None;*GUARD_NS.lock().unwrap()+=begin.elapsed();return None;}
  if self.snapshot.as_ref().is_some_and(|s|s.matches(data)){HITS.fetch_add(1,Ordering::Relaxed);*GUARD_NS.lock().unwrap()+=begin.elapsed();return self.effective.clone();}
  if data.weights.iter().flatten().any(|(_,w)|!w.is_finite()){self.snapshot=None;self.effective=None;*GUARD_NS.lock().unwrap()+=begin.elapsed();return None;}
  *GUARD_NS.lock().unwrap()+=begin.elapsed();MISSES.fetch_add(1,Ordering::Relaxed);
  let effective:Vec<_>=(0..count).map(|i|{let mut w=Vec::new();data.point_weights_into(i,glam::Vec3::ZERO,None,&mut w);w}).collect();
  self.snapshot=Some(Snapshot::new(data));self.effective=Some(Arc::new(effective));self.effective.clone()
 }
}
