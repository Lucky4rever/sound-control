use tray_icon::TrayIconBuilder;

pub struct SystemTray {
    pub _inner: tray_icon::TrayIcon,
}

impl SystemTray {
    pub fn init(icon_path: &std::path::Path) -> anyhow::Result<Self> {
        let image = image::open(icon_path)?
            .into_rgba8();

        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        let icon = tray_icon::Icon::from_rgba(rgba, width, height)?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("Регулятор звуку")
            .with_icon(icon)
            .build()?;

        Ok(Self { _inner: tray_icon })
    }
}