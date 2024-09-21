use std::sync::{Arc, Mutex};

use wgpu::RenderPassDescriptor;

struct Arena {
    arena: slotmap::SlotMap<slotmap::DefaultKey, ()>,
}

struct ScheduledForDeletion {}

struct HandleShared {
    arena: Mutex<Arena>,
    scheduler_for_deletion: Mutex<ScheduledForDeletion>,
}

// TODO: should we allow cloning? Probably not, use Arc if needed
struct Handle {
    // do we need a reference to the arena?
    // I don't think so actually
    shared: Arc<HandleShared>,
    key: slotmap::DefaultKey,
}

struct RenderableObject {
    // TODO: typed handles
}

impl RenderableObject {
    fn render<'pass>(&self, arena: &'pass mut Arena, encoder: &'pass mut wgpu::CommandEncoder) {
        // encoder.begin_render_pass(&RenderPassDescriptor {
        //     label: None,
        //     color_attachments: &[],
        //     depth_stencil_attachment: None,
        // });

        // arena
        todo!()
    }
}
