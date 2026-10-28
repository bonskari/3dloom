use wgpu::util::DeviceExt;
fn run(device:&wgpu::Device,queue:&wgpu::Queue,body:&str,input:&[[f32;4]],count:usize)->Result<Vec<f64>,String>{
    let shader_source=format!("@group(0) @binding(0) var<storage,read> input:array<vec4<f32>>;\n@group(0) @binding(1) var<storage,read_write> output:array<vec4<f64>>;\n@compute @workgroup_size(1) fn main(){{{body}}}");
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
    {let mut pass=encoder.begin_compute_pass(&Default::default());pass.set_pipeline(&pipeline);pass.set_bind_group(0,&binding,&[]);pass.dispatch_workgroups(1,1,1);}
    encoder.copy_buffer_to_buffer(&output,0,&staging,0,bytes);queue.submit([encoder.finish()]);
    let(tx,rx)=std::sync::mpsc::channel();staging.slice(..).map_async(wgpu::MapMode::Read,move|r|tx.send(r).unwrap());device.poll(wgpu::Maintain::Wait);rx.recv().unwrap().map_err(|e|e.to_string())?;
    let values:Vec<f64>=staging.slice(..).get_mapped_range().chunks_exact(8).map(|v|f64::from_le_bytes(v.try_into().unwrap())).collect();staging.unmap();Ok(values)
}
fn main(){
    let instance=wgpu::Instance::new(&wgpu::InstanceDescriptor{backends:wgpu::Backends::VULKAN,..Default::default()});
    let adapter=pollster::block_on(instance.request_adapter(&Default::default())).expect("actual Vulkan GPU required");
    let info=adapter.get_info();let supported=adapter.features().contains(wgpu::Features::SHADER_F64);
    eprintln!("F64_ADAPTER name={:?} backend={:?} vendor={} device={} advertised={supported}",info.name,info.backend,info.vendor,info.device);
    if !supported{eprintln!("F64_UNSUPPORTED no device feature requested; ordinary backend retained");return;}
    let(device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor{required_features:wgpu::Features::SHADER_F64,..Default::default()},None)).unwrap();
    assert!(device.features().contains(wgpu::Features::SHADER_F64));
    let input=[[1.0,1e-8,2.0,3.0],[1e-6,-1.0,0.2,0.3],[1.0000001192092896,0.9999998807907104,1200.0,1.0]];
    let body="let a=vec4<f64>(input[0]);let b=vec4<f64>(input[2]);output[0]=vec4<f64>(a.x+a.y,a.x*a.y,a.x/a.w,sqrt(a.z));output[1]=vec4<f64>(b.x*b.y,b.x*b.y-1.0,b.z+b.x,b.w);";
    let result=run(&device,&queue,body,&input,8).expect("real f64 basic shader must validate and execute");
    let a=input[0].map(f64::from);let b=input[2].map(f64::from);let expected=[a[0]+a[1],a[0]*a[1],a[0]/a[3],a[2].sqrt(),b[0]*b[1],b[0]*b[1]-1.0,b[2]+b[0],b[3]];
    let mut max=0.0f64;for(actual,expected)in result.iter().zip(expected){let error=(actual-expected).abs();assert!(actual.is_finite()&&error<=1e-13*expected.abs().max(1.0),"basic native f64 error {actual} vs {expected}");max=max.max(error);}
    assert_ne!(result[0],f64::from((input[0][0]+input[0][1])as f32));
    assert_ne!(result[4],f64::from(input[2][0]*input[2][1]));
    eprintln!("F64_BASIC_PASS result={result:?} max_abs_error={max:e} storage_stride_vec4_bytes=32");
    let body="let a=vec4<f64>(input[1]);output[0]=vec4<f64>(atan2(a.x,a.y),atan2(a.z,a.w),atan2(a.w,a.z),atan2(a.y,a.x));";
    match run(&device,&queue,body,&input,4){
        Ok(values)=>{let a=input[1].map(f64::from);let expected=[a[0].atan2(a[1]),a[2].atan2(a[3]),a[3].atan2(a[2]),a[1].atan2(a[0])];let errors:Vec<_>=values.iter().zip(expected).map(|(a,b)|(a-b).abs()).collect();eprintln!("F64_ATAN2_VALUES values={values:?} absolute_errors={errors:?}");if errors.iter().all(|v|v.is_finite()&&*v<1e-12){eprintln!("F64_ATAN2_PRECISION_PASS");}else{eprintln!("F64_ATAN2_PRECISION_INSUFFICIENT native atan2 not approved for precise group math");}}
        Err(error)=>eprintln!("F64_ATAN2_REJECTED {error}"),
    }
}
