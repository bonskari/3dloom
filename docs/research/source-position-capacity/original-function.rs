fn source_positions(mesh: &Mesh) -> Result<Vec<[f32; 4]>, String> {
    if mesh.vertex_count() == 0 {
        return Err("GPU runtime requires nonempty source positions".into());
    }
    mesh.vertices()
        .iter()
        .map(|v| {
            if v.position.is_finite() {
                Ok([v.position.x, v.position.y, v.position.z, 1.0])
            } else {
                Err("nonfinite current world position".into())
            }
        })
        .collect()
}
