//! Puts the app's icon in `roundhouse-desktop.exe` when it's built for Windows, which shows the icon of the program a
//! window belongs to. GPUI's Windows backend loads the icon resource with ID 1 for each window it opens, and the
//! taskbar shows that. The installer carries the same drawings, which `tools/package/ico.py` puts in an `.ico`.
//!
//! It writes the icon as a compiled resource file, a `.res`, and has the linker put it in the program. MSVC's linker
//! and LLVM's both take a `.res` as it is, so building needs no resource compiler, on Windows or from a Mac.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// The iconset the installer's icon is made from too.
const ICONSET: &str = "icons/working.iconset";
/// A drawing for each size, as `tools/package/ico.py` takes them: each one made for a display that isn't Retina, and
/// the 64-pixel one, which the iconset holds as 32 points at twice the scale. An icon can't be wider than 256.
const DRAWINGS: [&str; 5] =
    ["icon_16x16.png", "icon_32x32.png", "icon_32x32@2x.png", "icon_128x128.png", "icon_256x256.png"];
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The ID GPUI loads a window's icon by.
const APP_ICON: u16 = 1;
/// The resource types for one picture in an icon, and for the icon that lists them.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// What `rc.exe` marks each with: moveable and discardable, and the list pure as well.
const ICON_FLAGS: u16 = 0x1010;
const GROUP_FLAGS: u16 = 0x1030;
/// Neutral, since an icon has no language, so Windows finds it whatever the system's is.
const LANGUAGE: u16 = 0;
/// How long a resource's header is when its type and name are both numbers.
const HEADER_SIZE: u32 = 32;

/// `dir` is the package's directory.
pub(crate) fn embed(dir: &Path) -> Result<(), Box<dyn Error>> {
    if env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }
    let iconset = dir.join(ICONSET);
    println!("cargo::rerun-if-changed={}", iconset.display());
    let mut pictures = Vec::new();
    for name in DRAWINGS {
        let png = fs::read(iconset.join(name))?;
        pictures.push((side_of(&png).ok_or_else(|| format!("{name} isn't a square PNG at most 256 across"))?, png));
    }
    // Smallest first, as the installer's icon has them.
    pictures.sort();
    let res = PathBuf::from(env::var("OUT_DIR")?).join("app-icon.res");
    fs::write(&res, resources(&pictures)?)?;
    println!("cargo::rustc-link-arg-bin=roundhouse-desktop={}", res.display());
    Ok(())
}

/// How many pixels wide a square PNG is, from its header, or `None` if it isn't one an icon can hold.
fn side_of(png: &[u8]) -> Option<u32> {
    let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    let is_png = png.starts_with(PNG_SIGNATURE) && png.get(12..16) == Some(b"IHDR");
    (is_png && width == height && (1..=256).contains(&width)).then_some(width)
}

/// The `.res` file that holds each picture, in the order given, and the icon that lists them. Since Windows Vista a
/// picture in an icon can be a PNG as it is.
fn resources(pictures: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut res = Vec::new();
    // An empty resource first, which says the file is for 32-bit Windows.
    resource(&mut res, 0, 0, 0, &[])?;
    let mut group = Vec::new();
    // Reserved, then 1 for an icon rather than a cursor, then how many pictures it has.
    for word in [0, 1, u16::try_from(pictures.len())?] {
        group.extend(word.to_le_bytes());
    }
    for (id, (side, png)) in (1..).zip(pictures) {
        resource(&mut res, RT_ICON, id, ICON_FLAGS, png)?;
        // Its width and height, where 256 is written as 0, no palette, a reserved byte, one plane and 32 bits a pixel.
        let side = u8::try_from(side % 256)?;
        group.extend([side, side, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend(u32::try_from(png.len())?.to_le_bytes());
        group.extend(id.to_le_bytes());
    }
    resource(&mut res, RT_GROUP_ICON, APP_ICON, GROUP_FLAGS, &group)?;
    Ok(res)
}

/// Adds one resource, named by a number, with its header in front and padding after to the next four bytes.
fn resource(res: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) -> Result<(), Box<dyn Error>> {
    res.extend(u32::try_from(data.len())?.to_le_bytes());
    res.extend(HEADER_SIZE.to_le_bytes());
    // 0xFFFF before each says it's a number rather than a name.
    for word in [0xFFFF, kind, 0xFFFF, name] {
        res.extend(word.to_le_bytes());
    }
    // The data's version, the flags, the language, and the resource's version and characteristics, none of which
    // Windows reads.
    res.extend(0u32.to_le_bytes());
    res.extend(flags.to_le_bytes());
    res.extend(LANGUAGE.to_le_bytes());
    res.extend([0; 8]);
    res.extend(data);
    res.resize(res.len().next_multiple_of(4), 0);
    Ok(())
}
