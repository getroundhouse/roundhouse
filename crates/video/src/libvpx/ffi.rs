//! What the decoder calls of libvpx, declared as its headers in `vpx/` declare it, at the commit
//! `third_party/libvpx/COMMIT` pins. A field or a constant that changes in a new release changes here with it.

use std::ffi::{c_char, c_int, c_long, c_uint, c_void};

/// `vpx_codec_ctx_t`, which libvpx fills in as it starts a decoder. Only libvpx writes it.
#[repr(C)]
pub(super) struct CodecContext {
    name: *const c_char,
    iface: *const c_void,
    err: c_int,
    err_detail: *const c_char,
    init_flags: c_long,
    config: *const c_void,
    private: *mut c_void,
}

impl CodecContext {
    /// One for libvpx to fill in, as `vpx_codec_dec_init_ver` expects.
    pub(super) fn empty() -> Self {
        CodecContext {
            name: std::ptr::null(),
            iface: std::ptr::null(),
            err: 0,
            err_detail: std::ptr::null(),
            init_flags: 0,
            config: std::ptr::null(),
            private: std::ptr::null_mut(),
        }
    }
}

/// `vpx_codec_dec_cfg_t`. A width and height of 0 let the stream say.
#[repr(C)]
pub(super) struct DecoderConfig {
    pub(super) threads: c_uint,
    pub(super) width: c_uint,
    pub(super) height: c_uint,
}

/// `vpx_image_t`, a decoded frame, which the decoder owns.
#[repr(C)]
pub(super) struct Image {
    pub(super) format: c_int,
    pub(super) color_space: c_int,
    pub(super) range: c_int,
    width: c_uint,
    height: c_uint,
    pub(super) bit_depth: c_uint,
    /// The size it's shown at, which `width` and `height`, the size it's stored at, can be bigger than.
    pub(super) display_width: c_uint,
    pub(super) display_height: c_uint,
    render_width: c_uint,
    render_height: c_uint,
    pub(super) x_chroma_shift: c_uint,
    pub(super) y_chroma_shift: c_uint,
    pub(super) planes: [*const u8; 4],
    pub(super) stride: [c_int; 4],
    bits_per_sample: c_int,
    user_private: *mut c_void,
    data: *mut u8,
    data_owner: c_int,
    self_allocated: c_int,
    frame_buffer_private: *mut c_void,
}

/// `VPX_DECODER_ABI_VERSION`, which is `3 + VPX_CODEC_ABI_VERSION`, which is `4 + VPX_IMAGE_ABI_VERSION`, which is 5.
/// libvpx refuses a decoder started with another, so a release that changes a struct above fails at once.
pub(super) const DECODER_ABI_VERSION: c_int = 3 + 4 + 5;
/// `VPX_IMG_FMT_I420`, `VPX_IMG_FMT_I422`, `VPX_IMG_FMT_I444` and `VPX_IMG_FMT_I440`: 8-bit planes of Y, U and V, U
/// and V at half the width and height, half the width, the whole of each, and half the height, as the image's chroma
/// shifts say too. Other formats are 16-bit planes, or U and V in one plane, which VP9 doesn't decode to.
pub(super) const PLANAR_FORMATS: [c_int; 4] = [0x100 | 2, 0x100 | 5, 0x100 | 6, 0x100 | 7];
/// `VPX_CODEC_OK`.
pub(super) const OK: c_int = 0;

unsafe extern "C" {
    pub(super) fn vpx_codec_vp8_dx() -> *const c_void;
    pub(super) fn vpx_codec_vp9_dx() -> *const c_void;
    pub(super) fn vpx_codec_dec_init_ver(
        context: *mut CodecContext,
        iface: *const c_void,
        config: *const DecoderConfig,
        flags: c_long,
        version: c_int,
    ) -> c_int;
    pub(super) fn vpx_codec_decode(
        context: *mut CodecContext,
        data: *const u8,
        size: c_uint,
        user_private: *mut c_void,
        deadline: c_long,
    ) -> c_int;
    pub(super) fn vpx_codec_get_frame(context: *mut CodecContext, iterator: *mut *const c_void) -> *const Image;
    pub(super) fn vpx_codec_destroy(context: *mut CodecContext) -> c_int;
    pub(super) fn vpx_codec_err_to_string(error: c_int) -> *const c_char;
    pub(super) fn vpx_codec_error_detail(context: *const CodecContext) -> *const c_char;
}
