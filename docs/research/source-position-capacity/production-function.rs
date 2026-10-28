fn source_positions(mesh: &Mesh) -> Result<Vec<[f32; 4]>, String> {
    if mesh.vertex_count() == 0 {
        return Err("GPU runtime requires nonempty source positions".into());
    }
    let mut values = Vec::with_capacity(mesh.vertex_count());
    for v in mesh.vertices() {
        if !v.position.is_finite() {
            return Err("nonfinite current world position".into());
        }
        values.push([v.position.x, v.position.y, v.position.z, 1.0]);
    }
    Ok(values)
}
