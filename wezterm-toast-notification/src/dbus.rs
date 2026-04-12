//! See <https://developer.gnome.org/notification-spec/>

#![cfg(all(not(target_os = "macos"), not(windows)))]

use crate::ToastNotification;
use futures_util::stream::{abortable, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::proxy;
use zvariant::{Type, Value};

#[derive(Debug, Type, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ServerInformation {
    /// The product name of the server.
    pub name: String,

    /// The vendor name. For example "KDE," "GNOME," "freedesktop.org" or "Microsoft".
    pub vendor: String,

    /// The server's version number.
    pub version: String,

    /// The specification version the server is compliant with.
    pub spec_version: String,
}

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    /// Get server information.
    ///
    /// This message returns the information on the server.
    fn get_server_information(&self) -> zbus::Result<ServerInformation>;

    /// GetCapabilities method
    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;

    /// CloseNotification method
    fn close_notification(&self, nid: u32) -> zbus::Result<()>;

    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: &HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, nid: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, nid: u32, reason: u32) -> zbus::Result<()>;
}

/// Timeout/expiration was reached
const REASON_EXPIRED: u32 = 1;
/// User dismissed it
const REASON_USER_DISMISSED: u32 = 2;
/// CloseNotification was called with the nid
const REASON_CLOSE_NOTIFICATION: u32 = 3;

#[derive(Debug)]
enum Reason {
    Expired,
    Dismissed,
    Closed,
    #[allow(dead_code)]
    Unknown(u32),
}

impl Reason {
    fn new(n: u32) -> Self {
        match n {
            REASON_EXPIRED => Self::Expired,
            REASON_USER_DISMISSED => Self::Dismissed,
            REASON_CLOSE_NOTIFICATION => Self::Closed,
            _ => Self::Unknown(n),
        }
    }
}

/// Show notification and listen for events in a single connection.
/// Returns the notification ID immediately via the channel, then continues
/// listening for click/close events in the background.
async fn show_and_listen(
    notif: ToastNotification,
    tx: std::sync::mpsc::SyncSender<Option<u32>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::ConnectionBuilder::session()?.build().await?;
    let proxy = NotificationsProxy::new(&connection).await?;
    let caps = proxy.get_capabilities().await?;

    if notif.url.is_some() && !caps.iter().any(|cap| cap == "actions") {
        let _ = tx.send(None);
        return Ok(());
    }

    let mut hints = HashMap::new();
    hints.insert("urgency", Value::U8(1 /* Normal */));
    // Request notification to stay in notification center after timeout
    // (supported by KDE Plasma, GNOME, and other compliant servers)
    hints.insert("resident", Value::Bool(true));
    // Ensure notification is saved to history/notification list
    hints.insert("transient", Value::Bool(false));
    // KDE-specific: link to desktop entry for theming and grouping
    hints.insert("desktop-entry", Value::Str("org.wezfurlong.wezterm".into()));
    // KDE-specific: event ID for user-configurable notification settings
    // Users can configure behavior in System Settings > Notifications > Applications
    if let Some(event_id) = &notif.event_id {
        hints.insert("x-kde-eventId", Value::Str(event_id.as_str().into()));
    }

    // Build actions list: always include "default" for click-to-focus
    let actions: Vec<&str> = if notif.url.is_some() {
        vec!["default", "Focus", "show", "Show"]
    } else if caps.iter().any(|cap| cap == "actions") {
        vec!["default", "Focus"]
    } else {
        vec![]
    };

    let notification = proxy
        .notify(
            "wezterm",
            0,
            "org.wezfurlong.wezterm",
            &notif.title,
            &notif.message,
            &actions,
            &hints,
            notif.timeout.map(|d| d.as_millis() as _).unwrap_or(0),
        )
        .await?;

    // Send the notification ID back immediately
    let _ = tx.send(Some(notification));

    // Now listen for events while keeping the connection alive
    let invoked_stream = proxy.receive_action_invoked().await?;
    let closed_stream = proxy.receive_notification_closed().await?;

    let (mut invoked_stream, abort_invoked) = abortable(invoked_stream);
    let (mut closed_stream, abort_closed) = abortable(closed_stream);

    let notif_url = notif.url;

    let _ = futures_util::try_join!(
        async {
            while let Some(signal) = invoked_stream.next().await {
                if let Ok(args) = signal.args() {
                    if args.nid == notification {
                        match args.action_key.as_str() {
                            "default" => {
                                crate::invoke_on_click(notification);
                                abort_closed.abort();
                                break;
                            }
                            "show" => {
                                if let Some(url) = notif_url.as_ref() {
                                    wezterm_open_url::open_url(url);
                                }
                                abort_closed.abort();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok::<(), zbus::Error>(())
        },
        async {
            while let Some(signal) = closed_stream.next().await {
                if let Ok(args) = signal.args() {
                    if args.nid == notification {
                        abort_invoked.abort();
                        break;
                    }
                }
            }
            Ok(())
        }
    );

    Ok(())
}

pub fn show_notif(notif: ToastNotification) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);

    std::thread::spawn(move || {
        let res = async_io::block_on(show_and_listen(notif, tx));
        if let Err(err) = res {
            log::error!("while showing notification: {:#}", err);
        }
    });

    // Wait for the notification ID with a short timeout
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(nid) => Ok(nid),
        Err(_) => Ok(None),
    }
}

pub fn close_notif(nid: u32) {
    std::thread::spawn(move || {
        let res = async_io::block_on(async move {
            let connection = zbus::ConnectionBuilder::session()?.build().await?;
            let proxy = NotificationsProxy::new(&connection).await?;
            proxy.close_notification(nid).await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });
        if let Err(err) = res {
            log::error!("while closing notification: {:#}", err);
        }
    });
}
