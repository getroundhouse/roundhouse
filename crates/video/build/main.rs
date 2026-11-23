//! Compiles libvpx's VP8 and VP9 decoders, from the source it fetches at the commit `third_party/libvpx` pins, with the
//! config `tools/libvpx-sync.sh` wrote there for the target, into one static library, `vpx`. `libvpx-build` says what
//! that is.
//!
//! On x86, libvpx's fast paths are nasm's assembly, so nasm has to be on the PATH, and the build fails without it, as
//! `docs/reference/commands.md` says. libvpx's plain C would build without it, but decodes 3 to 5 times slower, which
//! can't keep up with a 60 fps Retina recording on an older x86 core.

use libvpx_build::{Compiler, Config, flags, sources};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::process::Command;

mod fetch;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let libvpx = manifest.join("../../third_party/libvpx");
    println!("cargo::rerun-if-changed={}", libvpx.display());
    let config = Config::for_target(&env::var("CARGO_CFG_TARGET_ARCH")?, &env::var("CARGO_CFG_TARGET_OS")?)?;
    let compiler = compiler()?;
    let configured = libvpx.join("config").join(config.folder());
    let out = PathBuf::from(env::var("OUT_DIR")?);
    let source = fetch::source(std::fs::read_to_string(libvpx.join("COMMIT"))?.trim(), &out)?;
    let list = std::fs::read_to_string(configured.join("sources.txt"))?;
    let files = sources(&list);

    // Every file in one group takes the same flags, since a `cc::Build` gives its flags to all its files.
    let mut groups: BTreeMap<&[&str], Vec<PathBuf>> = BTreeMap::new();
    for file in &files.c {
        groups.entry(flags(file, compiler)).or_default().push(source.join(file));
    }
    groups.entry(&[]).or_default().push(configured.join("vpx_config.c"));
    let mut objects = Vec::new();
    for (flags, files) in groups {
        let mut build = c_build(&libvpx, &configured, &source);
        for flag in flags {
            build.flag(flag);
        }
        objects.extend(build.files(files).try_compile_intermediates()?);
    }
    if let Some(format) = config.assembly_format() {
        let assembled = out.join("assembly");
        std::fs::create_dir_all(&assembled)?;
        for file in &files.assembly {
            objects.push(assemble(&source, &configured, file, format, &assembled)?);
        }
    }
    cc::Build::new().objects(objects).try_compile("vpx")?;
    Ok(())
}

/// Which flags the target's C compiler takes, by what `cc` finds it is: MSVC's own `cl.exe`, clang-cl, which takes
/// the same flags but for libvpx's extensions, or one that takes GCC's.
fn compiler() -> Result<Compiler, cc::Error> {
    let found = cc::Build::new().try_get_compiler()?;
    Ok(if found.is_like_clang_cl() {
        Compiler::ClangCl
    } else if found.is_like_msvc() {
        Compiler::Msvc
    } else {
        Compiler::Gnu
    })
}

/// A build of libvpx's C from `source`, which finds the config's headers before the source's. It's optimized and
/// without libvpx's own checks whatever the profile, so a debug build of the app plays video as a release does.
fn c_build(libvpx: &Path, configured: &Path, source: &Path) -> cc::Build {
    let mut build = cc::Build::new();
    build
        .include(configured)
        .include(libvpx.join("config"))
        .include(source)
        .opt_level(3)
        .debug(false)
        .define("NDEBUG", None)
        .warnings(false);
    build
}

/// Assembles `file`, a path in libvpx's source, with nasm, into `out`, and returns the object it wrote.
fn assemble(source: &Path, configured: &Path, file: &str, format: &str, out: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let object = out.join(format!("{}.o", file.replace('/', "_")));
    // nasm adds a file it includes to `-I` as it's written, so each folder ends with a separator.
    let include = |dir: &Path| format!("-I{}{MAIN_SEPARATOR}", dir.display());
    let assembled = Command::new("nasm")
        .args(["-f", format])
        .arg(include(configured))
        .arg(include(source))
        .arg("-o")
        .arg(&object)
        .arg(source.join(file))
        .status()
        .map_err(|error| {
            format!(
                "couldn't run nasm, which building libvpx for x86 needs, as docs/reference/commands.md says: {error}"
            )
        })?;
    if !assembled.success() {
        return Err(format!("nasm couldn't assemble {file}: {assembled}").into());
    }
    Ok(object)
}
