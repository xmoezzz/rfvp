use bevy_utils::{hashbrown::hash_map::Entry, StableHashMap};
use rfvp_core::{
    vm::command::types::{LayerId, LayerIdOpt, LayerType, VLayerId, VLayerIdRepr, PLANES_COUNT},
};
use smallvec::{smallvec, SmallVec};
use tracing::warn;

use crate::layer::LayerPropertiesSnapshot;

#[derive(Debug, Copy, Clone)]
pub struct LayerSelection {
    // TODO: enforce ordering?
    // TODO: do the layer plane changes affect the selection?
    // TODO: how to make an empty selection?
    pub low: LayerId,
    pub high: LayerId,
}

impl LayerSelection {
    pub fn iter(&self) -> impl Iterator<Item = LayerId> {
        LayerSelectionIter {
            current: LayerIdOpt::some(self.low),
            high: self.high,
        }
    }

    pub fn contains(&self, id: LayerId) -> bool {
        self.low <= id && id <= self.high
    }
}

struct LayerSelectionIter {
    current: LayerIdOpt,
    high: LayerId,
}

impl Iterator for LayerSelectionIter {
    type Item = LayerId;

    fn next(&mut self) -> Option<Self::Item> {
        match self.current.opt() {
            None => None,
            Some(current) => {
                if current > self.high {
                    None
                } else {
                    if current == self.high {
                        self.current = LayerIdOpt::none();
                    } else {
                        self.current = LayerIdOpt::some(current.next());
                    }

                    Some(current)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayerState {
    pub layerinit_params: Option<(LayerType, UntypedNumberArray)>,
    pub properties: LayerPropertiesSnapshot,
}

impl LayerState {
    pub fn new() -> Self {
        Self {
            layerinit_params: None,
            properties: LayerPropertiesSnapshot::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaneState {
    // TODO: allocations - bad?
    pub layers: StableHashMap<LayerId, LayerState>,
}

impl PlaneState {
    pub fn new() -> Self {
        Self {
            layers: StableHashMap::default(),
        }
    }

    pub fn get_layer(&self, layer_id: LayerId) -> Option<&LayerState> {
        self.layers.get(&layer_id)
    }

    pub fn get_layer_mut(&mut self, layer_id: LayerId) -> Option<&mut LayerState> {
        self.layers.get_mut(&layer_id)
    }

    pub fn alloc(&mut self, layer_id: LayerId) -> &mut LayerState {
        match self.layers.entry(layer_id) {
            // TODO: downgrade to a warning?
            Entry::Occupied(_) => panic!("LayerState::alloc: layer already allocated"),
            Entry::Vacant(v) => v.insert(LayerState {
                layerinit_params: None,
                properties: LayerPropertiesSnapshot::new(),
            }),
        }
    }

    pub fn free(&mut self, layer_id: LayerId) {
        if self.layers.remove(&layer_id).is_none() {
            // this warning is too noisy to be useful IMO
            // this needs to be more specific
            // warn!("LayerState::free: layer not allocated");
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayersState {
    pub current_plane: u32,
    pub layer_selection: Option<LayerSelection>,
    pub planes: [PlaneState; PLANES_COUNT],

    pub root_layer_group: LayerState,
    pub screen_layer: LayerState,
    pub page_layer: LayerState,
    pub plane_layer_group: LayerState,
}

/// can be whatever, just an optimization. Ideally, most selections made by the script should fit in
pub const ITER_VLAYER_SMALL_VECTOR_SIZE: usize = 0x10;

impl LayersState {
    pub fn new() -> Self {
        Self {
            current_plane: 0,
            layer_selection: None,
            planes: [
                PlaneState::new(),
                PlaneState::new(),
                PlaneState::new(),
                PlaneState::new(),
            ],
            root_layer_group: LayerState::new(),
            screen_layer: LayerState::new(),
            page_layer: LayerState::new(),
            plane_layer_group: LayerState::new(),
        }
    }

    /// Get user layer by id
    #[allow(unused)]
    pub fn get_layer(&self, layer_id: LayerId) -> Option<&LayerState> {
        self.planes[self.current_plane as usize].get_layer(layer_id)
    }

    /// Get user layer by id (mutable)
    pub fn get_layer_mut(&mut self, layer_id: LayerId) -> Option<&mut LayerState> {
        self.planes[self.current_plane as usize].get_layer_mut(layer_id)
    }

    /// Get layer by virtual id, handling the special layers & selection
    ///
    /// Returs an iterator over states of the layers corresponding to the vlayer
    #[allow(unused)]
    pub fn get_vlayer(&self, vlayer_id: VLayerId) -> impl Iterator<Item = &LayerState> {
        // if a special layer - return a single layer
        // if a normal layer id - return it if exists, otherwise print a warning and return an empty iterator
        // if a selection - return all layers in the selection and warn if the selection is empty
        match vlayer_id.repr() {
            VLayerIdRepr::RootLayerGroup => smallvec![&self.root_layer_group],
            VLayerIdRepr::ScreenLayer => smallvec![&self.screen_layer],
            VLayerIdRepr::PageLayer => smallvec![&self.page_layer],
            VLayerIdRepr::PlaneLayerGroup => smallvec![&self.plane_layer_group],
            VLayerIdRepr::Selected => {
                if let Some(selection) = self.layer_selection {
                    self.planes[self.current_plane as usize]
                        .layers
                        .iter()
                        .filter(move |(id, _)| selection.contains(**id))
                        .map(|(_, l)| l)
                        .collect::<SmallVec<&LayerState, { ITER_VLAYER_SMALL_VECTOR_SIZE }>>()
                } else {
                    warn!("LayersState::get_vlayer: no selection");
                    smallvec![]
                }
            }
            VLayerIdRepr::Layer(l) => {
                let v = self.get_layer(l);
                match v {
                    None => {
                        warn!("get_vlayer: layer not found: {:?}", l);
                        smallvec![]
                    }
                    Some(v) => smallvec![v],
                }
            }
        }
        .into_iter()
    }

    /// Get layer ids corresponding to the virtual id, handling the selection
    ///
    /// Note that this can return layer ids for layers that are not loaded (in case of using a selection)
    ///
    /// Attempt to get a layer id for a special layer panics (they have no "real" layer id)
    pub fn get_vlayer_ids(&self, vlayer_id: VLayerId) -> impl Iterator<Item = LayerId> {
        match vlayer_id.repr() {
            VLayerIdRepr::RootLayerGroup
            | VLayerIdRepr::ScreenLayer
            | VLayerIdRepr::PageLayer
            | VLayerIdRepr::PlaneLayerGroup => {
                panic!("get_vlayer_ids: special layer do not have ids");
            }
            VLayerIdRepr::Selected => {
                if let Some(selection) = self.layer_selection {
                    selection
                        .iter()
                        // do not filter the selection, for the sake of LAYERUNLOAD
                        // it unloads the layers in the VmState first
                        // and then it sucks ass, because it wouldn't unload
                        // .filter(|&id| self.get_layer(id).is_some())
                        .collect::<SmallVec<LayerId, { ITER_VLAYER_SMALL_VECTOR_SIZE }>>()
                        .into_iter()
                } else {
                    warn!("get_vlayer_ids: no selection");
                    smallvec![].into_iter()
                }
            }
            VLayerIdRepr::Layer(l) => smallvec![l].into_iter(),
        }
    }

    /// Get layer by virtual id, handling the special layers & selection
    ///
    /// Returs an iterator over (mutable) states of the layers corresponding to the vlayer
    pub fn get_vlayer_mut(&mut self, vlayer_id: VLayerId) -> impl Iterator<Item = &mut LayerState> {
        // same as get_vlayer, but mutable
        match vlayer_id.repr() {
            VLayerIdRepr::RootLayerGroup => smallvec![&mut self.root_layer_group],
            VLayerIdRepr::ScreenLayer => smallvec![&mut self.screen_layer],
            VLayerIdRepr::PageLayer => smallvec![&mut self.page_layer],
            VLayerIdRepr::PlaneLayerGroup => smallvec![&mut self.plane_layer_group],
            VLayerIdRepr::Selected => {
                // NOTE: usually, there are not that much layers present
                // so it's okay to do an O(N) iteration here
                if let Some(selection) = self.layer_selection {
                    self.planes[self.current_plane as usize]
                        .layers
                        .iter_mut()
                        .filter(|&(&id, _)| selection.contains(id))
                        .map(|(_, v)| v)
                        .collect::<SmallVec<&mut LayerState, { ITER_VLAYER_SMALL_VECTOR_SIZE }>>()
                } else {
                    warn!("LayersState::get_vlayer_mut: no selection");
                    smallvec![]
                }
            }
            VLayerIdRepr::Layer(l) => match self.get_layer_mut(l) {
                None => {
                    warn!("get_vlayer_mut: layer not found: {:?}", l);
                    smallvec![]
                }
                Some(l) => smallvec![l],
            },
        }
        .into_iter()
    }

    pub fn alloc(&mut self, layer_id: LayerId) -> &mut LayerState {
        self.planes[self.current_plane as usize].alloc(layer_id)
    }

    pub fn free(&mut self, layer_id: LayerId) {
        self.planes[self.current_plane as usize].free(layer_id)
    }
}
