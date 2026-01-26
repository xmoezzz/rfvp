use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use serde::{Deserialize, Serialize};



pub const INVAILD_PRIM_HANDLE: i16 = -1;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PrimType {
    #[default]
    PrimTypeNone = 0,
    PrimTypeGroup = 1,
    PrimTypeTile = 2,
    PrimTypeSprt = 4,
    PrimTypeText = 5,
    PrimTypeSnow = 7,
}

impl From<u8> for PrimType {
    fn from(v: u8) -> Self {
        match v {
            1 => PrimType::PrimTypeGroup,
            2 => PrimType::PrimTypeTile,
            4 => PrimType::PrimTypeSprt,
            5 => PrimType::PrimTypeText,
            7 => PrimType::PrimTypeSnow,
            _ => PrimType::PrimTypeNone,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Prim {
    typ: PrimType,
    draw_flag: bool,
    alpha: u8,
    blend: bool,
    is_paused: bool,
    parent: i16,
    sprt: i16,
    prev_sibling_idx: i16,
    next_sibling_idx: i16,
    z: i16,
    x: i16,
    y: i16,
    w: i16,
    h: i16,
    u: i16,
    v: i16,
    opx: i16,
    opy: i16,
    rotation: i16,
    factor_x: i16,
    factor_y: i16,
    first_child_idx: i16,
    last_child_idx: i16,
    texture_id: i16,
    tile: i16,
    text_index: i16,
    attr: u32,
}

impl Prim {
    pub fn new() -> Self {
        Prim {
            parent: INVAILD_PRIM_HANDLE,
            attr: 0,
            factor_x: 1000,
            factor_y: 1000,
            sprt: INVAILD_PRIM_HANDLE,
            prev_sibling_idx: INVAILD_PRIM_HANDLE,
            next_sibling_idx: INVAILD_PRIM_HANDLE,
            first_child_idx: INVAILD_PRIM_HANDLE,
            last_child_idx: INVAILD_PRIM_HANDLE,
            ..Default::default()
        }
    }

    pub fn set_type(&mut self, typ: PrimType) {
        self.typ = typ;
    }

    pub fn set_draw_flag(&mut self, draw_flag: bool) {
        self.draw_flag = draw_flag;
    }

    pub fn set_alpha(&mut self, alpha: u8) {
        self.alpha = alpha;
    }

    pub fn set_blend(&mut self, blend: bool) {
        self.blend = blend;
    }

    pub fn set_paused(&mut self, flag: bool) {
        self.is_paused = flag;
    }

    pub fn set_parent(&mut self, parent: i16) {
        self.parent = parent;
    }

    pub fn set_sprt(&mut self, sprt: i16) {
        self.sprt = sprt;
    }

    pub fn set_prev_sibling_idx(&mut self, prev_sibling_idx: i16) {
        self.prev_sibling_idx = prev_sibling_idx;
    }

    pub fn set_next_sibling_idx(&mut self, next_sibling_idx: i16) {
        self.next_sibling_idx = next_sibling_idx;
    }

    pub fn set_z(&mut self, z: i16) {
        self.z = z;
    }

    pub fn set_x(&mut self, x: i16) {
        self.x = x;
    }

    pub fn set_y(&mut self, y: i16) {
        self.y = y;
    }

    pub fn set_w(&mut self, w: i16) {
        self.w = w;
    }

    pub fn set_h(&mut self, h: i16) {
        self.h = h;
    }

    pub fn set_u(&mut self, u: i16) {
        self.u = u;
    }

    pub fn set_v(&mut self, v: i16) {
        self.v = v;
    }

    pub fn set_opx(&mut self, opx: i16) {
        self.opx = opx;
    }

    pub fn set_opy(&mut self, opy: i16) {
        self.opy = opy;
    }

    pub fn set_rotation(&mut self, rotation: i16) {
        self.rotation = rotation;
    }

    pub fn set_factor_x(&mut self, factor_x: i16) {
        self.factor_x = factor_x;
    }

    pub fn set_factor_y(&mut self, factor_y: i16) {
        self.factor_y = factor_y;
    }

    pub fn set_first_child_idx(&mut self, child: i16) {
        self.first_child_idx = child;
    }

    pub fn set_last_child_idx(&mut self, last_child_idx: i16) {
        self.last_child_idx = last_child_idx;
    }

    pub fn set_texture_id(&mut self, id: i16) {
        self.texture_id = id;
    }

    pub fn set_tile(&mut self, tile: i16) {
        self.tile = tile;
    }

    pub fn set_text_index(&mut self, text_index: i16) {
        self.text_index = text_index;
    }

    pub fn apply_attr(&mut self, attr: u32) {
        self.attr |= attr;
    }

    pub fn set_attr(&mut self, attr: u32) {
        self.attr = attr;
    }

    pub fn get_type(&self) -> PrimType {
        self.typ
    }

    pub fn get_draw_flag(&self) -> bool {
        self.draw_flag
    }

    pub fn get_alpha(&self) -> u8 {
        self.alpha
    }

    pub fn get_blend(&self) -> bool {
        self.blend
    }

    pub fn get_paused(&self) -> bool {
        self.is_paused
    }

    pub fn get_parent(&self) -> i16 {
        self.parent
    }

    pub fn get_sprt(&self) -> i16 {
        self.sprt
    }

    pub fn get_prev_sibling_idx(&self) -> i16 {
        self.prev_sibling_idx
    }

    pub fn get_next_sibling_idx(&self) -> i16 {
        self.next_sibling_idx
    }

    pub fn get_z(&self) -> i16 {
        self.z
    }

    pub fn get_x(&self) -> i16 {
        self.x
    }

    pub fn get_y(&self) -> i16 {
        self.y
    }

    pub fn get_w(&self) -> i16 {
        self.w
    }

    pub fn get_h(&self) -> i16 {
        self.h
    }

    pub fn get_u(&self) -> i16 {
        self.u
    }

    pub fn get_v(&self) -> i16 {
        self.v
    }

    pub fn get_opx(&self) -> i16 {
        self.opx
    }

    pub fn get_opy(&self) -> i16 {
        self.opy
    }

    pub fn get_rotation(&self) -> i16 {
        self.rotation
    }

    pub fn get_angle(&self) -> i16 {
        self.rotation
    }

    pub fn get_factor_x(&self) -> i16 {
        self.factor_x
    }

    pub fn get_factor_y(&self) -> i16 {
        self.factor_y
    }

    pub fn get_first_child_idx(&self) -> i16 {
        self.first_child_idx
    }

    pub fn get_last_child_idx(&self) -> i16 {
        self.last_child_idx
    }

    pub fn get_texture_id(&self) -> i16 {
        self.texture_id
    }

    pub fn get_tile(&self) -> i16 {
        self.tile
    }

    pub fn get_text_index(&self) -> i16 {
        self.text_index
    }

    pub fn get_attr(&self) -> u32 {
        self.attr
    }
}

#[derive(Debug)]
pub struct PrimManager {
    prims: Vec<AtomicRefCell<Prim>>,
    custom_root_prim_id: u16,
}

impl PrimManager {
    pub fn new() -> Self {
        let mut pm = Self {
            // allocate 4096 prims
            prims: (0..4096).map(|_| AtomicRefCell::new(Prim::new())).collect(),
            custom_root_prim_id: 0,
        };
        pm.prim_init_with_type(0, PrimType::PrimTypeGroup);
        pm.get_prim(0).set_alpha(255);
        pm
    }

    pub fn get_custom_root_prim_id(&self) -> u16 {
        self.custom_root_prim_id
    }

    /// Set the custom root primitive id used by renderer traversal.
    ///
    /// The syscall layer should sanitize negative/special ids; this expects a valid
    /// prim id in [0, 4095].
    pub fn set_custom_root_prim_id(&mut self, id: u16) {
        self.custom_root_prim_id = id;
    }

    pub fn get_prim(&self, id: i16) -> AtomicRefMut<'_, Prim> {
        self.prims[id as usize].borrow_mut()
    }

    pub fn get_prim_immutable(&self, id: i16) -> AtomicRef<'_, Prim> {
        self.prims[id as usize].borrow()
    }

    pub fn get_prims_mut(&mut self) -> &mut Vec<AtomicRefCell<Prim>> {
        &mut self.prims
    }

    pub fn prim_init_with_type(&mut self, id: i16, typ: PrimType) {
        //let mut prim = self.get_prim(id);
        if self.get_prim(id).get_type() != typ {
            if self.get_prim(id).get_type() == PrimType::PrimTypeGroup {
                let mut child = self.get_prim(id).get_first_child_idx();
                while child != INVAILD_PRIM_HANDLE {
                    // Capture next before unlinking; unlink_prim clears sibling links.
                    let next = self.get_prim(child).get_next_sibling_idx();
                    self.unlink_prim(child);
                    child = next;
                }
            }

            self.get_prim(id).set_type(typ);
            self.get_prim(id).set_draw_flag(true);
            if typ == PrimType::PrimTypeGroup {
                self.get_prim(id).set_first_child_idx(INVAILD_PRIM_HANDLE);
                self.get_prim(id).set_last_child_idx(INVAILD_PRIM_HANDLE);
                self.get_prim(id).set_x(0);
                self.get_prim(id).set_y(0);
            }
        }

        self.get_prim(id).apply_attr(0x40);
        self.get_prim(id).set_sprt(-1);
    }

    pub fn unlink_prim(&self, id: i16) {
        if id < 0 { return; }

        let parent = self.get_prim(id).get_parent();
        if parent == INVAILD_PRIM_HANDLE {
            // Still clear stale links to avoid later cycles.
            self.get_prim(id).set_prev_sibling_idx(INVAILD_PRIM_HANDLE);
            self.get_prim(id).set_next_sibling_idx(INVAILD_PRIM_HANDLE);
            return;
        }

        let prev = self.get_prim(id).get_prev_sibling_idx();
        let next = self.get_prim(id).get_next_sibling_idx();

        if prev == INVAILD_PRIM_HANDLE {
            self.get_prim(parent).set_first_child_idx(next);
        } else {
            self.get_prim(prev).set_next_sibling_idx(next);
        }

        if next == INVAILD_PRIM_HANDLE {
            self.get_prim(parent).set_last_child_idx(prev);
        } else {
            self.get_prim(next).set_prev_sibling_idx(prev);
        }

        self.get_prim(id).set_parent(INVAILD_PRIM_HANDLE);
        self.get_prim(id).set_prev_sibling_idx(INVAILD_PRIM_HANDLE);
        self.get_prim(id).set_next_sibling_idx(INVAILD_PRIM_HANDLE);
        self.get_prim(id).apply_attr(0x40);
    }

    pub fn prim_move(&mut self, new_root: i32, id: i32) {
        let new_root = new_root as i16;
        let id = id as i16;

        self.unlink_prim(id);

        let parent = self.get_prim(new_root).get_parent();
        if parent == INVAILD_PRIM_HANDLE {
            return;
        }

        let next = self.get_prim(new_root).get_next_sibling_idx();

        self.get_prim(id).set_parent(parent);
        self.get_prim(id).set_prev_sibling_idx(new_root);
        self.get_prim(id).set_next_sibling_idx(next);

        self.get_prim(new_root).set_next_sibling_idx(id);

        if next != INVAILD_PRIM_HANDLE {
            self.get_prim(next).set_prev_sibling_idx(id);
        } else {
            self.get_prim(parent).set_last_child_idx(id);
        }

        self.get_prim(id).apply_attr(0x40);
    }


    pub fn set_prim_group_in(&mut self, new_root: i32, id: i32) {
        let new_root = new_root as i16;
        let id = id as i16;

        self.prim_init_with_type(new_root, PrimType::PrimTypeGroup);
        self.unlink_prim(id);

        self.get_prim(id).set_parent(new_root);
        self.get_prim(id).set_next_sibling_idx(INVAILD_PRIM_HANDLE); // critical

        let first = self.get_prim(new_root).get_first_child_idx();
        if first == INVAILD_PRIM_HANDLE {
            self.get_prim(id).set_prev_sibling_idx(INVAILD_PRIM_HANDLE);
            self.get_prim(new_root).set_first_child_idx(id);
            self.get_prim(new_root).set_last_child_idx(id);
        } else {
            // Some buggy sequences may leave last_child_idx unset even though the list is non-empty.
            // Recompute a safe "last" by walking from first if needed.
            let mut last = self.get_prim(new_root).get_last_child_idx();
            if last == INVAILD_PRIM_HANDLE {
                let mut cur = first;
                for _ in 0..4096 {
                    let next = self.get_prim(cur).get_next_sibling_idx();
                    if next == INVAILD_PRIM_HANDLE {
                        last = cur;
                        break;
                    }
                    // Defensive: stop if next is out of range or forms an obvious self-loop.
                    if next < 0 || next == cur {
                        last = cur;
                        break;
                    }
                    cur = next;
                }
                self.get_prim(new_root).set_last_child_idx(last);
            }

            if last == INVAILD_PRIM_HANDLE {
                // Still inconsistent; treat as empty to avoid indexing -1.
                self.get_prim(id).set_prev_sibling_idx(INVAILD_PRIM_HANDLE);
                self.get_prim(new_root).set_first_child_idx(id);
                self.get_prim(new_root).set_last_child_idx(id);
            } else {
                self.get_prim(id).set_prev_sibling_idx(last);
                self.get_prim(last).set_next_sibling_idx(id);
                self.get_prim(new_root).set_last_child_idx(id);
            }
        }

        self.get_prim(id).apply_attr(0x40);
    }


    pub fn prim_set_op(&mut self, id: i32, opx: i32, opy: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_opx(opx as i16);
        prim.set_opy(opy as i16);

        // Matches the original engine behavior:
        // - (attr & 0x02) enables OP-based pivot semantics in the renderer.
        // - 0x40 is the common "dirty" bit used across many prim mutations.
        if prim.get_type() == PrimType::PrimTypeSprt {
            prim.apply_attr(0x02);
        }
    }

    /// Optional OP setter matching the original PrimSetOP semantics:
    /// - Only sprite prims (PrimTypeSprt) are affected.
    /// - opx/opy are applied independently when provided.
    /// - 0x02 is enabled only when at least one of opx/opy is provided.
    /// - 0x40 is always set for sprite prims when invoked.
    pub fn prim_set_op_partial(&mut self, id: i32, opx: Option<i32>, opy: Option<i32>) {
        let mut prim = self.get_prim(id as i16);
        if prim.get_type() != PrimType::PrimTypeSprt {
            return;
        }

        let mut any = false;
        if let Some(v) = opx {
            prim.set_opx(v as i16);
            any = true;
        }
        if let Some(v) = opy {
            prim.set_opy(v as i16);
            any = true;
        }

        if any {
            prim.apply_attr(0x02);
        }
        prim.apply_attr(0x40);
    }

    pub fn prim_set_alpha(&mut self, id: i32, alpha: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_alpha(alpha as u8);
    }

    pub fn prim_set_blend(&mut self, id: i32, blend: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_blend(blend != 0);
    }

    pub fn prim_set_draw(&mut self, id: i32, draw: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_draw_flag(draw != 0);
    }

    pub fn prim_set_rotation(&mut self, id: i32, rotation: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_rotation(rotation as i16);
    }

    pub fn prim_set_scale(&mut self, id: i32, factor_x: i32, factor_y: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_factor_x(factor_x as i16);
        prim.set_factor_y(factor_y as i16);
    }

    pub fn prim_set_uv(&mut self, id: i32, u: i32, v: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_u(u as i16);
        prim.set_v(v as i16);
    }

    pub fn prim_set_size(&mut self, id: i32, w: i32, h: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_w(w as i16);
        prim.set_h(h as i16);
    }

    pub fn prim_set_pos(&mut self, id: i32, x: i32, y: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_x(x as i16);
        prim.set_y(y as i16);
    }

    pub fn prim_set_sprt(&mut self, id: i32, sprt: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_sprt(sprt as i16);
    }

    pub fn prim_set_z(&mut self, id: i32, z: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_z(z as i16);
    }

    pub fn prim_set_texture_id(&mut self, id: i32, texture_id: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_texture_id(texture_id as i16);
    }

    pub fn prim_set_text(&mut self, id: i32, text_index: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_text_index(text_index as i16);
    }

    pub fn prim_set_tile(&mut self, id: i32, tile: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_tile(tile as i16);
    }

    pub fn prim_add_attr(&mut self, id: i32, mask: u32) {
        let mut prim = self.get_prim(id as i16);
        let attr = prim.get_attr();
        prim.set_attr(attr | mask);
    }

    pub fn prim_remove_attr(&mut self, id: i32, mask: u32) {
        let mut prim = self.get_prim(id as i16);
        let attr = prim.get_attr();
        prim.set_attr(attr & mask);
    }

    pub fn prim_set_attr(&mut self, id: i32, attr: i32) {
        let mut prim = self.get_prim(id as i16);
        prim.set_attr(attr as u32);
    }

    pub fn prim_get_type(&self, id: i32) -> PrimType {
        self.get_prim(id as i16).get_type()
    }
}


use std::collections::HashSet;

impl PrimManager {
    /// Dump the current primitive tree starting from `root`.
    ///
    /// This is intended for debugging only and is guarded by trace flags at call sites.
    pub fn debug_dump_tree(&self, root: i16, max_nodes: usize, max_depth: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "PrimTree(root={}, custom_root={}, max_nodes={}, max_depth={})\n",
            root,
            self.custom_root_prim_id,
            max_nodes,
            max_depth
        ));

        if root < 0 || root as usize >= self.prims.len() {
            out.push_str("  <invalid root>\n");
            return out;
        }

        let mut visited: HashSet<i16> = HashSet::new();
        let mut count: usize = 0;

        fn dump_node(pm: &PrimManager, id: i16, depth: usize, max_nodes: usize, max_depth: usize,
                     visited: &mut HashSet<i16>, count: &mut usize, out: &mut String) {
            if *count >= max_nodes {
                out.push_str("  <truncated: max_nodes reached>\n");
                return;
            }
            if depth > max_depth {
                out.push_str(&format!("{:indent$}<truncated: max_depth reached>\n", "", indent=depth*2));
                return;
            }
            if id < 0 || id as usize >= pm.prims.len() {
                out.push_str(&format!("{:indent$}<invalid prim id {}>\n", "", id, indent=depth*2));
                return;
            }
            if !visited.insert(id) {
                out.push_str(&format!("{:indent$}<cycle detected at {}>\n", "", id, indent=depth*2));
                return;
            }

            let p = pm.get_prim_immutable(id);

            let indent = depth * 2;
            out.push_str(&format!(
                "{:indent$}#{} type={:?} draw={} alpha={} blend={} paused={} parent={} sprt={} prev={} next={} first_child={} last_child={} \
x={} y={} z={} w={} h={} u={} v={} op=({}, {}) angle={} factor=({}, {}) tile={} text_index={} attr=0x{:08x}\n",
                "",
                id,
                p.get_type(),
                p.get_draw_flag(),
                p.get_alpha(),
                p.get_blend(),
                p.get_paused(),
                p.get_parent(),
                p.get_sprt(),
                p.get_prev_sibling_idx(),
                p.get_next_sibling_idx(),
                p.get_first_child_idx(),
                p.get_last_child_idx(),
                p.get_x(),
                p.get_y(),
                p.get_z(),
                p.get_w(),
                p.get_h(),
                p.get_u(),
                p.get_v(),
                p.get_opx(),
                p.get_opy(),
                p.get_angle(),
                p.get_factor_x(),
                p.get_factor_y(),
                p.get_tile(),
                p.get_text_index(),
                p.get_attr(),
                indent = indent
            ));

            let mut child = p.get_first_child_idx();
            drop(p);

            *count += 1;

            // Walk the sibling chain from first_child via next_sibling_idx.
            while child != INVAILD_PRIM_HANDLE {
                if *count >= max_nodes {
                    out.push_str(&format!("{:indent$}<truncated children: max_nodes reached>\n", "", indent=(depth+1)*2));
                    break;
                }
                dump_node(pm, child, depth + 1, max_nodes, max_depth, visited, count, out);

                let c = pm.get_prim_immutable(child);
                let next = c.get_next_sibling_idx();
                drop(c);
                child = next;
            }
        }

        dump_node(self, root, 0, max_nodes, max_depth, &mut visited, &mut count, &mut out);
        out
    }
}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimSnapshotV1 {
    pub typ: u8,
    pub draw_flag: bool,
    pub alpha: u8,
    pub blend: bool,
    pub is_paused: bool,
    pub parent: i16,
    pub sprt: i16,
    pub prev_sibling: i16,
    pub next_sibling: i16,
    pub first_child: i16,
    pub last_child: i16,

    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
    pub u: i16,
    pub v: i16,

    pub opx: i16,
    pub opy: i16,
    pub rotation: i16,
    pub factor_x: i16,
    pub factor_y: i16,

    pub texture_id: i16,

    pub z: i16,
    pub tile: i16,
    pub text_index: i16,
    pub attr: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimManagerSnapshotV1 {
    pub custom_root_prim_id: u16,
    pub prims: Vec<PrimSnapshotV1>,
}

impl Prim {
    pub fn capture_snapshot_v1(&self) -> PrimSnapshotV1 {
        PrimSnapshotV1 {
            typ: self.typ as u8,
            draw_flag: self.draw_flag,
            alpha: self.alpha,
            blend: self.blend,
            is_paused: self.is_paused,
            parent: self.parent,
            sprt: self.sprt,
            prev_sibling: self.prev_sibling_idx,
            next_sibling: self.next_sibling_idx,
            first_child: self.first_child_idx,
            last_child: self.last_child_idx,

            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            u: self.u,
            v: self.v,

            opx: self.opx,
            opy: self.opy,
            rotation: self.rotation,
            factor_x: self.factor_x,
            factor_y: self.factor_y,

            texture_id: self.texture_id,

            z: self.z,
            tile: self.tile,
            text_index: self.text_index,
            attr: self.attr,
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &PrimSnapshotV1) {
        self.typ = PrimType::from(snap.typ);
        self.draw_flag = snap.draw_flag;
        self.alpha = snap.alpha;
        self.blend = snap.blend;
        self.is_paused = snap.is_paused;
        self.parent = snap.parent;
        self.sprt = snap.sprt;
        self.prev_sibling_idx = snap.prev_sibling;
        self.next_sibling_idx = snap.next_sibling;
        self.first_child_idx = snap.first_child;
        self.last_child_idx = snap.last_child;

        self.x = snap.x;
        self.y = snap.y;
        self.w = snap.w;
        self.h = snap.h;
        self.u = snap.u;
        self.v = snap.v;

        self.opx = snap.opx;
        self.opy = snap.opy;
        self.rotation = snap.rotation;
        self.factor_x = snap.factor_x;
        self.factor_y = snap.factor_y;
        self.texture_id = snap.texture_id;

        self.z = snap.z;
        self.tile = snap.tile;
        self.text_index = snap.text_index;
        self.attr = snap.attr;
    }
}

impl PrimManager {
    pub fn capture_snapshot_v1(&self) -> PrimManagerSnapshotV1 {
        let mut prims = Vec::with_capacity(self.prims.len());
        for cell in &self.prims {
            let p = cell.borrow();
            prims.push(p.capture_snapshot_v1());
        }

        PrimManagerSnapshotV1 {
            custom_root_prim_id: self.custom_root_prim_id,
            prims,
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &PrimManagerSnapshotV1) {
        self.custom_root_prim_id = snap.custom_root_prim_id;

        let n = self.prims.len().min(snap.prims.len());
        for i in 0..n {
            let mut p = self.prims[i].borrow_mut();
            p.apply_snapshot_v1(&snap.prims[i]);
        }
    }
}
