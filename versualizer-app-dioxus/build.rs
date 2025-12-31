#[cfg(windows)]
fn main() -> std::io::Result<()> {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("icons/icon.ico");
    // Include manifest for ComCtl32.dll v6 (enables custom button labels in dialogs)
    res.set_manifest_file("app.manifest");
    res.compile()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
