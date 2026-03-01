//! This module provides monitor/screen enumeration for multi-screen presentation support.

use dioxus::desktop::DesktopContext;
use serde::{Deserialize, Serialize};

/// Information about a connected monitor/screen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorInfo {
    /// Index of the monitor in the enumeration order
    pub id: usize,
    /// Human-readable name of the monitor (may be empty on some platforms)
    pub name: String,
    /// Position of the monitor in virtual screen coordinates
    pub position: (i32, i32),
    /// Size of the monitor in physical pixels
    pub size: (u32, u32),
    /// Whether this is the primary monitor
    pub is_primary: bool,
}

/// Enumerates all available monitors using the desktop context.
pub fn enumerate_monitors(desktop: &DesktopContext) -> Vec<MonitorInfo> {
    let primary = desktop.primary_monitor();
    let monitors: Vec<_> = desktop.available_monitors().collect();

    monitors
        .into_iter()
        .enumerate()
        .map(|(id, monitor)| {
            let name = monitor.name().unwrap_or_default();
            let position = monitor.position();
            let size = monitor.size();
            let is_primary = primary
                .as_ref()
                .map(|p| p.name() == monitor.name() && p.position() == monitor.position())
                .unwrap_or(false);

            MonitorInfo {
                id,
                name,
                position: (position.x, position.y),
                size: (size.width, size.height),
                is_primary,
            }
        })
        .collect()
}

/// Resolves which monitor to use for presentation based on settings.
/// If `configured_name` is Some, tries to find a monitor with that name.
/// Otherwise, prefers a non-primary monitor (for presentation) or primary monitor (for presenter console).
pub fn resolve_monitor(
    monitors: &[MonitorInfo],
    configured_name: &Option<String>,
    prefer_primary: bool,
) -> Option<MonitorInfo> {
    if monitors.is_empty() {
        return None;
    }

    // If a specific monitor is configured, try to find it
    if let Some(name) = configured_name {
        if let Some(monitor) = monitors.iter().find(|m| &m.name == name) {
            return Some(monitor.clone());
        }
    }

    // Auto-select: prefer primary or non-primary based on the flag
    if prefer_primary {
        monitors
            .iter()
            .find(|m| m.is_primary)
            .or(monitors.first())
            .cloned()
    } else {
        monitors
            .iter()
            .find(|m| !m.is_primary)
            .or(monitors.first())
            .cloned()
    }
}
