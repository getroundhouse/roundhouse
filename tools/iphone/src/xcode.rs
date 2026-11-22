//! Packaging the Rust binary as the app with Xcode, and signing it. `apps/mobile-ios/ios/project.yml` describes an app
//! whose only source is a stub, which `XcodeGen` makes the Xcode project from on each build, and whose last build phase
//! puts the Rust binary in the stub's place, before Xcode signs it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};

use crate::cargo::{self, Profile};
use crate::connect::Key;
use crate::program;
use crate::stamp::{BuildNumber, Version};

/// The Apple Developer team that signs the app, as its ten-character ID.
pub(crate) struct TeamId(String);

impl TeamId {
    pub(crate) fn new(id: String) -> anyhow::Result<Self> {
        if id.len() != 10 || !id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            bail!("a team ID is ten capital letters and digits");
        }
        Ok(Self(id))
    }
}

/// The app's identifier in its team, such as `com.example.app`.
pub(crate) struct BundleId(String);

impl BundleId {
    pub(crate) fn new(id: String) -> anyhow::Result<Self> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-';
        if id.is_empty() || !id.chars().all(allowed) {
            bail!("a bundle identifier is letters, digits, dots and hyphens");
        }
        Ok(Self(id))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The app as one team signs it.
pub(crate) struct App {
    pub(crate) team: TeamId,
    pub(crate) bundle: BundleId,
}

/// A phone, as `xcrun devicectl list devices` names it.
pub(crate) struct DeviceId(String);

impl DeviceId {
    pub(crate) fn new(id: String) -> Self {
        Self(id)
    }
}

/// Builds the app, signs it for development, and, if `open`, installs and opens it on `device`. Building for the phone
/// itself, rather than any iOS device, is what lets Xcode add it to the development profile.
pub(crate) fn install(root: &Path, app: &App, device: &DeviceId, profile: Profile, open: bool) -> anyhow::Result<()> {
    let binary = cargo::compile(root, profile)?;
    let project = Project::generate(root, &app.bundle)?;
    let configuration = profile.configuration();
    let bundle = project.derived_data().join(format!("Build/Products/{configuration}-iphoneos/roundhouse-ios.app"));
    // The last build phase copies the binary over the stub on every build, but Xcode signs again only when it thinks
    // the bundle changed, so a rebuild would leave the new binary unsigned. With no bundle it signs again.
    if bundle.exists() {
        fs::remove_dir_all(&bundle).with_context(|| format!("couldn't remove {}", bundle.display()))?;
    }
    let build = BuildNumber::at(jiff::Timestamp::now());
    let settings = settings(app, &binary, &Version::for_a_phone(root), &build);
    program::run(
        project
            .xcodebuild("build", configuration)
            .args(["-allowProvisioningUpdates", "-allowProvisioningDeviceRegistration", "-destination"])
            .arg(format!("platform=iOS,id={}", device.0))
            .arg("CODE_SIGN_IDENTITY=Apple Development")
            .args(settings),
    )?;
    println!("Built {}", bundle.display());
    if open {
        program::run(xcrun(["devicectl", "device", "install", "app", "--device", &device.0]).arg(&bundle))?;
        program::run(&mut xcrun([
            "devicectl",
            "device",
            "process",
            "launch",
            "--terminate-existing",
            "--device",
            &device.0,
            app.bundle.as_str(),
        ]))?;
    }
    Ok(())
}

/// Archives `binary` as the app for any iPhone, unsigned, so archiving needs no certificate, into `work`.
pub(crate) fn archive(
    root: &Path,
    app: &App,
    binary: &Path,
    (version, build): (&Version, &BuildNumber),
    work: &Path,
) -> anyhow::Result<PathBuf> {
    let project = Project::generate(root, &app.bundle)?;
    let archive = work.join("Roundhouse.xcarchive");
    program::run(
        project
            .xcodebuild("archive", Profile::Release.configuration())
            .args(["-destination", "generic/platform=iOS", "-archivePath"])
            .arg(&archive)
            .arg("CODE_SIGNING_ALLOWED=NO")
            .args(settings(app, binary, version, build)),
    )?;
    Ok(archive)
}

/// Signs the archive for the App Store and uploads it. Xcode makes the profile, and uses a distribution certificate
/// Apple keeps, through the key.
pub(crate) fn export_and_upload(archive: &Path, team: &TeamId, key: &Key, work: &Path) -> anyhow::Result<()> {
    let options = work.join("ExportOptions.plist");
    fs::write(&options, export_options(team)).with_context(|| format!("couldn't write {}", options.display()))?;
    program::run(
        Command::new("xcodebuild")
            .args(["-exportArchive", "-allowProvisioningUpdates", "-authenticationKeyPath"])
            .arg(key.path())
            .args(["-authenticationKeyID", key.id().as_str(), "-authenticationKeyIssuerID", key.issuer().as_str()])
            .arg("-archivePath")
            .arg(archive)
            .arg("-exportOptionsPlist")
            .arg(&options)
            .arg("-exportPath")
            .arg(work.join("export")),
    )
}

/// For the team's own testers only, since anything further goes through Apple's review. Without symbols, since the
/// archive's are the stub's, not the Rust binary's. With the build number the archive has, not one Xcode picks.
fn export_options(team: &TeamId) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key><string>app-store-connect</string>
	<key>destination</key><string>upload</string>
	<key>signingStyle</key><string>automatic</string>
	<key>teamID</key><string>{}</string>
	<key>testFlightInternalTestingOnly</key><true/>
	<key>uploadSymbols</key><false/>
	<key>manageAppVersionAndBuildNumber</key><false/>
</dict>
</plist>
"#,
        team.0
    )
}

/// The Xcode project `XcodeGen` made for one bundle identifier, and where Xcode builds it.
struct Project {
    app_dir: PathBuf,
}

impl Project {
    fn generate(root: &Path, bundle: &BundleId) -> anyhow::Result<Self> {
        let app_dir = root.join("apps/mobile-ios");
        program::run(
            Command::new("xcodegen")
                .args(["generate", "--quiet"])
                .current_dir(app_dir.join("ios"))
                .env("BUNDLE_ID", &bundle.0),
        )?;
        Ok(Self { app_dir })
    }

    fn derived_data(&self) -> PathBuf {
        self.app_dir.join("build/DerivedData")
    }

    fn xcodebuild(&self, action: &str, configuration: &str) -> Command {
        let mut command = Command::new("xcodebuild");
        command
            .args([action, "-quiet", "-scheme", "Roundhouse", "-configuration", configuration, "-project"])
            .arg(self.app_dir.join("ios/Roundhouse.xcodeproj"))
            .arg("-derivedDataPath")
            .arg(self.derived_data());
        command
    }
}

/// What `project.yml` and `Info.plist` leave to the build: the team, the identifier, the binary, and the version.
fn settings(app: &App, binary: &Path, version: &Version, build: &BuildNumber) -> Vec<String> {
    vec![
        format!("DEVELOPMENT_TEAM={}", app.team.0),
        format!("BUNDLE_ID={}", app.bundle.0),
        format!("RUST_BINARY={}", binary.display()),
        format!("MARKETING_VERSION={version}"),
        format!("CURRENT_PROJECT_VERSION={build}"),
    ]
}

fn xcrun<const N: usize>(args: [&str; N]) -> Command {
    let mut command = Command::new("xcrun");
    command.args(args);
    command
}
