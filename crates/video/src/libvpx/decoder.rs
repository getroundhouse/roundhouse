//! A libvpx decoder for one stream, VP8 or VP9, which turns each packet of it into the frame it shows.

use super::ffi;
use crate::picture::{Colors, Matrix, Picture, Plane, Subsampling};
use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::num::NonZeroU32;

/// Which of libvpx's decoders a stream needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    Vp8,
    Vp9,
}

/// What libvpx said when it couldn't start a decoder or decode a packet: its error's name, and the detail it gave, if
/// any. Its words are for a person or a log.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("libvpx: {error}{}", detail.as_deref().map_or_else(String::new, |detail| format!(": {detail}")))]
pub struct LibvpxError {
    pub error: String,
    pub detail: Option<String>,
}

/// Why a packet couldn't become a picture.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error(transparent)]
    Libvpx(#[from] LibvpxError),
    /// A frame that isn't 8-bit planes, such as one of VP9's profiles with more bits, which the copy of libvpx isn't
    /// built for, or one too big for libvpx to say how big.
    #[error("a frame in a format this doesn't show: libvpx's format {format} with {bit_depth} bits")]
    Format { format: i32, bit_depth: u32 },
    /// A packet bigger than libvpx can be given, 4 GB.
    #[error("a packet of {bytes} bytes, more than libvpx takes")]
    TooBig { bytes: usize },
}

/// One stream's decoder. libvpx keeps a pointer to its context, so the context is boxed and never moves.
pub(crate) struct Decoder {
    context: Box<ffi::CodecContext>,
    /// The image the last packet showed, which libvpx owns until the next packet is decoded, and which was checked to
    /// be one [`Picture`] can show. Null when it showed none.
    shown: *const ffi::Image,
}

impl Decoder {
    /// A decoder for `codec` that decodes on up to `threads`, which only VP9 uses.
    pub(crate) fn new(codec: Codec, threads: NonZeroU32) -> Result<Self, LibvpxError> {
        let mut context = Box::new(ffi::CodecContext::empty());
        let config = ffi::DecoderConfig { threads: threads.get(), width: 0, height: 0 };
        // SAFETY: both decoders' interfaces are static, the context and config outlive the call, and the version is
        // the one the declarations in `ffi` are for.
        let started = unsafe {
            let iface = match codec {
                Codec::Vp8 => ffi::vpx_codec_vp8_dx(),
                Codec::Vp9 => ffi::vpx_codec_vp9_dx(),
            };
            ffi::vpx_codec_dec_init_ver(&raw mut *context, iface, &raw const config, 0, ffi::DECODER_ABI_VERSION)
        };
        if started != ffi::OK {
            // libvpx frees what it made of a context it couldn't start, so there's nothing to destroy.
            return Err(error(started, None));
        }
        Ok(Decoder { context, shown: std::ptr::null() })
    }

    /// Decodes `packet`, one frame of the stream as its container holds it, and says whether it shows a picture, which
    /// [`Decoder::picture`] returns. A frame the stream only refers to, such as VP9's hidden ones, shows none. A VP8 or
    /// VP9 packet shows at most one picture, so any after the first are left for the next packet to replace.
    pub(crate) fn decode(&mut self, packet: &[u8]) -> Result<bool, DecodeError> {
        self.shown = std::ptr::null();
        let size = c_uint::try_from(packet.len()).map_err(|_| DecodeError::TooBig { bytes: packet.len() })?;
        // SAFETY: the context was started, and `packet` is `size` bytes that outlive the call, which reads them only
        // until it returns.
        let decoded =
            unsafe { ffi::vpx_codec_decode(&raw mut *self.context, packet.as_ptr(), size, std::ptr::null_mut(), 0) };
        if decoded != ffi::OK {
            return Err(error(decoded, Some(&self.context)).into());
        }
        let mut iterator: *const c_void = std::ptr::null();
        // SAFETY: the context was started, and the iterator starts null, as libvpx asks.
        let image = unsafe { ffi::vpx_codec_get_frame(&raw mut *self.context, &raw mut iterator) };
        // SAFETY: libvpx returns null or an image it owns until the next packet is decoded.
        let Some(shown) = (unsafe { image.as_ref() }) else { return Ok(false) };
        picture(shown)?;
        self.shown = image;
        Ok(true)
    }

    /// The picture the last packet showed, which stays the decoder's until the next packet is decoded.
    pub(crate) fn picture(&self) -> Option<Picture<'_>> {
        // SAFETY: `shown` is null or the image the last packet showed, which libvpx keeps until the next call that
        // decodes, which needs `&mut self`, so the borrow of `self` the picture holds keeps it alive.
        let shown = unsafe { self.shown.as_ref() }?;
        // `decode` checked it, so it's a picture.
        picture(shown).ok()
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: the context was started, and nothing uses it after this.
        unsafe { ffi::vpx_codec_destroy(&raw mut *self.context) };
    }
}

/// How `image`'s colours are stored, by libvpx's `vpx_color_space_t` and `vpx_color_range_t`. BT.709's matrix serves
/// the spaces whose matrices are close to it, SMPTE 240M's and BT.2020's, sRGB's planes hold green, blue and red, and
/// BT.601's matrix serves every other.
fn colors(image: &ffi::Image) -> Colors {
    /// `VPX_CS_BT_709`, `VPX_CS_SMPTE_240` and `VPX_CS_BT_2020`.
    const WIDE: [c_int; 3] = [2, 4, 5];
    /// `VPX_CS_SRGB`.
    const SRGB: c_int = 7;
    /// `VPX_CR_FULL_RANGE`.
    const FULL_RANGE: c_int = 1;
    let matrix = match image.color_space {
        SRGB => Matrix::Identity,
        space if WIDE.contains(&space) => Matrix::Bt709,
        _ => Matrix::Bt601,
    };
    Colors { matrix, full_range: image.range == FULL_RANGE }
}

/// The picture `image` holds, if it's 8-bit, in planes, borrowing them. One of green, blue and red has each pixel's
/// own, as VP9 has them.
fn picture(image: &ffi::Image) -> Result<Picture<'_>, DecodeError> {
    let format = || DecodeError::Format { format: image.format, bit_depth: image.bit_depth };
    let shift = |shift: c_uint| (shift <= 1).then_some(shift).ok_or_else(format);
    let subsampling = Subsampling { across: shift(image.x_chroma_shift)?, down: shift(image.y_chroma_shift)? };
    let colors = colors(image);
    let rgb_shared = colors.matrix == Matrix::Identity && subsampling != Subsampling::FOUR_FOUR_FOUR;
    if !ffi::PLANAR_FORMATS.contains(&image.format) || image.bit_depth != 8 || rgb_shared {
        return Err(format());
    }
    let width = usize::try_from(image.display_width).map_err(|_| format())?;
    let height = usize::try_from(image.display_height).map_err(|_| format())?;
    let (chroma_width, chroma_height) = subsampling.chroma_size(width, height);
    let plane = |index: usize, width: usize, height: usize| {
        let stride = usize::try_from(image.stride[index]).map_err(|_| format())?;
        if height == 0 || stride < width {
            return Err(format());
        }
        let bytes = stride * (height - 1) + width;
        // SAFETY: libvpx's plane `index` of an 8-bit planar image holds `height` rows of at least `width` bytes,
        // `stride` apart, U's and V's as big as its chroma shifts make them, which `bytes` reaches the end of, and
        // lives as long as `image`.
        let data = unsafe { std::slice::from_raw_parts(image.planes[index], bytes) };
        Ok(Plane { data, stride, width, height })
    };
    Ok(Picture {
        width,
        height,
        colors,
        subsampling,
        y: plane(0, width, height)?,
        u: plane(1, chroma_width, chroma_height)?,
        v: plane(2, chroma_width, chroma_height)?,
    })
}

/// What libvpx says `code` means, and the context's detail of it, when there's a context to ask.
fn error(code: c_int, context: Option<&ffi::CodecContext>) -> LibvpxError {
    // SAFETY: libvpx returns a static string for any code.
    let error = unsafe { text(ffi::vpx_codec_err_to_string(code)) }.unwrap_or_else(|| format!("error {code}"));
    // SAFETY: the context was started, and its detail is null or a string it owns, which is copied at once.
    let detail = context.and_then(|context| unsafe { text(ffi::vpx_codec_error_detail(context)) });
    LibvpxError { error, detail }
}

/// The text at `pointer`, which is null or a C string.
///
/// # Safety
///
/// `pointer` is null or points to a C string that lives until this returns.
unsafe fn text(pointer: *const c_char) -> Option<String> {
    // SAFETY: as this function's contract says.
    (!pointer.is_null()).then(|| unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned())
}
