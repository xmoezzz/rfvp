pub const LAYERBANKS_COUNT: u8 = 0x30;
pub const LAYERS_COUNT: u32 = 0x100;
pub const PLANES_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id<T: num_traits::Unsigned + TryFrom<u32> + Into<u32> + Copy, const SENTINEL: u32>(T);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdOpt<T: num_traits::Unsigned + TryFrom<u32> + Into<u32> + Copy, const SENTINEL: u32>(T);

impl<T: num_traits::Unsigned + TryFrom<u32> + Into<u32> + Copy, const SENTINEL: u32>
    Id<T, SENTINEL>
{
    pub fn new(id: T) -> Self {
        assert!(
            (0..SENTINEL).contains(&id.into()),
            "Id::new: id out of range"
        );
        Self(id)
    }

    pub fn raw(self) -> T {
        self.0
    }

    pub fn next(self) -> Self {
        let id = self.0 + T::one();
        assert_ne!(id.into(), SENTINEL, "Id::next: id out of range");
        Self::new(id)
    }
}

impl<T: num_traits::Unsigned + TryFrom<u32> + Into<u32> + Copy, const SENTINEL: u32>
    IdOpt<T, SENTINEL>
{
    pub fn none() -> Self {
        Self(
            T::try_from(SENTINEL)
                .map_err(|_| "BUG: sentinel conversion failed")
                .unwrap(),
        )
    }

    pub fn some(id: Id<T, SENTINEL>) -> Self {
        Self(id.0)
    }

    pub fn opt(self) -> Option<Id<T, SENTINEL>> {
        if self.0.into() == SENTINEL {
            None
        } else {
            Some(Id(self.0))
        }
    }

    pub fn raw(self) -> T {
        self.0
    }
}

/// Layer id, but allowing only "real" layers
pub type LayerId = Id<u32, { LAYERS_COUNT }>;
/// Layer id, but allowing only "real" layers and a "none" value
pub type LayerIdOpt = IdOpt<u32, { LAYERS_COUNT }>;

/// Layer id, allowing for the special values -1, -2, -3, -4, -5
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VLayerId(i32);

#[derive(Debug)]
pub enum VLayerIdRepr {
    RootLayerGroup,
    ScreenLayer,
    PageLayer,
    PlaneLayerGroup,
    Selected,
    Layer(LayerId),
}

impl VLayerId {
    pub const MIN: i32 = -5;

    pub fn new(id: i32) -> Self {
        assert!(
            (Self::MIN..LAYERS_COUNT.try_into().unwrap()).contains(&id),
            "VLayerId::new: id out of range"
        );
        Self(id)
    }

    pub fn repr(self) -> VLayerIdRepr {
        if self.0 < 0 {
            match self.0 {
                -1 => VLayerIdRepr::RootLayerGroup,
                -2 => VLayerIdRepr::ScreenLayer,
                -3 => VLayerIdRepr::PageLayer,
                -4 => VLayerIdRepr::PlaneLayerGroup,
                -5 => VLayerIdRepr::Selected,
                _ => unreachable!(),
            }
        } else {
            VLayerIdRepr::Layer(LayerId::new(self.0.try_into().unwrap()))
        }
    }
}

