//! Bit-by-bit Huffman/VLC decoder (upstream get_vlc2 equivalent strategy).
//!
//! We intentionally use a tree (bit traversal) instead of a flat lookup table,
//! because MSMPEG4/WMV2 DC tables contain code lengths up to 24 bits.

use crate::bitreader::BitReader;

#[derive(Clone, Copy, Default)]
struct Node {
    left: Option<usize>,
    right: Option<usize>,
    sym: Option<i32>,
}

/// MSB-first VLC tree.
#[derive(Clone)]
pub struct VlcTree {
    nodes: Vec<Node>,
}

impl VlcTree {
    pub fn new() -> Self {
        VlcTree { nodes: vec![Node::default()] }
    }

    pub fn insert(&mut self, code: u32, len: u8, sym: i32) {
        let mut cur = 0usize;
        for bitpos in (0..len).rev() {
            let bit_is_one = ((code >> bitpos) & 1) != 0;

            // Avoid holding a mutable reference into `self.nodes` across a `push()`.
            // (A push may reallocate and would invalidate such a reference.)
            let next = if !bit_is_one { self.nodes[cur].left } else { self.nodes[cur].right };
            cur = match next {
                Some(i) => i,
                None => {
                    let i = self.nodes.len();
                    self.nodes.push(Node::default());
                    if !bit_is_one {
                        self.nodes[cur].left = Some(i);
                    } else {
                        self.nodes[cur].right = Some(i);
                    }
                    i
                }
            };
        }
        self.nodes[cur].sym = Some(sym);
    }

    /// Decode a symbol. Returns None on EOF or invalid code path.
    pub fn decode(&self, br: &mut BitReader<'_>) -> Option<i32> {
        let mut cur = 0usize;
        loop {
            if let Some(sym) = self.nodes[cur].sym {
                return Some(sym);
            }
            let bit = br.read_bit()?;
            cur = if !bit {
                self.nodes[cur].left?
            } else {
                self.nodes[cur].right?
            };
        }
    }
}
