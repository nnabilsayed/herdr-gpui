//! A deduplicated Dock (macOS) or taskbar (Windows) attention count across all
//! windows, with a QA preview.

use crate::endpoint::Endpoint;
use gpui::{App, Global, WindowId};
use herdr_client::ConnectTarget;
use herdr_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
struct Contribution {
    snapshots: Vec<(Option<String>, Arc<ClientShellSnapshot>)>,
}

impl Contribution {
    fn update<'a>(
        &mut self,
        snapshots: impl Iterator<Item = (Option<&'a str>, &'a Arc<ClientShellSnapshot>)> + Clone,
    ) -> bool {
        // Surface-only updates keep the same Arc. Do not rescan agents every tick.
        if self
            .snapshots
            .iter()
            .map(|(host, snapshot)| (host.as_deref(), Arc::as_ptr(snapshot)))
            .eq(snapshots
                .clone()
                .map(|(host, snapshot)| (host, Arc::as_ptr(snapshot))))
        {
            return false;
        }
        self.snapshots = snapshots
            .map(|(host, snapshot)| (host.map(str::to_owned), snapshot.clone()))
            .collect();
        true
    }
}

#[derive(Default)]
struct Badge {
    windows: HashMap<WindowId, Contribution>,
    published: Option<usize>,
    preview: bool,
    /// Native window handles, which Windows badges one taskbar button at a time.
    #[cfg_attr(not(windows), allow(dead_code))]
    native: HashMap<WindowId, isize>,
}

impl Global for Badge {}

impl Badge {
    fn publish(&mut self) {
        // Snapshot revisions are client-local. Agent state sequences are shared;
        // for the same sequence, an acknowledgement wins over a stale Done dot.
        let mut agents = HashMap::new();
        for (host, snapshot) in self.windows.values().flat_map(|window| &window.snapshots) {
            for agent in &snapshot.agents {
                let attention =
                    matches!(agent.agent_status, AgentStatus::Done | AgentStatus::Blocked);
                let current = agents
                    .entry((
                        host.as_deref(),
                        snapshot.boot_id.as_str(),
                        agent.pane_id.as_str(),
                    ))
                    .or_insert((agent.state_change_seq, attention));
                if agent.state_change_seq > current.0 {
                    *current = (agent.state_change_seq, attention);
                } else if agent.state_change_seq == current.0 {
                    current.1 &= attention;
                }
            }
        }
        let count = agents
            .values()
            .filter(|(_, attention)| *attention)
            .count()
            .max(if self.preview { 2 } else { 0 });
        if self.published != Some(count) {
            set_native(count, &self.native);
            self.published = Some(count);
        }
    }
}

pub(super) fn install(cx: &mut App) {
    cx.default_global::<Badge>().publish();
    cx.on_action(|action: &crate::actions::SetBadgePreview, cx| {
        let badge = cx.default_global::<Badge>();
        badge.preview = action.enabled;
        badge.publish();
    });
    cx.on_window_closed(|cx, _| {
        let open = cx.windows();
        let badge = cx.default_global::<Badge>();
        badge
            .windows
            .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        badge
            .native
            .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        badge.publish();
    })
    .detach();
}

pub(super) fn sync(window: WindowId, endpoints: &[Endpoint], cx: &mut App) {
    let badge = cx.default_global::<Badge>();
    let changed = badge.windows.entry(window).or_default().update(
        endpoints
            .iter()
            .filter(|endpoint| endpoint.enabled && endpoint.live.status.is_connected())
            .filter_map(|endpoint| {
                let host = match &endpoint.connection.target {
                    ConnectTarget::Ssh { target, .. } => Some(target.as_str()),
                    ConnectTarget::Wsl { distro, .. } => Some(distro.as_str()),
                    _ => None,
                };
                endpoint
                    .live
                    .snapshot
                    .as_ref()
                    .map(|snapshot| (host, snapshot))
            }),
    );
    if changed {
        badge.publish();
    }
}

/// Remembers this window's taskbar button and badges it if agents already wait.
#[cfg(windows)]
pub(super) fn register_window(window: &gpui::Window, cx: &mut App) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // The trait method, not `Window::window_handle`, which returns GPUI's own handle.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get();
    let badge = cx.default_global::<Badge>();
    if badge
        .native
        .insert(window.window_handle().window_id(), hwnd)
        != Some(hwnd)
        && let Some(count) = badge.published.filter(|count| *count > 0)
    {
        set_native(
            count,
            &HashMap::from([(window.window_handle().window_id(), hwnd)]),
        );
    }
}

#[cfg(all(target_os = "macos", not(test)))]
fn set_native(count: usize, _: &HashMap<WindowId, isize>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

#[cfg(all(windows, not(test)))]
fn set_native(count: usize, windows: &HashMap<WindowId, isize>) {
    for hwnd in windows.values() {
        taskbar::set(*hwnd, count);
    }
}

// Headless tests must never initialize AppKit or change the test runner's Dock icon.
#[cfg(test)]
fn set_native(_: usize, _: &HashMap<WindowId, isize>) {}

#[cfg(windows)]
mod taskbar;

#[cfg(all(target_os = "macos", feature = "integration-test"))]
pub(super) fn verify_native() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let main_thread = MainThreadMarker::new().context("badge check requires the main thread")?;
    let tile = NSApplication::sharedApplication(main_thread).dockTile();
    let previous = tile.badgeLabel();
    for count in [2, 1, 0] {
        set_native(count, &HashMap::new());
        let actual = tile.badgeLabel().map(|label| label.to_string());
        let valid = if count > 0 {
            actual.as_deref() == Some(count.to_string().as_str())
        } else {
            actual.as_deref().is_none_or(str::is_empty)
        };
        if !valid {
            tile.setBadgeLabel(previous.as_deref());
            anyhow::bail!("native Dock badge did not reflect count={count}");
        }
    }
    tile.setBadgeLabel(previous.as_deref());
    eprintln!("BADGE native PASS: Dock count changed from 2 to 1 and cleared");
    Ok(())
}

#[cfg(test)]
mod tests;
