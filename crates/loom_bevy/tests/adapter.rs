//! The GPU adapter: with a discrete GPU present it is always the one picked,
//! never an integrated GPU beside it.

#[test]
fn the_discrete_gpu_is_picked_when_present() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
    let all: Vec<_> = instance.enumerate_adapters(wgpu::Backends::PRIMARY).into_iter().map(|a| a.get_info()).collect();
    for a in &all {
        eprintln!("adapter present: {} ({:?}, {:?})", a.name, a.device_type, a.backend);
    }
    let Some(adapter) = pollster::block_on(instance.request_adapter(&loom_bevy::ADAPTER_OPTIONS)) else {
        eprintln!("no GPU adapter, skipped");
        return;
    };
    let picked = adapter.get_info();
    eprintln!("adapter picked: {} ({:?})", picked.name, picked.device_type);
    if all.iter().any(|a| a.device_type == wgpu::DeviceType::DiscreteGpu) {
        assert_eq!(picked.device_type, wgpu::DeviceType::DiscreteGpu, "picked {}", picked.name);
    }
}
