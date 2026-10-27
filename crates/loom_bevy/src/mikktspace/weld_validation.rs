//! Test-only previous-partition proof. Never substitutes a rest-pose attribute.
use super::MeshData;
use std::cell::RefCell;
use std::collections::{HashMap,HashSet};

#[derive(Default)]
pub(crate) struct WeldValidation {
    pub enabled: bool,
    pub hits: usize,
    pub misses: usize,
    pub bit_rejects: usize,
    pub merge_rejects: usize,
    pub nonfinite_rejects: usize,
    entry: Option<Entry>,
}
struct Input {
    indices: Vec<u32>, uvs: Vec<[f32;2]>, dimensions: [usize;3], nondegenerate: bool,
}
struct Entry {
    input: Input, welded: Vec<i32>, classes: Vec<usize>, representatives: Vec<usize>, uv_buckets: Vec<Vec<usize>>, manifold: bool,
}
struct Active { cache: WeldValidation, proven: bool, input: Option<Input> }
thread_local! { static STACK: RefCell<Vec<Option<Active>>> = const { RefCell::new(Vec::new()) }; }

fn nondegenerate(data:&MeshData)->bool {
    data.indices.chunks_exact(3).all(|t| {
        let(a,b,c)=(data.positions[t[0] as usize],data.positions[t[1] as usize],data.positions[t[2] as usize]);
        a!=b&&a!=c&&b!=c
    })
}
fn bits(data:&MeshData,index:usize)->[u32;8] {
    let(p,n,u)=(data.positions[index],data.normals[index],data.uvs[index]);
    [p[0].to_bits(),p[1].to_bits(),p[2].to_bits(),n[0].to_bits(),n[1].to_bits(),n[2].to_bits(),u[0].to_bits(),u[1].to_bits()]
}
fn zero(bits:u32)->u32 {if bits&0x7fff_ffff==0 {0}else{bits}}
fn equality_key(bits:[u32;8])->[u32;6] {[zero(bits[0]),zero(bits[1]),zero(bits[2]),zero(bits[3]),zero(bits[4]),zero(bits[5])]}

impl Entry {
    fn build(input:Input,welded:&[i32])->Self {
        let mut labels=HashMap::new();let mut representatives=Vec::new();
        let classes=welded.iter().map(|encoded| {
            let next=labels.len();
            *labels.entry(*encoded).or_insert_with(|| {
                let linear=(*encoded as usize>>2)*3+(*encoded as usize&3);
                representatives.push(input.indices[linear] as usize);next
            })
        }).collect();
        let mut buckets=HashMap::<[u32;2],Vec<usize>>::new();
        for (class,index) in representatives.iter().enumerate() {
            buckets.entry(input.uvs[*index].map(|v|zero(v.to_bits()))).or_default().push(class);
        }
        let mut edges=HashMap::new();
        for t in welded.chunks_exact(3) {for e in 0..3 {let(a,b)=(t[e],t[(e+1)%3]);*edges.entry((a.min(b),a.max(b))).or_insert(0usize)+=1;}}
        let manifold=welded.len()==input.indices.len()&&edges.values().all(|count|*count<=2);
        Self{input,welded:welded.to_vec(),classes,representatives,uv_buckets:buckets.into_values().collect(),manifold}
    }
    // Rejection reasons: 0 dependency/eligibility, 1 exact member bits,
    // 2 newly equal classes, 3 nonfinite attributes.
    fn validate(&self,data:&MeshData)->Result<(),u8> {
        if !self.manifold||!self.input.nondegenerate||self.input.indices!=data.indices
            ||self.input.dimensions!=[data.positions.len(),data.normals.len(),data.uvs.len()]
            ||bytemuck::cast_slice::<_,u8>(&self.input.uvs)!=bytemuck::cast_slice::<_,u8>(&data.uvs)
            ||!nondegenerate(data) {return Err(0);}
        let mut values=Vec::with_capacity(self.representatives.len());
        for index in &self.representatives {
            let value=bits(data,*index);
            if value.iter().any(|v|!f32::from_bits(*v).is_finite()) {return Err(3);}
            values.push(value);
        }
        for (corner,index) in data.indices.iter().enumerate() {
            if bits(data,*index as usize)!=values[self.classes[corner]] {return Err(1);}
        }
        // UVs are unchanged bit-for-bit. Distinct-class comparisons therefore
        // need only occur inside the original float-equal UV bucket.
        let mut distinct=HashSet::new();
        for bucket in &self.uv_buckets {
            if bucket.len()<=8 {
                for i in 0..bucket.len() {for j in i+1..bucket.len() {
                    if equality_key(values[bucket[i]])==equality_key(values[bucket[j]]) {return Err(2);}
                }}
            } else {
                distinct.clear();distinct.reserve(bucket.len());
                for class in bucket {if !distinct.insert(equality_key(values[*class])) {return Err(2);}}
            }
        }
        Ok(())
    }
}

pub(super) struct Scope { depth:usize,closed:bool }
pub(super) fn begin(data:&MeshData,cache:Option<WeldValidation>)->Scope {
    let _timer=loom_render::profile::Scope::new("mikk.weld_validation");
    let active=cache.map(|mut cache| {
        let result=cache.entry.as_ref().map_or(Err(0),|entry|entry.validate(data));
        match result {Err(1)=>cache.bit_rejects+=1,Err(2)=>cache.merge_rejects+=1,Err(3)=>cache.nonfinite_rejects+=1,_=>{}}
        let proven=result.is_ok();
        let input=(!proven).then(||Input{indices:data.indices.clone(),uvs:data.uvs.clone(),dimensions:[data.positions.len(),data.normals.len(),data.uvs.len()],nondegenerate:nondegenerate(data)});
        Active{cache,proven,input}
    });
    let depth=STACK.with(|s| {let mut s=s.borrow_mut();let depth=s.len();s.push(active);depth});
    Scope{depth,closed:false}
}
impl Scope {
    pub(super) fn finish(mut self)->Option<WeldValidation> {
        let active=STACK.with(|s| {let mut s=s.borrow_mut();assert_eq!(s.len(),self.depth+1);s.pop().unwrap()});
        self.closed=true;active.map(|a|a.cache)
    }
}
impl Drop for Scope {fn drop(&mut self) {if !self.closed {STACK.with(|s|s.borrow_mut().truncate(self.depth));}}}

pub(super) fn restore(indices:&mut [i32])->bool {
    STACK.with(|s| {
        let mut stack=s.borrow_mut();let Some(Some(active))=stack.last_mut() else{return false;};
        if active.proven {
            let entry=active.cache.entry.as_ref().unwrap();assert_eq!(indices.len(),entry.welded.len());
            indices.copy_from_slice(&entry.welded);active.cache.hits+=1;true
        } else {active.cache.misses+=1;false}
    })
}
pub(super) fn record(indices:&[i32]) {
    STACK.with(|s| {
        let mut stack=s.borrow_mut();let Some(Some(active))=stack.last_mut() else{return;};
        if let Some(input)=active.input.take() {active.cache.entry=Some(Entry::build(input,indices));}
    });
}
