use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicU64, AtomicUsize};

use crate::svc::rc::ResultInvalidAddress;

use super::*;

pub const SHMEM_SIZE: usize = 0x40000;

#[repr(C)]
pub struct RingLifo<T: Copy + Clone + Debug, const S: usize> {
    pub vptr: u64,
    pub buf_count: AtomicUsize,
    pub tail: AtomicUsize,
    pub count: AtomicUsize,
    pub items: [AtomicSample<T>; S],
}

impl<T: Copy + Clone + Debug, const S: usize> core::fmt::Debug for RingLifo<T, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RingLifo")
            .field("vptr", &self.vptr)
            .field("buf_count", &self.buf_count)
            .field("tail", &self.tail)
            .field("count", &self.count)
            .field("items", &self.items)
            .finish()
    }
}
// Copy bound it important as we (and libnx) rely on the data being trivially copy-able to memcopy the data out.
#[derive(Debug)]
#[repr(C)]
pub struct AtomicSample<T: Copy + Clone + Debug> {
    pub sampling_size: AtomicU64,
    pub storage: T,
}

impl<T: Copy + Debug, const S: usize> RingLifo<T, S> {
    pub fn get_tail_item(&self) -> T {
        let mut out_value: MaybeUninit<T> = MaybeUninit::uninit();
        loop {
            let tail_index = self.tail.load(Ordering::Acquire);
            let sampling_value_before =
                self.items[tail_index].sampling_size.load(Ordering::Acquire);
            // We read through a volatile pointer since it basically an uncached memcpy for our purposes.
            // The read is safe, as it is being constructed from a valid reference.
            unsafe {
                out_value.as_mut_ptr().write(core::ptr::read_volatile(
                    &raw const self.items[tail_index].storage,
                ))
            };
            if sampling_value_before == self.items[tail_index].sampling_size.load(Ordering::Acquire)
            {
                // the value hasn't been updated while we were reading it (split read)
                break;
            }
        }
        // we have successfully initialized the value slot, so we can unwrap it into the real type
        unsafe { out_value.assume_init() }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct DebugPadState {
    pub sampling_number: u64,
    pub attrs: DebugPadAttribute,
    pub buttons: DebugPadButton,
    pub analog_stick_r: AnalogStickState,
    pub analog_stick_l: AnalogStickState,
}

#[derive(Debug)]
#[repr(C)]
pub struct DebugPadSharedMemoryFormat {
    pub lifo: RingLifo<DebugPadState, 17>,
    pub pad: [u8; 0x138],
}
const_assert!(core::mem::size_of::<DebugPadSharedMemoryFormat>() == 0x400);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct TouchScreenState {
    pub sampling_number: u64,
    pub count: u32,
    pub reserved: [u8; 4],
    pub touches: [TouchState; 16],
}

#[derive(Debug)]
#[repr(C)]
pub struct TouchScreenSharedMemoryFormat {
    pub lifo: RingLifo<TouchScreenState, 17>,
    pub pad: [u8; 0x3C8],
}
const_assert!(core::mem::size_of::<TouchScreenSharedMemoryFormat>() == 0x3000);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct MouseState {
    pub sampling_number: u64,
    pub x: u32,
    pub y: u32,
    pub delta_x: u32,
    pub delta_y: u32,
    pub wheel_delta_x: u32,
    pub wheel_delta_y: u32,
    pub buttons: MouseButton,
    pub attributes: MouseAttribute,
}

#[derive(Debug)]
#[repr(C)]
pub struct MouseSharedMemoryFormat {
    pub lifo: RingLifo<MouseState, 17>,
    pub pad: [u8; 0xB0],
}
const_assert!(core::mem::size_of::<MouseSharedMemoryFormat>() == 0x400);

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct KeyboardState {
    pub sampling_number: u64,
    pub modifiers: KeyboardModifier,
    pub keys: KeyboardKeyStates,
}

#[derive(Debug)]
#[repr(C)]
pub struct KeyboardSharedMemoryFormat {
    pub lifo: RingLifo<KeyboardState, 17>,
    pub pad: [u8; 0x28],
}
const_assert!(core::mem::size_of::<KeyboardSharedMemoryFormat>() == 0x400);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct BasicXpadState {
    pub sampling_number: u64,
    pub attributes: BasicXpadAttribute,
    pub buttons: BasicXpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
}

#[derive(Debug)]
#[repr(C)]
pub struct BasicXpadSharedMemoryEntry {
    pub lifo: RingLifo<BasicXpadState, 17>,
    pub pad: [u8; 0x138],
}

#[derive(Debug)]
#[repr(C)]
pub struct BasicXpadSharedMemoryFormat {
    pub entries: [BasicXpadSharedMemoryEntry; 4],
}
const_assert!(core::mem::size_of::<BasicXpadSharedMemoryFormat>() == 0x1000);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct DigitizerState {
    pub sampling_number: u64,
    pub unk_1: [u32; 2],
    pub attributes: DigitizerAttribute,
    pub buttons: DigitizerButton,
    pub unk_2: [u32; 16],
}

#[derive(Debug)]
#[repr(C)]
pub struct DigitizerSharedMemoryFormat {
    pub lifo: RingLifo<DigitizerState, 17>,
    pub pad: [u8; 0x980],
}
const_assert!(core::mem::size_of::<DigitizerSharedMemoryFormat>() == 0x1000);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct HomeButtonState {
    pub sampling_number: u64,
    pub buttons: HomeButton,
}

#[derive(Debug)]
#[repr(C)]
pub struct HomeButtonSharedMemoryFormat {
    pub lifo: RingLifo<HomeButtonState, 17>,
    pub pad: [u8; 0x48],
}
const_assert!(core::mem::size_of::<HomeButtonSharedMemoryFormat>() == 0x200);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct SleepButtonState {
    pub sampling_number: u64,
    pub buttons: SleepButton,
}

#[derive(Debug)]
#[repr(C)]
pub struct SleepButtonSharedMemoryFormat {
    pub lifo: RingLifo<SleepButtonState, 17>,
    pub pad: [u8; 0x48],
}
const_assert!(core::mem::size_of::<SleepButtonSharedMemoryFormat>() == 0x200);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct CaptureButtonState {
    pub sampling_number: u64,
    pub buttons: CaptureButton,
}

#[derive(Debug)]
#[repr(C)]
pub struct CaptureButtonSharedMemoryFormat {
    pub lifo: RingLifo<CaptureButtonState, 17>,
    pub pad: [u8; 0x48],
}
const_assert!(core::mem::size_of::<CaptureButtonSharedMemoryFormat>() == 0x200);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InputDetectorState {
    pub source_state: InputSourceState,
    pub sampling_number: u64,
}

#[derive(Debug)]
#[repr(C)]
pub struct InputDetectorSharedMemoryEntry {
    pub lifo: RingLifo<InputDetectorState, 2>,
    pub pad: [u8; 0x30],
}

#[derive(Debug)]
#[repr(C)]
pub struct InputDetectorSharedMemoryFormat {
    pub entries: [InputDetectorSharedMemoryEntry; 16],
}
const_assert!(core::mem::size_of::<InputDetectorSharedMemoryFormat>() == 0x800);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct UniquePadConfig {
    pub pad_type: UniquePadType,
    pub interface: UniquePadInterface,
    pub serial_number: UniquePadSerialNumber,
    pub controller_number: u32,
    pub is_active: bool,
    pub reserved: [u8; 3],
    pub sampling_number: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct AnalogStickCalibrationStateImpl {
    pub state: AnalogStickState,
    pub flags: AnalogStickCalibrationFlags,
    pub stage: AnalogStickManualCalibrationStage,
    pub sampling_number: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct SixAxisSensorUserCalibrationState {
    pub flags: SixAxisSensorUserCalibrationFlags,
    pub reserved: [u8; 4],
    pub stage: SixAxisSensorUserCalibrationStage,
    pub sampling_number: u64,
}

#[derive(Debug)]
#[repr(C)]
pub struct UniquePadSharedMemoryEntry {
    pub config_lifo: RingLifo<UniquePadConfig, 2>,
    pub analog_stick_calibration_state_impls: [RingLifo<AnalogStickCalibrationStateImpl, 2>; 2],
    pub six_axis_sensor_user_calibration_state: RingLifo<SixAxisSensorUserCalibrationState, 2>,
    pub unique_pad_config_mutex: [u8; 0x40],
    pub pad: [u8; 0x220],
}

#[derive(Debug)]
#[repr(C)]
pub struct UniquePadSharedMemoryFormat {
    pub entries: [UniquePadSharedMemoryEntry; 16],
}
const_assert!(core::mem::size_of::<UniquePadSharedMemoryFormat>() == 0x4000);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadFullKeyColorState {
    pub attribute: ColorAttribute,
    pub color: NpadControllerColor,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadJoyColorState {
    pub attribute: ColorAttribute,
    pub left_joy_color: NpadControllerColor,
    pub right_joy_color: NpadControllerColor,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadFullKeyState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadHandheldState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadJoyDualState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadJoyLeftState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadJoyRightState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadSystemState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadPalmaState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadSystemExtState {
    pub sampling_number: u64,
    pub buttons: NpadButton,
    pub analog_stick_l: AnalogStickState,
    pub analog_stick_r: AnalogStickState,
    pub attributes: NpadAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct SixAxisSensorState {
    pub delta_time: u64,
    pub sampling_number: u64,
    pub acceleration_x: u32,
    pub acceleration_y: u32,
    pub acceleration_z: u32,
    pub angular_velocity_x: u32,
    pub angular_velocity_y: u32,
    pub angular_velocity_z: u32,
    pub angle_x: u32,
    pub angle_y: u32,
    pub angle_z: u32,
    pub direction: DirectionState,
    pub attributes: SixAxisSensorAttribute,
    pub reserved: [u8; 4],
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NfcXcdDeviceHandleStateImpl {
    pub handle: u64, // TODO: xcd::DeviceHandle
    pub is_available: bool,
    pub is_activated: bool,
    pub reserved: [u8; 6],
    pub sampling_number: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct NpadGcTriggerState {
    pub sampling_number: u64,
    pub trigger_l: u32,
    pub trigger_r: u32,
}

// V1: 1.0.0-3.0.2
// V2: 4.0.0-8.1.0
// V3: 9.0.0-12.1.0
// V4: 13.0.0-
#[derive(Debug)]
pub enum SharedMemoryFormat {
    V1(&'static SharedMemoryFormatV1),
    V2(&'static SharedMemoryFormatV2),
    V3(&'static SharedMemoryFormatV3),
    V4(&'static SharedMemoryFormatV4),
    V5(&'static SharedMemoryFormatV5),
    V6(&'static SharedMemoryFormatV6),
}

impl SharedMemoryFormat {
    /// Constructs an enum to get the controller parameters from the shared memory map.
    ///
    /// # Args
    /// * ptr - a const pointer to the controller's shared memory
    ///
    /// # Safety
    ///
    ///  It is the caller's responsibility to make sure the returned struct does not outlive the shared memory mapping.
    pub unsafe fn from_shmem_ptr(ptr: *const u8) -> Result<Self> {
        let firmware_version = version::get_version();

        // Safety - The calls to `cast()` should be safe as we only access it though the checked `as_ref()` calls,
        // and the pointer->reference preconditions are checked above.
        unsafe {
            if SharedMemoryFormatV1::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V1(
                    ptr.cast::<SharedMemoryFormatV1>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else if SharedMemoryFormatV2::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V2(
                    ptr.cast::<SharedMemoryFormatV2>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else if SharedMemoryFormatV3::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V3(
                    ptr.cast::<SharedMemoryFormatV3>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else if SharedMemoryFormatV4::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V4(
                    ptr.cast::<SharedMemoryFormatV4>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else if SharedMemoryFormatV5::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V5(
                    ptr.cast::<SharedMemoryFormatV5>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else if SharedMemoryFormatV6::VERSION_INTERVAL.contains(firmware_version) {
                Ok(Self::V6(
                    ptr.cast::<SharedMemoryFormatV6>()
                        .as_ref()
                        .ok_or(ResultInvalidAddress::make())?,
                ))
            } else {
                unreachable!(
                    "We should never have this happen as all versions should be covered by the above matching"
                )
            }
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        match *self {
            Self::V1(r) => r as *const SharedMemoryFormatV1 as *const _,
            Self::V2(r) => r as *const SharedMemoryFormatV2 as *const _,
            Self::V3(r) => r as *const SharedMemoryFormatV3 as *const _,
            Self::V4(r) => r as *const SharedMemoryFormatV4 as *const _,
            Self::V5(r) => r as *const SharedMemoryFormatV5 as *const _,
            Self::V6(r) => r as *const SharedMemoryFormatV6 as *const _,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryEntryV1 {
    pub style_tag: NpadStyleTag,
    pub joy_assignment_mode: NpadJoyAssignmentMode,
    pub full_key_color: NpadFullKeyColorState,
    pub joy_color: NpadJoyColorState,
    pub full_key_lifo: RingLifo<NpadFullKeyState, 17>,
    pub handheld_lifo: RingLifo<NpadHandheldState, 17>,
    pub joy_dual_lifo: RingLifo<NpadJoyDualState, 17>,
    pub joy_left_lifo: RingLifo<NpadJoyLeftState, 17>,
    pub joy_right_lifo: RingLifo<NpadJoyRightState, 17>,
    pub system_lifo: RingLifo<NpadSystemState, 17>,
    pub system_ext_lifo: RingLifo<NpadSystemExtState, 17>,
    pub full_key_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub handheld_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub device_type: DeviceType,
    pub reserved: [u8; 4],
    pub system_properties: NpadSystemProperties,
    pub system_button_properties: NpadSystemButtonProperties,
    pub battery_level_joy_dual: NpadBatteryLevel,
    pub battery_level_joy_left: NpadBatteryLevel,
    pub battery_level_joy_right: NpadBatteryLevel,
    pub nfc_xcd_device_handle_lifo: RingLifo<NfcXcdDeviceHandleStateImpl, 2>,
    pub mutex: [u8; 0x40],
    pub gc_trigger_lifo: RingLifo<NpadGcTriggerState, 17>,
    pub lark_type_l_and_main: NpadLarkType,
    pub lark_type_r: NpadLarkType,
    pub lucia_type: NpadLuciaType,
    pub lager_type: NpadLagerType,
    pub pad: [u8; 0xBF0],
}
const_assert!(core::mem::size_of::<NpadSharedMemoryEntryV1>() == 0x5000);

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryEntryV2 {
    pub style_tag: NpadStyleTag,
    pub joy_assignment_mode: NpadJoyAssignmentMode,
    pub full_key_color: NpadFullKeyColorState,
    pub joy_color: NpadJoyColorState,
    pub full_key_lifo: RingLifo<NpadFullKeyState, 17>,
    pub handheld_lifo: RingLifo<NpadHandheldState, 17>,
    pub joy_dual_lifo: RingLifo<NpadJoyDualState, 17>,
    pub joy_left_lifo: RingLifo<NpadJoyLeftState, 17>,
    pub joy_right_lifo: RingLifo<NpadJoyRightState, 17>,
    pub palma_lifo: RingLifo<NpadPalmaState, 17>,
    pub system_ext_lifo: RingLifo<NpadSystemExtState, 17>,
    pub full_key_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub handheld_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub device_type: DeviceType,
    pub reserved: [u8; 4],
    pub system_properties: NpadSystemProperties,
    pub system_button_properties: NpadSystemButtonProperties,
    pub battery_level_joy_dual: NpadBatteryLevel,
    pub battery_level_joy_left: NpadBatteryLevel,
    pub battery_level_joy_right: NpadBatteryLevel,
    pub nfc_xcd_device_handle_lifo: RingLifo<NfcXcdDeviceHandleStateImpl, 2>,
    pub mutex: [u8; 0x40],
    pub gc_trigger_lifo: RingLifo<NpadGcTriggerState, 17>,
    pub lark_type_l_and_main: NpadLarkType,
    pub lark_type_r: NpadLarkType,
    pub lucia_type: NpadLuciaType,
    pub lager_type: NpadLagerType,
    pub pad: [u8; 0xBF0],
}
const_assert!(core::mem::size_of::<NpadSharedMemoryEntryV2>() == 0x5000);

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryEntryV3 {
    pub style_tag: NpadStyleTag,
    pub joy_assignment_mode: NpadJoyAssignmentMode,
    pub full_key_color: NpadFullKeyColorState,
    pub joy_color: NpadJoyColorState,
    pub full_key_lifo: RingLifo<NpadFullKeyState, 17>,
    pub handheld_lifo: RingLifo<NpadHandheldState, 17>,
    pub joy_dual_lifo: RingLifo<NpadJoyDualState, 17>,
    pub joy_left_lifo: RingLifo<NpadJoyLeftState, 17>,
    pub joy_right_lifo: RingLifo<NpadJoyRightState, 17>,
    pub palma_lifo: RingLifo<NpadPalmaState, 17>,
    pub system_ext_lifo: RingLifo<NpadSystemExtState, 17>,
    pub full_key_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub handheld_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub device_type: DeviceType,
    pub reserved: [u8; 4],
    pub system_properties: NpadSystemProperties,
    pub system_button_properties: NpadSystemButtonProperties,
    pub battery_level_joy_dual: NpadBatteryLevel,
    pub battery_level_joy_left: NpadBatteryLevel,
    pub battery_level_joy_right: NpadBatteryLevel,
    pub applet_footer_ui_attributes: AppletFooterUiAttribute,
    pub applet_footer_ui_type: AppletFooterUiType,
    pub reserved_2: [u8; 0x7B],
    pub gc_trigger_lifo: RingLifo<NpadGcTriggerState, 17>,
    pub lark_type_l_and_main: NpadLarkType,
    pub lark_type_r: NpadLarkType,
    pub lucia_type: NpadLuciaType,
    pub lager_type: NpadLagerType,
    pub pad: [u8; 0xC10],
}
const_assert!(core::mem::size_of::<NpadSharedMemoryEntryV3>() == 0x5000);

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryEntryV4 {
    pub style_tag: NpadStyleTag,
    pub joy_assignment_mode: NpadJoyAssignmentMode,
    pub full_key_color: NpadFullKeyColorState,
    pub joy_color: NpadJoyColorState,
    pub full_key_lifo: RingLifo<NpadFullKeyState, 17>,
    pub handheld_lifo: RingLifo<NpadHandheldState, 17>,
    pub joy_dual_lifo: RingLifo<NpadJoyDualState, 17>,
    pub joy_left_lifo: RingLifo<NpadJoyLeftState, 17>,
    pub joy_right_lifo: RingLifo<NpadJoyRightState, 17>,
    pub palma_lifo: RingLifo<NpadPalmaState, 17>,
    pub system_ext_lifo: RingLifo<NpadSystemExtState, 17>,
    pub full_key_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub handheld_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_dual_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_left_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub joy_right_six_axis_sensor_lifo: RingLifo<SixAxisSensorState, 17>,
    pub device_type: DeviceType,
    pub reserved: [u8; 4],
    pub system_properties: NpadSystemProperties,
    pub system_button_properties: NpadSystemButtonProperties,
    pub battery_level_joy_dual: NpadBatteryLevel,
    pub battery_level_joy_left: NpadBatteryLevel,
    pub battery_level_joy_right: NpadBatteryLevel,
    pub applet_footer_ui_attributes: AppletFooterUiAttribute,
    pub applet_footer_ui_type: AppletFooterUiType,
    pub reserved_2: [u8; 0x7B],
    pub gc_trigger_lifo: RingLifo<NpadGcTriggerState, 17>,
    pub lark_type_l_and_main: NpadLarkType,
    pub lark_type_r: NpadLarkType,
    pub lucia_type: NpadLuciaType,
    pub lager_type: NpadLagerType,
    pub six_axis_sensor_properties: [SixAxisSensorProperties; 6],
    pub pad: [u8; 0xC08],
}
const_assert!(core::mem::size_of::<NpadSharedMemoryEntryV4>() == 0x5000);

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryFormatV1 {
    pub entries: [NpadSharedMemoryEntryV1; 10],
}

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryFormatV2 {
    pub entries: [NpadSharedMemoryEntryV2; 10],
}

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryFormatV3 {
    pub entries: [NpadSharedMemoryEntryV3; 10],
}

#[derive(Debug)]
#[repr(C)]
pub struct NpadSharedMemoryFormatV4 {
    pub entries: [NpadSharedMemoryEntryV4; 10],
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C)]
pub struct GestureDummyState {
    pub sampling_number: u64,
    pub context_number: u8,
    pub gesture_type: GestureType,
    pub direction: GestureDirection,
    pub x: u32,
    pub y: u32,
    pub delta_x: u32,
    pub delta_y: u32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub attributes: GestureAttribute,
    pub scale: u32,
    pub rotation_angle: u32,
    pub point_count: u32,
    pub points: [GesturePoint; 4],
}

#[derive(Debug)]
#[repr(C)]
pub struct GestureSharedMemoryFormat {
    pub lifo: RingLifo<GestureDummyState, 17>,
    pub pad: [u8; 0xF8],
}
const_assert!(core::mem::size_of::<GestureSharedMemoryFormat>() == 0x800);

#[derive(Debug)]
#[repr(C)]
pub struct ConsoleSixAxisSensorSharedMemoryFormat {
    pub sampling_number: AtomicU64,
    pub is_seven_six_axis_sensor_at_rest: bool,
    pub pad: [u8; 3],
    pub verticalization_error: f32,
    pub gyro_bias: [f32; 3],
    pub pad2: [u8; 4],
}
const_assert!(core::mem::size_of::<ConsoleSixAxisSensorSharedMemoryFormat>() == 0x20);

// V1: 1.0.0-3.0.2
// V2: 4.0.0-4.1.0
// V3: 5.0.0-8.1.0/8.1.1
// V4: 9.0.0-9.2.0
// V5: 10.0.0-12.1.0
// V6: 13.0.0-

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV1 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub basic_xpad: BasicXpadSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub unique_pad: UniquePadSharedMemoryFormat,
    pub npad: NpadSharedMemoryFormatV1,
    pub gesture: GestureSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV1>() <= 8);

impl SharedMemoryFormatV1 {
    pub const VERSION_INTERVAL: version::VersionInterval = version::VersionInterval::from_to(
        version::Version::new(1, 0, 0),
        version::Version::new(3, 0, 2),
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV2 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub basic_xpad: BasicXpadSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub unique_pad: UniquePadSharedMemoryFormat,
    pub npad: NpadSharedMemoryFormatV2,
    pub gesture: GestureSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV2>() <= 8);

impl SharedMemoryFormatV2 {
    pub const VERSION_INTERVAL: version::VersionInterval = version::VersionInterval::from_to(
        version::Version::new(4, 0, 0),
        version::Version::new(4, 1, 0),
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV3 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub basic_xpad: BasicXpadSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub pad: [u8; 0x4000],
    pub npad: NpadSharedMemoryFormatV2,
    pub gesture: GestureSharedMemoryFormat,
    pub console_six_axis_sensor: ConsoleSixAxisSensorSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV3>() <= 8);

impl SharedMemoryFormatV3 {
    pub const VERSION_INTERVAL: version::VersionInterval = version::VersionInterval::from_to(
        version::Version::new(5, 0, 0),
        version::Version::new(8, 1, 1),
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV4 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub basic_xpad: BasicXpadSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub pad: [u8; 0x4000],
    pub npad: NpadSharedMemoryFormatV3,
    pub gesture: GestureSharedMemoryFormat,
    pub console_six_axis_sensor: ConsoleSixAxisSensorSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV4>() <= 8);

impl SharedMemoryFormatV4 {
    pub const VERSION_INTERVAL: version::VersionInterval = version::VersionInterval::from_to(
        version::Version::new(9, 0, 0),
        version::Version::new(9, 2, 0),
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV5 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub digitizer: DigitizerSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub pad: [u8; 0x4000],
    pub npad: NpadSharedMemoryFormatV3,
    pub gesture: GestureSharedMemoryFormat,
    pub console_six_axis_sensor: ConsoleSixAxisSensorSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV5>() <= 8);

impl SharedMemoryFormatV5 {
    pub const VERSION_INTERVAL: version::VersionInterval = version::VersionInterval::from_to(
        version::Version::new(10, 0, 0),
        version::Version::new(12, 1, 0),
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SharedMemoryFormatV6 {
    pub debug_pad: DebugPadSharedMemoryFormat,
    pub touch_screen: TouchScreenSharedMemoryFormat,
    pub mouse: MouseSharedMemoryFormat,
    pub keyboard: KeyboardSharedMemoryFormat,
    pub digitizer: DigitizerSharedMemoryFormat,
    pub home_button: HomeButtonSharedMemoryFormat,
    pub sleep_button: SleepButtonSharedMemoryFormat,
    pub capture_button: CaptureButtonSharedMemoryFormat,
    pub input_detector: InputDetectorSharedMemoryFormat,
    pub pad: [u8; 0x4000],
    pub npad: NpadSharedMemoryFormatV4,
    pub gesture: GestureSharedMemoryFormat,
    pub console_six_axis_sensor: ConsoleSixAxisSensorSharedMemoryFormat,
}
const_assert!(align_of::<SharedMemoryFormatV6>() <= 8);

impl SharedMemoryFormatV6 {
    pub const VERSION_INTERVAL: version::VersionInterval =
        version::VersionInterval::from(version::Version::new(13, 0, 0));
}
