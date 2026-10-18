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

fn vfs() -> Vfs {
    let mut vfs = Vfs::new(Nls::ShiftJIS).unwrap();
    vfs.add_loose_file("white.nvsg", source());
    vfs
}

fn pixels(graph: &GraphBuff) -> Vec<u8> {
    graph.texture.as_ref().unwrap().to_rgba8().into_raw()
}

#[test]
fn toned_source_is_saved_as_path_and_rgb_then_replayed() {
    let mut graph = GraphBuff::new();
    graph.load_texture("white.nvsg", source()).unwrap();
    graph.set_color_tone(50, 25, 75);
    let saved = graph.capture_snapshot_with_id(0);
    assert!(saved.rgba.is_none());
    assert_eq!((saved.r_value, saved.g_value, saved.b_value), (50, 25, 75));

    let mut restored = GraphBuff::new();
    restored.apply_snapshot_v1(&saved, &vfs()).unwrap();
    assert_eq!(pixels(&restored), pixels(&graph));
    assert_eq!(
        (restored.get_r_value(), restored.get_g_value(), restored.get_b_value()),
        (50, 25, 75)
    );
    assert_eq!(restored.texture_path, "white.nvsg");
}

#[test]
fn parts_overlaid_source_keeps_its_pixels_across_restores() {
    let mut graph = GraphBuff::new();
    graph.load_texture("white.nvsg", source()).unwrap();
    graph.texture.as_mut().unwrap().as_mut_rgba8().unwrap().put_pixel(1, 1, image::Rgba([9, 8, 7, 255]));
    graph.mark_pixels_overlaid();
    let overlaid = pixels(&graph);
    let saved = graph.capture_snapshot_with_id(0);
    assert!(saved.rgba.is_some());

    let mut restored = GraphBuff::new();
    restored.apply_snapshot_v1(&saved, &vfs()).unwrap();
    assert_eq!(pixels(&restored), overlaid);
    assert_eq!(restored.texture_path, "white.nvsg");
    // Saving the restored graph again must not fall back to the unmodified asset.
    assert!(restored.capture_snapshot_with_id(0).rgba.is_some());
}
