#[cfg(windows)]
fn main() -> std::io::Result<()> {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("icons/icon.ico");
    res.compile()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
