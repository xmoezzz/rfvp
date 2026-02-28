use std::fmt;

/// A tiny bitset implementation for graph analyses.
/// Index range: [0, n)
#[derive(Clone)]
pub struct BitSet {
    bits: Vec<u64>,
    n: usize,
}

impl BitSet {
    pub fn empty(n: usize) -> Self {
        let words = (n + 63) / 64;
        Self { bits: vec![0u64; words], n }
    }

    pub fn full(n: usize) -> Self {
        let mut s = Self::empty(n);
        for i in 0..n {
            s.insert(i);
        }
        s
    }

    pub fn singleton(n: usize, idx: usize) -> Self {
        let mut s = Self::empty(n);
        s.insert(idx);
        s
    }

    pub fn insert(&mut self, idx: usize) {
        debug_assert!(idx < self.n);
        let w = idx / 64;
        let b = idx % 64;
        self.bits[w] |= 1u64 << b;
    }

    pub fn contains(&self, idx: usize) -> bool {
        if idx >= self.n {
            return false;
        }
        let w = idx / 64;
        let b = idx % 64;
        (self.bits[w] >> b) & 1u64 == 1u64
    }

    pub fn intersect_assign(&mut self, other: &BitSet) {
        debug_assert_eq!(self.n, other.n);
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= *b;
        }
    }

    pub fn union_assign(&mut self, other: &BitSet) {
        debug_assert_eq!(self.n, other.n);
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a |= *b;
        }
    }

    pub fn remove(&mut self, idx: usize) {
        debug_assert!(idx < self.n);
        let w = idx / 64;
        let b = idx % 64;
        self.bits[w] &= !(1u64 << b);
    }

    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|x| *x == 0)
    }

    pub fn to_vec(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for i in 0..self.n {
            if self.contains(i) {
                out.push(i);
            }
        }
        out
    }
}

impl fmt::Debug for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.to_vec()).finish()
    }
}

pub fn compute_dominators(n: usize, preds: &[Vec<usize>], entry: usize) -> Vec<BitSet> {
    let mut dom: Vec<BitSet> = vec![BitSet::full(n); n];
    dom[entry] = BitSet::singleton(n, entry);

    let mut changed = true;
    while changed {
        changed = false;
        for v in 0..n {
            if v == entry {
                continue;
            }
            if preds[v].is_empty() {
                // Unreachable from entry in our CFG model.
                let newset = BitSet::singleton(n, v);
                if dom[v].to_vec() != newset.to_vec() {
                    dom[v] = newset;
                    changed = true;
                }
                continue;
            }

            let mut newset = BitSet::full(n);
            for &p in &preds[v] {
                newset.intersect_assign(&dom[p]);
            }
            newset.insert(v);

            if dom[v].to_vec() != newset.to_vec() {
                dom[v] = newset;
                changed = true;
            }
        }
    }
    dom
}

/// Compute the immediate dominator for each node (entry has None).
pub fn compute_idom(dom: &[BitSet], preds: &[Vec<usize>], entry: usize) -> Vec<Option<usize>> {
    let n = dom.len();
    let mut idom = vec![None; n];

    for v in 0..n {
        if v == entry {
            continue;
        }
        if preds[v].is_empty() {
            continue;
        }

        let mut candidates: Vec<usize> = dom[v].to_vec().into_iter().filter(|&x| x != v).collect();
        if candidates.is_empty() {
            continue;
        }

        // Pick c such that all other strict dominators of v also dominate c.
        // In other words: candidates ⊆ dom[c].
        let mut chosen = None;
        'outer: for &c in &candidates {
            for &d in &candidates {
                if !dom[c].contains(d) {
                    continue 'outer;
                }
            }
            chosen = Some(c);
            break;
        }
        idom[v] = chosen;
    }

    idom
}

/// Postdominator analysis with a virtual exit node.
/// Returns (pdom, ipdom, virt_exit_id).
pub fn compute_postdominators(
    n: usize,
    succs: &[Vec<usize>],
    exits: &[usize],
) -> (Vec<BitSet>, Vec<Option<usize>>, usize) {
    let virt = n;
    let n2 = n + 1;

    let mut succs2: Vec<Vec<usize>> = Vec::with_capacity(n2);
    for i in 0..n {
        if succs[i].is_empty() || exits.contains(&i) {
            succs2.push(vec![virt]);
        } else {
            succs2.push(succs[i].clone());
        }
    }
    succs2.push(Vec::new()); // virt exit

    let mut pdom: Vec<BitSet> = vec![BitSet::full(n2); n2];
    pdom[virt] = BitSet::singleton(n2, virt);

    let mut changed = true;
    while changed {
        changed = false;
        for v in 0..n2 {
            if v == virt {
                continue;
            }
            let mut newset = BitSet::full(n2);
            for &s in &succs2[v] {
                newset.intersect_assign(&pdom[s]);
            }
            newset.insert(v);

            if pdom[v].to_vec() != newset.to_vec() {
                pdom[v] = newset;
                changed = true;
            }
        }
    }

    // ipdom: same rule as idom but on pdom sets, with successors instead of preds.
    let mut ipdom = vec![None; n2];
    for v in 0..n2 {
        if v == virt {
            continue;
        }
        let mut candidates: Vec<usize> = pdom[v].to_vec().into_iter().filter(|&x| x != v).collect();
        if candidates.is_empty() {
            continue;
        }
        let mut chosen = None;
        'outer: for &c in &candidates {
            for &d in &candidates {
                if !pdom[c].contains(d) {
                    continue 'outer;
                }
            }
            chosen = Some(c);
            break;
        }
        ipdom[v] = chosen;
    }

    (pdom, ipdom, virt)
}
