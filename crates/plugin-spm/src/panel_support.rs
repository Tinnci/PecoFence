//! Shared helpers for SPM `PanelInstance` implementations.

use crate::board::text::HostMeasure;
use pecofence_plugin_api::{
    Capability, RenderService, TextSpec, design::ContentScale, scene::SceneNode,
};
use std::time::{Duration, Instant};

pub(crate) fn scale_nodes(nodes: &mut [SceneNode], scale: ContentScale) {
    for node in nodes {
        node.rect = scale.rect(node.rect);
        node.style.size *= scale.factor();
        node.style.radius *= scale.factor();
    }
}

pub(crate) fn warmup_wake_after(
    stopped: bool,
    chrome_warmup_until: Instant,
    published_chrome: bool,
    refresh_due: impl FnOnce() -> Option<Duration>,
) -> Option<Duration> {
    if stopped {
        return None;
    }
    if Instant::now() < chrome_warmup_until {
        return Some(Duration::from_millis(500));
    }
    if !published_chrome {
        return Some(Duration::from_millis(10));
    }
    refresh_due()
}

pub(crate) fn with_host_measure<R>(
    render: Capability<dyn RenderService>,
    f: impl FnOnce(&HostMeasure<'_>) -> R,
) -> R {
    let measure = move |spec: &TextSpec, width: f32| {
        render
            .with(|r| r.measure(spec, width))
            .ok()
            .map(|m| (m.width, m.height))
    };
    f(&HostMeasure(&measure))
}
