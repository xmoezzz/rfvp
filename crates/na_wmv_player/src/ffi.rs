//! Minimal C ABI for WMV2 decoding.
//!
//! This module is behind the `ffi` feature.
//! It decodes one assembled WMV2 frame payload at a time and exposes the internal
//! YUV420p planes.

use std::ffi::c_void;

use crate::api::Wmv2Decoder;

#[repr(C)]
pub struct Wmv2FrameView {
    pub width: u32,
    pub height: u32,

    pub y_ptr: *const u8,
    pub y_len: usize,
    pub y_stride: usize,

    pub cb_ptr: *const u8,
    pub cb_len: usize,
    pub cb_stride: usize,

    pub cr_ptr: *const u8,
    pub cr_len: usize,
    pub cr_stride: usize,
}

struct Opaque {
    dec: Wmv2Decoder,
    has_frame: bool,
}

/// Create a WMV2 decoder.
///
/// `extradata` is copied.
#[no_mangle]
pub extern "C" fn wmv2_decoder_create(
    width: u32,
    height: u32,
    extradata: *const u8,
    extradata_len: usize,
) -> *mut c_void {
    let extra = unsafe {
        if extradata.is_null() || extradata_len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(extradata, extradata_len)
        }
    };

    let dec = Wmv2Decoder::new(width, height, extra);
    let opaque = Box::new(Opaque { dec, has_frame: false });
    Box::into_raw(opaque) as *mut c_void
}

#[no_mangle]
pub extern "C" fn wmv2_decoder_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(handle as *mut Opaque);
    }
}

/// Decode one assembled WMV2 frame payload.
///
/// Returns:
///   1  = decoded frame available
///   0  = no frame (header not found)
///  -1  = invalid arguments
///  -2  = decode error
#[no_mangle]
pub extern "C" fn wmv2_decoder_decode(
    handle: *mut c_void,
    payload: *const u8,
    payload_len: usize,
    is_key_frame: i32,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    if payload.is_null() || payload_len == 0 {
        return -1;
    }

    let opaque = unsafe { &mut *(handle as *mut Opaque) };
    let data = unsafe { std::slice::from_raw_parts(payload, payload_len) };

    match opaque.dec.decode_frame(data, is_key_frame != 0) {
        Ok(Some(_)) => {
            opaque.has_frame = true;
            1
        }
        Ok(None) => {
            opaque.has_frame = false;
            0
        }
        Err(_) => {
            opaque.has_frame = false;
            -2
        }
    }
}

/// Get the most recently decoded frame view.
///
/// The returned pointers remain valid until the next successful decode call.
///
/// Returns:
///   1  = success
///   0  = no decoded frame available
///  -1  = invalid arguments
#[no_mangle]
pub extern "C" fn wmv2_decoder_get_frame(handle: *mut c_void, out: *mut Wmv2FrameView) -> i32 {
    if handle.is_null() || out.is_null() {
        return -1;
    }

    let opaque = unsafe { &mut *(handle as *mut Opaque) };
    if !opaque.has_frame {
        return 0;
    }

    let f = opaque.dec.current_frame();
    let y_stride = f.width as usize;
    let uv_stride = (f.width as usize) / 2;

    unsafe {
        *out = Wmv2FrameView {
            width: f.width,
            height: f.height,
            y_ptr: f.y.as_ptr(),
            y_len: f.y.len(),
            y_stride,
            cb_ptr: f.cb.as_ptr(),
            cb_len: f.cb.len(),
            cb_stride: uv_stride,
            cr_ptr: f.cr.as_ptr(),
            cr_len: f.cr.len(),
            cr_stride: uv_stride,
        };
    }

    1
}
