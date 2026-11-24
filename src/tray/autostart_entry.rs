//! The "start at login" menu entry. Disabled, with the reason, when the app is not installed.
use rust_i18n::t;
use tray_icon::menu::CheckMenuItem;

use crate::{
    autostart::{self, Autostart, Eligibility},
    i18n::Lang,
    panel_view::LoginItem,
};

pub(super) struct AutostartEntry {
    pub(super) item: CheckMenuItem,
    autostart: Option<Autostart>,
}

impl AutostartEntry {
    pub(super) fn new(lang: Lang) -> Self {
        let exe = std::env::current_exe()
            .ok()
            .filter(|exe| autostart::eligibility(exe) == Eligibility::Allowed);
        let autostart = exe
            .as_deref()
            .and_then(|exe| Autostart::new(autostart::APP_NAME, exe).ok());
        if let (Some(autostart), Some(exe)) = (&autostart, &exe) {
            if let Ok(previous) = Autostart::new(crate::legacy::OLD_NAME, exe) {
                autostart.take_over(&previous);
            }
            autostart.refresh_if_enabled();
        }
        let checked = autostart.as_ref().is_some_and(Autostart::is_enabled);
        let item = CheckMenuItem::new("", autostart.is_some(), checked, None);
        let entry = Self { item, autostart };
        entry.relabel(lang);
        entry
    }

    /// Names the entry in `lang`, saying why it is disabled when it is.
    pub(super) fn relabel(&self, lang: Lang) {
        let locale = lang.code();
        self.item.set_text(if self.autostart.is_some() {
            t!("autostart.label", locale = locale)
        } else {
            t!("autostart.menu_unavailable", locale = locale)
        });
    }

    pub(super) fn login_item(&self) -> LoginItem {
        match &self.autostart {
            None => LoginItem::Unavailable,
            Some(autostart) if autostart.is_enabled() => LoginItem::On,
            Some(_) => LoginItem::Off,
        }
    }

    /// Flips the system setting, then shows what the system really says
    /// (the menu toggles the check by itself, which may not match a failed change).
    pub(super) fn toggle(&self) {
        let Some(autostart) = &self.autostart else {
            return;
        };
        if let Err(error) = autostart.set(!autostart.is_enabled()) {
            eprintln!("Could not change start at login ({error})");
        }
        self.item.set_checked(autostart.is_enabled());
    }
}
