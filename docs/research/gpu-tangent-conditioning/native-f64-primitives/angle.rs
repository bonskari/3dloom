use wgpu::util::DeviceExt;
fn run(device:&wgpu::Device,queue:&wgpu::Queue,body:&str,input:&[[f64;4]],count:usize)->Result<Vec<f64>,String>{
    let shader_source=format!("{}\n{}",include_str!("angle.wgsl"),format!("@group(0) @binding(0) var<storage,read> input:array<vec4<f64>>;\n@group(0) @binding(1) var<storage,read_write> output:array<vec4<f64>>;\n@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){{let i=id.x;if i>=arrayLength(&input){{return;}}{body}}}"));
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("isolated native f64 capability"),source:wgpu::ShaderSource::Wgsl(shader_source.into())});
    let pipeline=device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:Some("isolated native f64 capability"),layout:None,module:&shader,entry_point:Some("main"),compilation_options:Default::default(),cache:None});
    if let Some(error)=pollster::block_on(device.pop_error_scope()){return Err(error.to_string());}
    let source=device.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:None,contents:bytemuck::cast_slice(input),usage:wgpu::BufferUsages::STORAGE});
    let bytes=count as u64*8;
    let output=device.create_buffer(&wgpu::BufferDescriptor{label:None,size:bytes,usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC,mapped_at_creation:false});
    let staging=device.create_buffer(&wgpu::BufferDescriptor{label:None,size:bytes,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let binding=device.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&pipeline.get_bind_group_layout(0),entries:&[wgpu::BindGroupEntry{binding:0,resource:source.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:output.as_entire_binding()}]});
    let mut encoder=device.create_command_encoder(&Default::default());
    {let mut pass=encoder.begin_compute_pass(&Default::default());pass.set_pipeline(&pipeline);pass.set_bind_group(0,&binding,&[]);pass.dispatch_workgroups((input.len()as u32).div_ceil(64),1,1);}
    encoder.copy_buffer_to_buffer(&output,0,&staging,0,bytes);queue.submit([encoder.finish()]);
    let(tx,rx)=std::sync::mpsc::channel();staging.slice(..).map_async(wgpu::MapMode::Read,move|r|tx.send(r).unwrap());device.poll(wgpu::Maintain::Wait);rx.recv().unwrap().map_err(|e|e.to_string())?;
    let values:Vec<f64>=staging.slice(..).get_mapped_range().chunks_exact(8).map(|v|f64::from_le_bytes(v.try_into().unwrap())).collect();staging.unmap();Ok(values)
}fn main(){
    let instance=wgpu::Instance::new(&wgpu::InstanceDescriptor{backends:wgpu::Backends::VULKAN,..Default::default()});
    let adapter=pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert!(adapter.features().contains(wgpu::Features::SHADER_F64));
    eprintln!("ANGLE_ADAPTER {:?}",adapter.get_info());
    let(device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor{required_features:wgpu::Features::SHADER_F64,..Default::default()},None)).unwrap();
    let mut input=Vec::new();
    for y in 0..=64 {for x in -64..=64 {input.push([y as f64/64.,x as f64/64.,0.,0.]);}}
    for y in [0.0,-0.0,1.0,f64::MIN_POSITIVE,1e-300,1e-100,1e-30,1e-16,1e-12]{for x in [0.0,-0.0,1.0,-1.0,1e-300,-1e-300] {input.push([y,x,0.,0.]);}}
    let t=std::f64::consts::FRAC_PI_8.tan();
    for x in [1.,-1.] {for y in [t-1e-15,t,t+1e-15,1.-1e-15,1.,1.+1e-15]{input.push([y,x,0.,0.]);}}
    let mut state=0x123456789abcdef0u64;
    for _ in 0..32768{state=state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);let y=((state>>11)as f64)/((1u64<<53)as f64);state=state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);let x=2.*((state>>11)as f64)/((1u64<<53)as f64)-1.;input.push([y,x,0.,0.]);}
    let body="output[i]=vec4<f64>(angle_upper(input[i].x,input[i].y),0.0,0.0,0.0);";
    let values=run(&device,&queue,body,&input,input.len()*4).unwrap();
    let(mut max,mut worst)=(0.0f64,0usize);
    for(i,record)in values.chunks_exact(4).enumerate(){let[y,x,_,_]=input[i];let expected=if y==0.&&x==0.{std::f64::consts::FRAC_PI_2}else{y.abs().atan2(x)};let error=(record[0]-expected).abs();assert!(record[0].is_finite()&&record[0]>=0.&&record[0]<=std::f64::consts::PI);if error>max{max=error;worst=i;}assert!(error<=4e-15,"angle mismatch i={i} input={:?} actual={} expected={expected} error={error:e}",input[i],record[0]);}
    let truncation_bound=t.powi(43)/43.0;
    eprintln!("F64_ANGLE_POLYNOMIAL_PASS cases={} max_abs_error={max:e} worst_input={:?} degree41_truncation_bound={truncation_bound:e}",input.len(),input[worst]);
}
