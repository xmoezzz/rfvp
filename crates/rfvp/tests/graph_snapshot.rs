use rfvp::{
    script::parser::Nls,
    subsystem::resources::{graph_buff::GraphBuff, vfs::Vfs},
};

fn source() -> Vec<u8> {
    let mut bytes = b"hzc1".to_vec();
    bytes.extend_from_slice(&32_u32.to_le_bytes());
    bytes.extend_from_slice(&32_u32.to_le_bytes());
    bytes.extend_from_slice(b"NVSG");
    for field in [0_u16, 1, 4, 2, 0, 0, 0, 0] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&[0; 12]);
    bytes.extend_from_slice(&[120, 156, 251, 255, 31, 63, 0, 0, 14, 46, 31, 225]);
    bytes
}

#[test]
fn modified_source_pixels_and_geometry_survive_v1_restore() {
    let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
    vfs.add_loose_file("white.nvsg", source());
    let mut graph = GraphBuff::new();
    graph.load_texture("white.nvsg", source()).unwrap();
    graph.set_color_tone(50, 25, 75);
    graph.offset_x = 3;
    graph.offset_y = 5;
    graph.display_width = 8;
    graph.display_height = 4;
    graph.u = 1;
    graph.v = 1;
    let pixels = graph.texture.as_ref().unwrap().to_rgba8().into_raw();
    let saved = graph.capture_snapshot_with_id(0);
    let mut restored = GraphBuff::new();
    restored.apply_snapshot_v1(&saved, &vfs).unwrap();
    assert_eq!(
        restored.texture.as_ref().unwrap().to_rgba8().into_raw(),
        pixels
    );
    assert_eq!(
        (restored.offset_x, restored.offset_y, restored.u, restored.v),
        (3, 5, 1, 1)
    );
    assert_eq!((restored.display_width, restored.display_height), (8, 4));
    assert_eq!(restored.texture_path, "white.nvsg");
}

#[test]
fn pristine_source_geometry_survives_v1_restore() {
    let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
    vfs.add_loose_file("white.nvsg", source());
    let mut graph = GraphBuff::new();
    graph.load_texture("white.nvsg", source()).unwrap();
    graph.offset_x = 3;
    graph.offset_y = 5;
    graph.display_width = 8;
    graph.display_height = 4;
    graph.u = 1;
    graph.v = 1;
    let saved = graph.capture_snapshot_with_id(0);
    assert!(saved.rgba.is_none());
    graph.apply_snapshot_v1(&saved, &vfs).unwrap();
    assert_eq!(
        (graph.offset_x, graph.offset_y, graph.u, graph.v),
        (3, 5, 1, 1)
    );
    assert_eq!((graph.display_width, graph.display_height), (8, 4));
}
