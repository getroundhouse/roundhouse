//! What a config compiles, from its `sources.txt`.

use std::path::Path;

/// The files a config compiles, as paths in libvpx's source, in the order its `sources.txt` has them.
#[derive(Debug, PartialEq, Eq)]
pub struct Sources<'a> {
    pub c: Vec<&'a str>,
    /// nasm's, which only x86 has.
    pub assembly: Vec<&'a str>,
}

/// The files in `list`, a config's `sources.txt`, which has a path a line, each a C file or assembly.
pub fn sources(list: &str) -> Sources<'_> {
    let paths = list.lines().map(str::trim).filter(|line| !line.is_empty());
    let (assembly, c) = paths.partition(|path| Path::new(path).extension().is_some_and(|extension| extension == "asm"));
    Sources { c, assembly }
}

#[cfg(test)]
mod tests {
    use super::{Sources, sources};

    #[test]
    fn a_list_parts_into_c_and_assembly_and_skips_blank_lines() {
        let list = "vp8/common/alloccommon.c\nvpx_ports/emms_mmx.asm\n\nvpx_dsp/x86/inv_txfm_sse2.c\n";

        let parted = sources(list);

        assert_eq!(
            parted,
            Sources {
                c: vec!["vp8/common/alloccommon.c", "vpx_dsp/x86/inv_txfm_sse2.c"],
                assembly: vec!["vpx_ports/emms_mmx.asm"],
            }
        );
    }
}
