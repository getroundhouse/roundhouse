//! The flags a C file of libvpx's takes beyond its target's, for the extension it's written for.

/// Which flags the C compiler takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compiler {
    /// MSVC's own `cl.exe`, as Windows is built in CI and for release.
    Msvc,
    /// Clang as MSVC's `cl.exe`, as `cargo xwin` builds for Windows from a Mac. Unlike `cl.exe`, it compiles an
    /// extension's intrinsics only with the extension's flag, which it takes as Clang spells it.
    ClangCl,
    /// GCC, or Clang, which takes GCC's flags.
    Gnu,
}

/// The flags `file`, a path in libvpx's source, takes beyond its target's own, for the extension the end of its name
/// says it's written for, such as `_avx2.c`, as libvpx's own build gives them. Its Makefile gives GCC a flag for each
/// extension, from `build/make/Makefile`. Its projects for Visual Studio give MSVC only `/arch:AVX`, to a file for AVX
/// or AVX2, from `build/make/gen_msvs_vcxproj.sh`, since MSVC takes any extension's intrinsics without a flag, and that
/// one only quiets its warnings.
pub fn flags(file: &str, compiler: Compiler) -> &'static [&'static str] {
    let Some(stem) = file.strip_suffix(".c") else { return &[] };
    match compiler {
        Compiler::Gnu | Compiler::ClangCl => gnu(stem),
        Compiler::Msvc if stem.ends_with("_avx") || stem.ends_with("_avx2") => &["/arch:AVX"],
        Compiler::Msvc => &[],
    }
}

/// GCC's flags for a file whose name, without `.c`, is `stem`. The longest suffix comes first, since `_neon_dotprod`
/// would otherwise read as no extension and `_sve2` as `_sve`.
fn gnu(stem: &str) -> &'static [&'static str] {
    const BY_SUFFIX: &[(&str, &[&str])] = &[
        ("_neon_dotprod", &["-march=armv8.2-a+dotprod"]),
        ("_neon_i8mm", &["-march=armv8.2-a+dotprod+i8mm"]),
        ("_sve2", &["-march=armv9-a+i8mm+sve2"]),
        ("_sve", &["-march=armv8.2-a+dotprod+i8mm+sve"]),
        ("_avx512", &["-mavx512f", "-mavx512cd", "-mavx512bw", "-mavx512dq", "-mavx512vl"]),
        ("_avx2", &["-mavx2"]),
        ("_avx", &["-mavx"]),
        ("_sse4", &["-msse4.1"]),
        ("_ssse3", &["-mssse3"]),
        ("_sse3", &["-msse3"]),
        ("_sse2", &["-msse2"]),
        ("_mmx", &["-mmmx"]),
    ];
    BY_SUFFIX.iter().find(|(suffix, _)| stem.ends_with(suffix)).map_or(&[], |(_, flags)| flags)
}

#[cfg(test)]
mod tests {
    use super::{Compiler, flags};

    #[test]
    fn each_extension_takes_the_flags_libvpx_gives_it_and_clang_cl_takes_gccs() {
        let files: &[(&str, &[&str], &[&str])] = &[
            ("vpx_dsp/vpx_convolve.c", &[], &[]),
            ("vpx_dsp/arm/vpx_convolve8_neon.c", &[], &[]),
            ("vpx_dsp/arm/vpx_convolve8_neon_dotprod.c", &["-march=armv8.2-a+dotprod"], &[]),
            ("vpx_dsp/arm/vpx_convolve8_neon_i8mm.c", &["-march=armv8.2-a+dotprod+i8mm"], &[]),
            ("vpx_dsp/arm/sum_squares_sve.c", &["-march=armv8.2-a+dotprod+i8mm+sve"], &[]),
            ("vpx_dsp/arm/highbd_sse_sve2.c", &["-march=armv9-a+i8mm+sve2"], &[]),
            ("vp8/common/x86/idct_blk_mmx.c", &["-mmmx"], &[]),
            ("vpx_dsp/x86/inv_txfm_sse2.c", &["-msse2"], &[]),
            ("vpx_dsp/x86/vpx_subpixel_8t_intrin_ssse3.c", &["-mssse3"], &[]),
            ("vp9/common/x86/vp9_highbd_iht4x4_add_sse4.c", &["-msse4.1"], &[]),
            ("vpx_dsp/x86/quantize_avx.c", &["-mavx"], &["/arch:AVX"]),
            ("vpx_dsp/x86/loopfilter_avx2.c", &["-mavx2"], &["/arch:AVX"]),
            ("vpx_dsp/x86/sad_avx512.c", &["-mavx512f", "-mavx512cd", "-mavx512bw", "-mavx512dq", "-mavx512vl"], &[]),
            ("vpx_ports/x86_abi_support.asm", &[], &[]),
        ];

        for (file, gnu, msvc) in files {
            assert_eq!(flags(file, Compiler::Gnu), *gnu, "{file} with GCC's flags");
            assert_eq!(flags(file, Compiler::Msvc), *msvc, "{file} with MSVC's");
            assert_eq!(flags(file, Compiler::ClangCl), *gnu, "{file} with clang-cl, which takes GCC's");
        }
    }
}
