mod dbus;
mod macos;
mod windows;

use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone)]
pub struct ToastNotification {
    pub title: String,
    pub message: String,
    pub url: Option<String>,
    pub timeout: Option<std::time::Duration>,
}

impl ToastNotification {
    pub fn show(self) -> Option<u32> {
        show(self)
    }
}

/// Global callback for notification click events.
/// The callback receives the notification ID.
static ON_CLICK_CALLBACK: LazyLock<Mutex<Option<Box<dyn Fn(u32) + Send + Sync>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Register a callback to be invoked when a notification is clicked.
/// Only one callback can be registered at a time.
pub fn set_on_click_callback<F>(callback: F)
where
    F: Fn(u32) + Send + Sync + 'static,
{
    *ON_CLICK_CALLBACK.lock().unwrap() = Some(Box::new(callback));
}

/// Called internally when a notification is clicked
pub(crate) fn invoke_on_click(nid: u32) {
    if let Some(callback) = ON_CLICK_CALLBACK.lock().unwrap().as_ref() {
        callback(nid);
    }
}

#[cfg(windows)]
use crate::windows as backend;
#[cfg(all(not(target_os = "macos"), not(windows)))]
use dbus as backend;
#[cfg(target_os = "macos")]
use macos as backend;

mod nop {
    use super::*;

    #[allow(dead_code)]
    pub fn show_notif(_: ToastNotification) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        Ok(None)
    }

    #[allow(dead_code)]
    pub fn close_notif(_nid: u32) {}
}

pub fn show(notif: ToastNotification) -> Option<u32> {
    match backend::show_notif(notif) {
        Ok(nid) => nid,
        Err(err) => {
            log::error!("Failed to show notification: {}", err);
            None
        }
    }
}

/// Close a notification by its ID (Linux/D-Bus only)
#[cfg(all(not(target_os = "macos"), not(windows)))]
pub fn close(nid: u32) {
    backend::close_notif(nid);
}

#[cfg(any(target_os = "macos", windows))]
pub fn close(_nid: u32) {
    // Not implemented for macOS/Windows
}

pub fn persistent_toast_notification_with_click_to_open_url(title: &str, message: &str, url: &str) {
    show(ToastNotification {
        title: title.to_string(),
        message: message.to_string(),
        url: Some(url.to_string()),
        timeout: None,
    });
}

pub fn persistent_toast_notification(title: &str, message: &str) -> Option<u32> {
    show(ToastNotification {
        title: title.to_string(),
        message: message.to_string(),
        url: None,
        // Auto-hide after 30 seconds (notification goes to notification list, not closed)
        timeout: Some(std::time::Duration::from_secs(30)),
    })
}

#[cfg(target_os = "macos")]
pub use macos::initialize as macos_initialize;
