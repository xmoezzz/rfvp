//! Input utils and wrappers

use rc::ResultInvalidControllerId;

use crate::applet;
use crate::ipc::sf;
use crate::result::*;
use crate::service;
use crate::service::applet::AppletResourceUserId;
use crate::service::hid;
use crate::service::hid::AnalogStickState;
use crate::service::hid::AppletResource;
use crate::service::hid::HidService;
use crate::service::hid::IAppletResourceClient;
use crate::service::hid::IHidClient;
use crate::service::hid::shmem;
use crate::service::hid::shmem::SharedMemoryFormat;
use crate::svc;
use crate::version;
use crate::vmem;

pub mod rc;

#[inline(always)]
fn get_npad_id_shmem_entry_index(npad_id: hid::NpadIdType) -> usize {
    match npad_id {
        hid::NpadIdType::Handheld => 8,
        hid::NpadIdType::Other => 9,
        _ => npad_id as usize, // (No1...8 -> 0...7)
    }
}

macro_rules! get_npad_property {
    ($self:expr, $property:ident) => {
        match $self.shmem {
            $crate::service::hid::shmem::SharedMemoryFormat::V1(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V2(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V3(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V4(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V5(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V6(m) => {
                &m.npad.entries[$self.npad_id_idx].$property
            }
        }
    };
}

macro_rules! get_keyboard_tail_item {
    ($self:expr) => {
        match $self.shmem {
            $crate::service::hid::shmem::SharedMemoryFormat::V1(m) => {
                m.keyboard.lifo.get_tail_item()
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V2(m) => {
                m.keyboard.lifo.get_tail_item()
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V3(m) => {
                m.keyboard.lifo.get_tail_item()
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V4(m) => {
                m.keyboard.lifo.get_tail_item()
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V5(m) => {
                m.keyboard.lifo.get_tail_item()
            }
            $crate::service::hid::shmem::SharedMemoryFormat::V6(m) => {
                m.keyboard.lifo.get_tail_item()
            }
        }
    };
}

macro_rules! get_state_one_tag {
    ($self:expr, $style_tag:expr, $state_field:ident) => {
        if $style_tag.contains(hid::NpadStyleTag::FullKey()) {
            get_npad_property!($self, full_key_lifo)
                .get_tail_item()
                .$state_field
        } else if $style_tag.contains(hid::NpadStyleTag::Handheld()) {
            get_npad_property!($self, handheld_lifo)
                .get_tail_item()
                .$state_field
        } else if $style_tag.contains(hid::NpadStyleTag::JoyDual()) {
            get_npad_property!($self, joy_dual_lifo)
                .get_tail_item()
                .$state_field
        } else if $style_tag.contains(hid::NpadStyleTag::JoyLeft()) {
            get_npad_property!($self, joy_left_lifo)
                .get_tail_item()
                .$state_field
        } else if $style_tag.contains(hid::NpadStyleTag::JoyRight()) {
            get_npad_property!($self, joy_right_lifo)
                .get_tail_item()
                .$state_field
        } else if $style_tag.contains(hid::NpadStyleTag::System())
            || $style_tag.contains(hid::NpadStyleTag::SystemExt())
        {
            get_npad_property!($self, system_ext_lifo)
                .get_tail_item()
                .$state_field
        } else {
            Default::default()
        }
    };
}

macro_rules! get_state_multi_tag {
    ($self:expr, $style_tag:expr, $state_type:ty, $state_field:ident) => {{
        let mut state: $state_type = Default::default();
        if $style_tag.contains(hid::NpadStyleTag::FullKey()) {
            state |= get_npad_property!($self, full_key_lifo)
                .get_tail_item()
                .$state_field;
        }
        if $style_tag.contains(hid::NpadStyleTag::Handheld()) {
            state |= get_npad_property!($self, handheld_lifo)
                .get_tail_item()
                .$state_field;
        }
        if $style_tag.contains(hid::NpadStyleTag::JoyDual()) {
            state |= get_npad_property!($self, joy_dual_lifo)
                .get_tail_item()
                .$state_field;
        }
        if $style_tag.contains(hid::NpadStyleTag::JoyLeft()) {
            state |= get_npad_property!($self, joy_left_lifo)
                .get_tail_item()
                .$state_field;
        }
        if $style_tag.contains(hid::NpadStyleTag::JoyRight()) {
            state |= get_npad_property!($self, joy_right_lifo)
                .get_tail_item()
                .$state_field;
        }
        if $style_tag.contains(hid::NpadStyleTag::System())
            || $style_tag.contains(hid::NpadStyleTag::SystemExt())
        {
            state |= get_npad_property!($self, system_ext_lifo)
                .get_tail_item()
                .$state_field;
        }
        state
    }};
}

/// Represents a console controller type
///
/// It's essentially a wrapper type over HID shared-memory to simplify input detection
pub struct Player<'player> {
    npad_id: hid::NpadIdType,
    npad_id_idx: usize,
    supported_style_tags: hid::NpadStyleTag,
    shmem: &'player SharedMemoryFormat,
    prev_buttons: hid::NpadButton,
}

impl<'player, 'context: 'player> Player<'player> {
    /// Creates a [`Player`] from shared-memory information
    ///
    /// If using a [`Context`], look for [`Context::get_player`] instead (for simplicity)
    ///
    /// # Arguments
    ///
    /// * `npad_id`: The [`NpadIdType`][`hid::NpadIdType`] of the desired controller
    /// * `supported_style_tags`: The [`NpadStyleTag`][`hid::NpadStyleTag`] flags which will be used by the [`Player`] to scan for input, etc.
    /// * `shmem_ptr`: The address of HID shared-memory
    pub fn new(
        npad_id: hid::NpadIdType,
        supported_style_tags: hid::NpadStyleTag,
        shmem: &'context SharedMemoryFormat,
    ) -> Result<Self> {
        Ok(Self {
            npad_id,
            npad_id_idx: get_npad_id_shmem_entry_index(npad_id),
            supported_style_tags,
            shmem,
            prev_buttons: Default::default(),
        })
    }

    #[inline]
    pub fn get_previous_buttons(&self) -> hid::NpadButton {
        self.prev_buttons
    }

    /// Gets the [`NpadAttribute`][`hid::NpadAttribute`]s for a certain [`NpadStyleTag`][`hid::NpadStyleTag`]
    ///
    /// # Arguments
    ///
    /// * `style_tag`: Must be a [`NpadStyleTag`][`hid::NpadStyleTag`] with a single flag set (otherwise only one will take effect and the rest will be ignored)
    #[inline]
    pub fn get_style_tag_attributes(&self, style_tag: hid::NpadStyleTag) -> hid::NpadAttribute {
        get_state_one_tag!(self, style_tag, attributes)
    }

    /// Gets the stick status from a provided style tag (which may or may not be configured)
    #[inline]
    pub fn get_stick_status(
        &self,
        style_tag: hid::NpadStyleTag,
    ) -> (AnalogStickState, AnalogStickState) {
        (
            get_state_one_tag!(self, style_tag, analog_stick_l),
            get_state_one_tag!(self, style_tag, analog_stick_r),
        )
    }

    #[inline]
    pub fn get_first_stick_status(
        &self,
        deadzone: f32,
    ) -> Option<(AnalogStickState, AnalogStickState)> {
        debug_assert!(
            (0.0..=1.0).contains(&deadzone),
            "deadzone is a factor in the range (0, 1)"
        );
        // TODO - make an iterator through the supported style tags to get the first one passing the deadzone test.

        None
    }

    /// Gets the [`NpadButton`][`hid::NpadButton`]s for a certain [`NpadStyleTag`][`hid::NpadStyleTag`]
    ///
    /// # Arguments
    ///
    /// * `style_tag`: Must be a [`NpadStyleTag`][`hid::NpadStyleTag`] with a single flag set (otherwise only one will take effect and the rest will be ignored)
    pub fn get_style_tag_buttons(&mut self, style_tag: hid::NpadStyleTag) -> hid::NpadButton {
        let cur_buttons = get_state_one_tag!(self, style_tag, buttons);
        self.prev_buttons = cur_buttons;
        cur_buttons
    }

    /// Gets the down [`NpadButton`][`hid::NpadButton`]s for a certain [`NpadStyleTag`][`hid::NpadStyleTag`]
    ///
    /// This is similar to `get_style_tag_buttons` but this only gets the buttons once after they're down/pressed
    ///
    /// # Arguments
    ///
    /// * `style_tag`: Must be a [`NpadStyleTag`][`hid::NpadStyleTag`] with a single flag set (otherwise only one will take effect and the rest will be ignored)
    pub fn get_style_tag_buttons_down(&mut self, style_tag: hid::NpadStyleTag) -> hid::NpadButton {
        let prev_buttons = self.prev_buttons;
        let cur_buttons = self.get_style_tag_buttons(style_tag);
        (!prev_buttons) & cur_buttons
    }

    /// Gets the up [`NpadButton`][`hid::NpadButton`]s for a certain [`NpadStyleTag`][`hid::NpadStyleTag`]
    ///
    /// This is similar to `get_style_tag_buttons` but this only gets the buttons once after they're up/released
    ///
    /// # Arguments
    ///
    /// * `style_tag`: Must be a [`NpadStyleTag`][`hid::NpadStyleTag`] with a single flag set (otherwise only one will take effect and the rest will be ignored)
    pub fn get_style_tag_buttons_up(&mut self, style_tag: hid::NpadStyleTag) -> hid::NpadButton {
        let prev_buttons = self.prev_buttons;
        let cur_buttons = self.get_style_tag_buttons(style_tag);
        prev_buttons & (!cur_buttons)
    }

    /// Gets the [`NpadButton`][`hid::NpadButton`]s for all of the supported [`NpadStyleTag`][`hid::NpadStyleTag`]s, combining all of them
    ///
    /// This is like combining the result of `get_style_tag_buttons` with all the supported [`NpadStyleTag`][`hid::NpadStyleTag`] flags
    pub fn get_buttons(&mut self) -> hid::NpadButton {
        let cur_buttons =
            get_state_multi_tag!(self, self.supported_style_tags, hid::NpadButton, buttons);
        self.prev_buttons = cur_buttons;
        cur_buttons
    }

    /// Gets the down [`NpadButton`][`hid::NpadButton`]s for all of the supported [`NpadStyleTag`][`hid::NpadStyleTag`]s, combining all of them
    ///
    /// This is similar to `get_buttons` but this only gets the buttons once after they're down/pressed
    pub fn get_buttons_down(&mut self) -> hid::NpadButton {
        let prev_buttons = self.prev_buttons;
        let cur_buttons = self.get_buttons();
        (!prev_buttons) & cur_buttons
    }

    /// Gets the up [`NpadButton`][`hid::NpadButton`]s for all of the supported [`NpadStyleTag`][`hid::NpadStyleTag`]s, combining all of them
    ///
    /// This is similar to `get_buttons` but this only gets the buttons once after they're up/released
    pub fn get_buttons_up(&mut self) -> hid::NpadButton {
        let prev_buttons = self.prev_buttons;
        let cur_buttons = self.get_buttons();
        prev_buttons & (!cur_buttons)
    }

    /// Gets the up [`NpadButton`][`hid::NpadButton`]s for all of the supported [`NpadStyleTag`][`hid::NpadStyleTag`]s, combining all of them
    ///
    /// This only updates the state once, but is otherwise equivalent to `(self.get_previous(), self.get_buttons(), self.get_buttons_down(), self.get_buttons_up())`
    #[inline]
    pub fn get_button_updates(
        &mut self,
    ) -> (
        hid::NpadButton,
        hid::NpadButton,
        hid::NpadButton,
        hid::NpadButton,
    ) {
        let prev_buttons = self.prev_buttons;
        let cur_buttons = self.get_buttons();
        (
            prev_buttons,
            cur_buttons,
            (!prev_buttons) & cur_buttons,
            prev_buttons & (!cur_buttons),
        )
    }

    /// Gets the [`NpadIdType`][`hid::NpadIdType`] being used with this [`Player`]
    #[inline]
    pub fn get_npad_id(&self) -> hid::NpadIdType {
        self.npad_id
    }

    /// Gets the supported [`NpadStyleTag`][`hid::NpadStyleTag`] flags being used with this [`Player`]
    #[inline]
    pub fn get_supported_style_tags(&self) -> hid::NpadStyleTag {
        self.supported_style_tags
    }

    #[inline]
    pub fn get_reported_style_tag(&self) -> hid::NpadStyleTag {
        *get_npad_property!(self, style_tag)
    }

    #[inline]
    pub fn get_controller_type(&self) -> hid::DeviceType {
        *get_npad_property!(self, device_type)
    }

    #[inline]
    pub fn get_keyboard_state(&self) -> shmem::KeyboardState {
        get_keyboard_tail_item!(self)
    }
}

/// Represents a simple type for dealing with input handling
#[allow(dead_code)]
pub struct Context {
    hid_service: HidService,
    applet_resource: AppletResource,
    supported_style_tags: hid::NpadStyleTag,
    shmem_handle: svc::Handle,
    shmem: SharedMemoryFormat,
}

impl Context {
    /// Creates a [`Context`] from supported input values
    ///
    /// The supported values are essentially used to enable supported controllers/controller configs via [`hid`] commands, and for opening [`Player`] types
    ///
    /// # Arguments
    ///
    /// * `supported_style_tags`: Supported [`NpadStyleTag`][`hid::NpadStyleTag`] flags
    /// * `supported_npad_ids`: Supported [`NpadIdType`][`hid::NpadIdType`] values
    pub fn new(supported_style_tags: hid::NpadStyleTag, mut player_count: usize) -> Result<Self> {
        result_return_unless!((1..=8).contains(&player_count), ResultInvalidControllerId);

        let mut players: [hid::NpadIdType; 9] = [hid::NpadIdType::No1; 9];

        for (player_id, player_slot) in players.iter_mut().enumerate().take(player_count) {
            // corresponding to values No1..No7
            *player_slot = unsafe { core::mem::transmute::<u32, hid::NpadIdType>(player_id as u32) }
        }

        if supported_style_tags.contains(hid::NpadStyleTag::Handheld())
            || supported_style_tags.contains(hid::NpadStyleTag::HandheldLark())
        {
            // allow for 8 controller players plus the handheld controllers still working
            players[player_count] = hid::NpadIdType::Handheld;
            player_count += 1;
        }

        let aruid = AppletResourceUserId::new(
            applet::GLOBAL_ARUID.load(core::sync::atomic::Ordering::Relaxed),
        );
        let players = sf::Buffer::from_array(&players[..player_count]);

        let mut hid_srv = service::new_service_object::<HidService>()?;
        let mut applet_res = hid_srv.create_applet_resource(aruid.clone())?;

        let shmem_handle = applet_res.get_shared_memory_handle()?;
        let shmem_address = vmem::allocate(shmem::SHMEM_SIZE)?;
        unsafe {
            svc::map_shared_memory(
                shmem_handle.handle,
                shmem_address,
                shmem::SHMEM_SIZE,
                svc::MemoryPermission::Read(),
            )?
        };

        Self::activate_npad(&mut hid_srv, aruid.clone())?;
        hid_srv.set_supported_npad_style_set(supported_style_tags, aruid.clone())?;
        hid_srv.set_supported_npad_id_type(aruid.clone(), players)?;

        let _styles = hid_srv.get_supported_npad_style_set(aruid);

        Ok(Self {
            hid_service: hid_srv,
            applet_resource: applet_res,
            supported_style_tags,
            shmem_handle: shmem_handle.handle,
            shmem: unsafe { SharedMemoryFormat::from_shmem_ptr(shmem_address)? },
        })
    }

    fn activate_npad(hid_srv: &mut HidService, aruid: AppletResourceUserId) -> Result<()> {
        let current_version = version::get_version();
        if current_version < version::Version::new(5, 0, 0) {
            hid_srv.activate_npad(aruid)
        } else {
            let revision = match current_version.major {
                0..6 => 1,
                6..8 => 2,
                8..18 => 3,
                18.. => 5,
            };
            hid_srv.activate_npad_with_revision(revision, aruid)
        }
    }

    /// Opens a [`Player`] type for the specified [`NpadIdType`][`hid::NpadIdType`]
    ///
    /// This simplifies creating a [`Player`] type, since this context contains the supported [`NpadStyleTag`][`hid::NpadStyleTag`] values and the mapped shared-memory address
    ///
    /// # Arguments
    ///
    ///  `npad_id`: The [`NpadIdType`][`hid::NpadIdType`] to use
    #[inline]
    pub fn get_player(&'_ self, npad_id: hid::NpadIdType) -> Player<'_> {
        Player::new(npad_id, self.supported_style_tags, &self.shmem)
            .expect("The pointers provided by the hid service should never be invalid")
    }

    /// Gets the current [`TouchScreenState`][`shmem::TouchScreenState`] values for the console touch-screen, returning the number of states present/set
    ///
    /// # Arguments
    ///
    /// * `touch_states`: Array of [`TouchState`][`hid::TouchState`] values to get filled, the array doesn't have to be bigger than `17` items
    pub fn get_touch_state(&self) -> shmem::TouchScreenState {
        match self.shmem {
            shmem::SharedMemoryFormat::V1(m) => &m.touch_screen,
            shmem::SharedMemoryFormat::V2(m) => &m.touch_screen,
            shmem::SharedMemoryFormat::V3(m) => &m.touch_screen,
            shmem::SharedMemoryFormat::V4(m) => &m.touch_screen,
            shmem::SharedMemoryFormat::V5(m) => &m.touch_screen,
            shmem::SharedMemoryFormat::V6(m) => &m.touch_screen,
        }
        .lifo
        .get_tail_item()
    }

    /// Gets the current [`TouchState`][`hid::TouchState`] values for the console touch-screen, returning the number of states present/set
    ///
    /// # Arguments
    ///
    /// * `touch_states`: Array of [`TouchState`][`hid::TouchState`] values to get filled, the array doesn't have to be bigger than `17` items
    pub fn get_touches(&self, touch_states: &mut [hid::TouchState]) -> usize {
        let screen_state = self.get_touch_state();
        let min_count = touch_states.len().min(screen_state.count as usize);
        touch_states[..min_count].copy_from_slice(&screen_state.touches[..min_count]);
        min_count
    }
}

impl Drop for Context {
    /// Destroys the [`Context`], un-mapping the shared-memory and closing it, and also closing its [`IHidClient`] session
    fn drop(&mut self) {
        let _ = self
            .hid_service
            .deactivate_npad(AppletResourceUserId::new(0));
        let _ = unsafe {
            svc::unmap_shared_memory(self.shmem_handle, self.shmem.as_ptr(), shmem::SHMEM_SIZE)
        };
        let _ = svc::close_handle(self.shmem_handle);
    }
}
