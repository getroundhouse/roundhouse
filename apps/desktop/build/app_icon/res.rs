//! The compiled resource file, a `.res`, that holds an icon, as `rc.exe` compiles one from an `.ico`.
//!
//! An `.ico` is a list of its pictures, each with its size and where it starts in the file, then the pictures. The
//! `.res` holds each picture as a resource of its own, numbered from 1 in the list's order, and then the icon, whose
//! list is the `.ico`'s with each picture's number in place of where it starts. Each resource has a header in front and
//! padding after, to the next four bytes, and the file starts with an empty one, which says it's for 32-bit Windows.
//! It's what `rc.exe` compiles from `LANGUAGE 0, 0` and `1 ICON "AppIcon.ico"`, byte for byte, which is how to check a
//! change to it.
//!
//! The desktop app's own tests take this module in too, since a build script's tests never run.

use std::error::Error;
use std::fmt;

/// The icon's number, which GPUI's Windows backend loads its windows' icon by.
const ICON: u16 = 1;

/// How an `.ico` starts, and the icon resource's list too: reserved, then 1 for an icon rather than a cursor. How many
/// pictures it has follows, to make a header of six bytes.
const ICO_START: [u8; 4] = [0, 0, 1, 0];
const ICO_HEADER: usize = 6;
/// How long each entry in an `.ico`'s list is, and how much of it the icon resource's list keeps: a picture's width,
/// height, colours, a reserved byte, planes, bits a pixel and length. The rest is where the picture starts.
const ICO_ENTRY: usize = 16;
const KEPT: usize = 12;
/// How long a resource's header is when its kind and its name are both numbers.
const HEADER: u32 = 32;
/// Neutral, since an icon has no language. Windows matches a resource's language against the one it looks for, and
/// falls back to neutral, so it finds this whatever the user's is.
const LANGUAGE: u16 = 0;

/// The kinds of resource the file holds, each with the type Windows finds it by and the flags `rc.exe` marks it with.
#[derive(Clone, Copy)]
enum Kind {
    /// The empty one the file starts with.
    Empty,
    /// One picture of an icon, `RT_ICON`.
    Picture,
    /// An icon, which lists its pictures, `RT_GROUP_ICON`.
    Icon,
}

impl Kind {
    fn type_id(self) -> u16 {
        match self {
            Self::Empty => 0,
            Self::Picture => 3,
            Self::Icon => 14,
        }
    }

    /// Moveable and discardable, and the icon pure as well, which Windows ignores, though `rc.exe` writes them.
    fn flags(self) -> u16 {
        match self {
            Self::Empty => 0,
            Self::Picture => 0x1010,
            Self::Icon => 0x1030,
        }
    }
}

/// Why a file isn't an icon a `.res` can be made from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NotAnIcon {
    /// It doesn't start as an icon does.
    Header,
    /// It lists no pictures.
    NoPictures,
    /// It ends before its list does, or before a picture the list gives.
    CutShort,
    /// A resource would be longer than one can be, which is 4 GiB.
    TooLong,
}

impl fmt::Display for NotAnIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Header => "it doesn't start as an icon does",
            Self::NoPictures => "it lists no pictures",
            Self::CutShort => "it ends before its list does, or before a picture the list gives",
            Self::TooLong => "a resource would be longer than one can be",
        })
    }
}

impl Error for NotAnIcon {}

/// The `.res` that holds the icon an `.ico` holds, as icon `ICON`.
pub(crate) fn from_ico(ico: &[u8]) -> Result<Vec<u8>, NotAnIcon> {
    let header = ico.get(..ICO_HEADER).ok_or(NotAnIcon::CutShort)?;
    if header[..4] != ICO_START {
        return Err(NotAnIcon::Header);
    }
    let count = u16::from_le_bytes([header[4], header[5]]);
    if count == 0 {
        return Err(NotAnIcon::NoPictures);
    }
    let entries = ico.get(ICO_HEADER..ICO_HEADER + ICO_ENTRY * usize::from(count)).ok_or(NotAnIcon::CutShort)?;
    let mut res = Vec::new();
    add(&mut res, Kind::Empty, 0, &[])?;
    let mut list = header.to_vec();
    for (id, entry) in (1..=count).zip(entries.as_chunks::<ICO_ENTRY>().0) {
        add(&mut res, Kind::Picture, id, picture(ico, entry)?)?;
        list.extend(&entry[..KEPT]);
        list.extend(id.to_le_bytes());
    }
    add(&mut res, Kind::Icon, ICON, &list)?;
    Ok(res)
}

/// The picture an entry in an `.ico`'s list gives, by its length and where it starts.
fn picture<'a>(ico: &'a [u8], entry: &[u8; ICO_ENTRY]) -> Result<&'a [u8], NotAnIcon> {
    let number = |at: usize| u32::from_le_bytes([entry[at], entry[at + 1], entry[at + 2], entry[at + 3]]);
    let (Ok(len), Ok(start)) = (usize::try_from(number(8)), usize::try_from(number(12))) else {
        return Err(NotAnIcon::CutShort);
    };
    start.checked_add(len).and_then(|end| ico.get(start..end)).ok_or(NotAnIcon::CutShort)
}

/// Adds a resource of a kind, by its number, with its header in front and padding after to the next four bytes.
fn add(res: &mut Vec<u8>, kind: Kind, id: u16, data: &[u8]) -> Result<(), NotAnIcon> {
    res.extend(u32::try_from(data.len()).map_err(|_| NotAnIcon::TooLong)?.to_le_bytes());
    res.extend(HEADER.to_le_bytes());
    // 0xFFFF before each says it's a number rather than a name.
    for word in [0xFFFF, kind.type_id(), 0xFFFF, id] {
        res.extend(word.to_le_bytes());
    }
    // The data's version, which `rc.exe` leaves at 0 too.
    res.extend(0u32.to_le_bytes());
    res.extend(kind.flags().to_le_bytes());
    res.extend(LANGUAGE.to_le_bytes());
    // The resource's version and characteristics, for tools that read resource files, which `rc.exe` leaves at 0 too.
    res.extend([0; 8]);
    res.extend(data);
    res.resize(res.len().next_multiple_of(4), 0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

    /// The app's icon, which `tools/package/ico.py` made.
    const APP_ICON: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/AppIcon.ico"));

    /// A picture in an `.ico`, with the start of its entry in the list: width, height, colours, a reserved byte,
    /// planes and bits a pixel.
    type Picture = ([u8; 8], Vec<u8>);

    /// A resource read back from a `.res`.
    #[derive(Debug, PartialEq, Eq)]
    struct Resource {
        type_id: u16,
        id: u16,
        language: u16,
        data: Data,
    }

    /// A resource's data, which a failure shows whole only when it's short, rather than every byte of a picture.
    #[derive(PartialEq, Eq)]
    struct Data(Vec<u8>);

    impl fmt::Debug for Data {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self.0.get(..16) {
                Some(start) if self.0.len() > 64 => write!(f, "{} bytes, starting {start:?}", self.0.len()),
                _ => write!(f, "{:?}", self.0),
            }
        }
    }

    /// An `.ico` that holds each picture, after the list and in its order, as `ico.py` lays one out.
    fn ico(pictures: &[Picture]) -> Vec<u8> {
        let mut ico = ICO_START.to_vec();
        ico.extend(u16::try_from(pictures.len()).unwrap().to_le_bytes());
        let mut start = ICO_HEADER + ICO_ENTRY * pictures.len();
        for (entry, png) in pictures {
            ico.extend(entry);
            ico.extend(u32::try_from(png.len()).unwrap().to_le_bytes());
            ico.extend(u32::try_from(start).unwrap().to_le_bytes());
            start += png.len();
        }
        for (_, png) in pictures {
            ico.extend(png);
        }
        ico
    }

    /// The pictures an `.ico` holds, as its list gives them.
    fn pictures_of(ico: &[u8]) -> Vec<Picture> {
        let count = usize::from(u16::from_le_bytes([ico[4], ico[5]]));
        let number = |bytes: &[u8]| usize::try_from(u32::from_le_bytes(bytes.try_into().unwrap())).unwrap();
        let entries = ico[ICO_HEADER..][..ICO_ENTRY * count].as_chunks::<ICO_ENTRY>().0;
        let picture = |entry: &[u8; ICO_ENTRY]| ico[number(&entry[12..])..][..number(&entry[8..12])].to_vec();
        entries.iter().map(|entry| (entry[..8].try_into().unwrap(), picture(entry))).collect()
    }

    /// Each resource in a `.res`, in order, failing unless each has a 32-byte header, starts on a four-byte boundary
    /// and is padded with zeros to the next.
    fn read_back(res: &[u8]) -> Vec<Resource> {
        let mut resources = Vec::new();
        let mut at = 0;
        while at < res.len() {
            assert_eq!(at % 4, 0, "a resource starts at {at}");
            let word = |offset: usize| u16::from_le_bytes(res[at + offset..][..2].try_into().unwrap());
            let size = usize::try_from(u32::from_le_bytes(res[at..][..4].try_into().unwrap())).unwrap();
            assert_eq!(res[at + 4..][..4], HEADER.to_le_bytes(), "the header's length, at {at}");
            assert_eq!((word(8), word(12)), (0xFFFF, 0xFFFF), "a type or a name at {at} isn't a number");
            let data = Data(res[at + 32..][..size].to_vec());
            resources.push(Resource { type_id: word(10), id: word(14), language: word(22), data });
            let end = at + 32 + size;
            at = end.next_multiple_of(4);
            let padding = res.get(end..at).expect("the last resource isn't padded");
            assert_eq!(padding, vec![0; at - end], "the padding after {end}");
        }
        resources
    }

    /// What an icon of these pictures compiles to: the empty resource, each picture as one of an icon's, numbered from
    /// 1, and the icon, as resource 1, listing them by those numbers. Each is neutral.
    fn compiled(pictures: &[Picture]) -> Vec<Resource> {
        let resource = |type_id, id, data| Resource { type_id, id, language: 0, data: Data(data) };
        let mut list = ICO_START.to_vec();
        list.extend(u16::try_from(pictures.len()).unwrap().to_le_bytes());
        let mut resources = vec![resource(0, 0, Vec::new())];
        for (id, (entry, png)) in (1u16..).zip(pictures) {
            list.extend(entry);
            list.extend(u32::try_from(png.len()).unwrap().to_le_bytes());
            list.extend(id.to_le_bytes());
            resources.push(resource(3, id, png.clone()));
        }
        resources.push(resource(14, 1, list));
        resources
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn an_icon_compiles_to_its_pictures_and_a_list_of_them_by_number(
            pictures in vec((any::<[u8; 8]>(), vec(any::<u8>(), 0..40)), 1..6),
        ) {
            let res = from_ico(&ico(&pictures)).unwrap();

            prop_assert_eq!(read_back(&res), compiled(&pictures));
        }
    }

    #[test]
    fn the_apps_own_icon_compiles_as_any_icon_does() {
        let pictures = pictures_of(APP_ICON);

        let res = from_ico(APP_ICON).unwrap();

        assert!(ico(&pictures) == APP_ICON, "it isn't laid out as the law's icons are, so the law doesn't cover it");
        assert_eq!(read_back(&res), compiled(&pictures));
    }

    #[test]
    fn a_file_that_isnt_a_whole_icon_is_refused() {
        let whole = ico(&[([16, 16, 0, 0, 1, 0, 32, 0], vec![1, 2, 3])]);

        assert_eq!(from_ico(b"\x89PNG\r\n\x1a\n"), Err(NotAnIcon::Header));
        assert_eq!(from_ico(&ico(&[])), Err(NotAnIcon::NoPictures));
        for cut in [3, ICO_HEADER + 4, whole.len() - 1] {
            assert_eq!(from_ico(&whole[..cut]), Err(NotAnIcon::CutShort), "cut to {cut} bytes");
        }
    }
}
