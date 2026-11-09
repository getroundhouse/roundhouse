//! Starts the app on an iPhone. What it shows is all in the library, which also builds on the Mac.

#[cfg(target_os = "ios")]
fn main() {
    use std::path::PathBuf;
    use std::sync::Arc;

    use gpui_kit::assets::Assets;
    use mobile::DeviceVault;

    // The app's container. iOS gives every app its own, and backs this directory up.
    let data_dir = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("Library/Application Support");
    let vault = Arc::new(DeviceVault::new(&data_dir));
    let (links, opened) = flume::unbounded();
    let application = gpui_kit::Application::with_platform(gpui_ios::current_platform(false)).with_assets(Assets);
    application.on_open_urls(move |urls| {
        for url in urls {
            let _ = links.send(url);
        }
    });
    application.run(move |cx| {
        gpui_kit::init(cx);
        gpui_ios::ios::set_status_bar_style(gpui_ios::StatusBarContentStyle::Light);
        if let Err(e) = mobile::open_window(vault, opened, gpui_kit::WindowOptions::default(), cx) {
            eprintln!("couldn't open the window: {e:#}");
        }
    });
}

#[cfg(not(target_os = "ios"))]
fn main() {
    eprintln!("roundhouse-ios runs on an iPhone. apps/mobile-ios/README.md says how to build it.");
    std::process::exit(1);
}
