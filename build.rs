//! Embeds the app icon and version info into the Windows `.exe`, and rebuilds when the
//! interface texts change.
//!
//! The icon is embedded only when building ON Windows: it needs the Windows resource
//! compiler, and checking the code for Windows from another system (`cargo check --target
//! ...`) neither has nor needs it.
//! A failure here is a warning, never a build error: the app works without the file icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/cuantoquedapp.ico");
    // The interface texts are read at compile time by `rust_i18n::i18n!`, which does not tell
    // cargo about them: without this, an edited translation would not reach the binary.
    println!("cargo:rerun-if-changed=locales/app.yml");
    #[cfg(windows)]
    embed_windows_resources();
}

#[cfg(windows)]
fn embed_windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resources = winresource::WindowsResource::new();
    resources.set_icon("assets/cuantoquedapp.ico");
    resources.set("ProductName", "CuantoQuedapp");
    resources.set("FileDescription", "CuantoQuedapp");
    if let Err(error) = resources.compile() {
        println!("cargo:warning=could not embed the Windows icon: {error}");
    }
}
