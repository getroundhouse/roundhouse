//! Which of the copy's configs a target builds with.

/// One of the folders in `third_party/libvpx/config`, each written by libvpx's `configure` for one target, as
/// `tools/libvpx-sync.sh` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Config {
    /// macOS and iOS, whose arm64 has the same extensions.
    Arm64Darwin,
    Arm64Linux,
    /// Without SVE, which MSVC's compiler doesn't have.
    Arm64Windows,
    X64Linux,
    X64Windows,
}

/// A target the copy has no config for, such as an Intel Mac, which the app isn't built for.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error(
    "libvpx has no config for {arch} on {os}. tools/libvpx-sync.sh writes one for each target the app is built for"
)]
pub struct UnsupportedTarget {
    pub arch: String,
    pub os: String,
}

impl Config {
    /// The config for the target Cargo names with `CARGO_CFG_TARGET_ARCH` and `CARGO_CFG_TARGET_OS`.
    pub fn for_target(arch: &str, os: &str) -> Result<Self, UnsupportedTarget> {
        match (arch, os) {
            ("aarch64", "macos" | "ios") => Ok(Config::Arm64Darwin),
            ("aarch64", "linux") => Ok(Config::Arm64Linux),
            ("aarch64", "windows") => Ok(Config::Arm64Windows),
            ("x86_64", "linux") => Ok(Config::X64Linux),
            ("x86_64", "windows") => Ok(Config::X64Windows),
            _ => Err(UnsupportedTarget { arch: arch.to_string(), os: os.to_string() }),
        }
    }

    /// Its folder's name in `third_party/libvpx/config`.
    pub fn folder(self) -> &'static str {
        match self {
            Config::Arm64Darwin => "arm64-darwin",
            Config::Arm64Linux => "arm64-linux",
            Config::Arm64Windows => "arm64-windows",
            Config::X64Linux => "x64-linux",
            Config::X64Windows => "x64-windows",
        }
    }

    /// The object format nasm assembles x86's assembly into, as its `-f` names it. Arm64's extensions are all in C, so
    /// it has none.
    pub fn assembly_format(self) -> Option<&'static str> {
        match self {
            Config::X64Linux => Some("elf64"),
            Config::X64Windows => Some("win64"),
            Config::Arm64Darwin | Config::Arm64Linux | Config::Arm64Windows => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, UnsupportedTarget};

    #[test]
    fn each_target_the_app_is_built_for_has_a_config_and_x86_alone_has_assembly() {
        let targets = [
            ("aarch64", "macos", Config::Arm64Darwin, "arm64-darwin", None),
            ("aarch64", "ios", Config::Arm64Darwin, "arm64-darwin", None),
            ("aarch64", "linux", Config::Arm64Linux, "arm64-linux", None),
            ("aarch64", "windows", Config::Arm64Windows, "arm64-windows", None),
            ("x86_64", "linux", Config::X64Linux, "x64-linux", Some("elf64")),
            ("x86_64", "windows", Config::X64Windows, "x64-windows", Some("win64")),
        ];

        for (arch, os, config, folder, format) in targets {
            assert_eq!(Config::for_target(arch, os), Ok(config), "{arch} on {os}");
            assert_eq!(config.folder(), folder);
            assert_eq!(config.assembly_format(), format);
        }
    }

    #[test]
    fn a_target_without_a_config_says_which_it_is() {
        let refused = Config::for_target("x86_64", "macos");

        assert_eq!(refused, Err(UnsupportedTarget { arch: "x86_64".to_string(), os: "macos".to_string() }));
    }
}
