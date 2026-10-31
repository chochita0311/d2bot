//! Capture selection and provenance are independent of the native implementation.
mod latest;
pub use latest::{ControlCapture, LatestCapture};

use super::{
    config::{BotConfig, ConfigError},
    error::ControlError,
    frame::PixelRect,
    frame_geometry::{FrameGeometry, PhysicalTarget, ReferenceLayout},
    realtime::{CapturedFrame, ScreenSource},
    vision_ops::resize::normalize,
};
use crate::platform::{CaptureBackend, WindowInfo};
use serde::Deserialize;
use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CaptureSettings {
    pub monitor_index: i64,
    pub region: Option<CaptureRegion>,
    pub window_title: Option<String>,
    pub window_title_mode: String,
    pub follow_window: bool,
    pub capture_backend: String,
    pub reference_layout: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureRegion {
    pub left: i32,
    pub top: i32,
    pub width: usize,
    pub height: usize,
}
impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            monitor_index: 1,
            region: None,
            window_title: None,
            window_title_mode: "contains".into(),
            follow_window: true,
            capture_backend: "auto".into(),
            reference_layout: None,
        }
    }
}
impl CaptureSettings {
    pub fn from_config(config: &BotConfig) -> Result<Self, ConfigError> {
        let value = config
            .section("capture")
            .ok_or_else(|| ConfigError::value("capture settings missing"))?;
        serde_json::from_str(&value.to_string())
            .map_err(|e| ConfigError::value(format!("native capture settings: {e}")))
    }
    fn named(&self) -> Option<&str> {
        self.window_title
            .as_deref()
            .filter(|title| !title.is_empty())
    }
}

pub fn find_window(windows: Vec<WindowInfo>, title: &str, mode: &str) -> Option<WindowInfo> {
    let title = super::unicode_casefold::casefold(title);
    windows.into_iter().find(|window| {
        let current = super::unicode_casefold::casefold(&window.title);
        if mode == "exact" {
            current == title
        } else {
            current.contains(&title)
        }
    })
}
fn required_window<B: CaptureBackend>(
    backend: &B,
    settings: &CaptureSettings,
) -> Result<WindowInfo, ControlError> {
    let title = settings
        .named()
        .ok_or_else(|| ConfigError::value("named window required"))?;
    find_window(backend.windows()?, title, &settings.window_title_mode).ok_or_else(|| {
        ControlError::Runtime(format!(
            "Could not find window with title {} '{}'.",
            settings.window_title_mode, title
        ))
    })
}
pub struct ScreenCapture<B: CaptureBackend> {
    backend: B,
    settings: CaptureSettings,
    target: PhysicalTarget,
    window: Option<WindowInfo>,
    layout: Option<ReferenceLayout>,
    geometry: Option<Arc<FrameGeometry>>,
    closed: bool,
    last_method: Option<&'static str>,
}
impl<B: CaptureBackend> ScreenCapture<B> {
    /// Layout references stay repository-relative, as in the Python module.
    pub fn new(
        mut backend: B,
        settings: CaptureSettings,
        project_root: &Path,
    ) -> Result<Self, ControlError> {
        let layout = settings
            .reference_layout
            .as_deref()
            .filter(|path| !path.is_empty())
            .map(|path| {
                if settings.named().is_none() {
                    return Err(ConfigError::value(
                        "reference layout requires a named game window",
                    )
                    .into());
                }
                let path = Path::new(path);
                ReferenceLayout::load(if path.is_absolute() {
                    path.to_owned()
                } else {
                    project_root.join(path)
                })
                .map_err(|e| ControlError::Configuration(ConfigError::new(e.kind(), e.message())))
            })
            .transpose();
        let layout = match layout {
            Ok(layout) => layout,
            Err(error) => {
                return Err(match backend.close() {
                    Ok(()) => error,
                    Err(cleanup) => {
                        ControlError::Runtime(format!("{error}; capture close failed: {cleanup}"))
                    }
                });
            }
        };
        Self::with_layout(backend, settings, layout)
    }
    pub fn with_layout(
        mut backend: B,
        settings: CaptureSettings,
        layout: Option<ReferenceLayout>,
    ) -> Result<Self, ControlError> {
        let resolved = (|| {
            if layout.is_some() && settings.named().is_none() {
                return Err(
                    ConfigError::value("reference layout requires a named game window").into(),
                );
            }
            let window = if settings.named().is_some() {
                Some(required_window(&backend, &settings)?)
            } else {
                None
            };
            let target = if let Some(window) = &window {
                window.target
            } else if let Some(region) = &settings.region {
                PhysicalTarget::new(region.left, region.top, region.width, region.height)
                    .map_err(|e| ConfigError::value(e.to_string()))?
            } else {
                let monitors = backend.monitors()?;
                let index = if settings.monitor_index < 0 {
                    monitors.len() as i128 + i128::from(settings.monitor_index)
                } else {
                    i128::from(settings.monitor_index)
                };
                usize::try_from(index)
                    .ok()
                    .and_then(|index| monitors.get(index).copied())
                    .ok_or_else(|| ControlError::Runtime("monitor index out of range".into()))?
            };
            Ok((window, target))
        })();
        let (window, target) = match resolved {
            Ok(value) => value,
            Err(error) => {
                return Err(match backend.close() {
                    Ok(()) => error,
                    Err(cleanup) => {
                        ControlError::Runtime(format!("{error}; capture close failed: {cleanup}"))
                    }
                });
            }
        };
        Ok(Self {
            backend,
            settings,
            target,
            window,
            layout,
            geometry: None,
            closed: false,
            last_method: None,
        })
    }
    pub fn target(&self) -> PhysicalTarget {
        self.target
    }
    pub fn geometry(&self) -> Option<&Arc<FrameGeometry>> {
        self.geometry.as_ref()
    }
    /// 포커스 검사가 있는 live 제어의 auto는 창 재렌더링 없이 현재 화면을 읽는다.
    pub fn prefer_focused_screen(&mut self) {
        if self.settings.named().is_some() && self.settings.capture_backend == "auto" {
            self.settings.capture_backend = "screen".into();
        }
    }
    pub fn configured_window(&self) -> Result<Option<WindowInfo>, ControlError> {
        self.settings
            .named()
            .map(|title| {
                self.backend
                    .windows()
                    .map(|windows| find_window(windows, title, &self.settings.window_title_mode))
            })
            .transpose()
            .map(Option::flatten)
    }
    pub fn current_target(&self) -> Result<PhysicalTarget, ControlError> {
        if self.settings.named().is_some() {
            Ok(required_window(&self.backend, &self.settings)?.target)
        } else {
            Ok(self.target)
        }
    }
    pub fn current_client_box(&self) -> Result<PixelRect, ControlError> {
        if self.layout.is_none() {
            Ok(PixelRect::full(self.target.size()))
        } else {
            self.backend
                .client_box(&required_window(&self.backend, &self.settings)?)
        }
    }
    pub fn grab(&mut self) -> Result<CapturedFrame, ControlError> {
        self.last_method = None;
        if self.closed {
            return Err(ControlError::Runtime("capture is closed".into()));
        }
        if self.settings.named().is_some() && self.settings.follow_window {
            let window = required_window(&self.backend, &self.settings)?;
            self.target = window.target;
            self.window = Some(window);
        }
        let client = self.current_client_box()?;
        let mut frame = None;
        if let Some(window) = &self.window
            && matches!(self.settings.capture_backend.as_str(), "auto" | "window")
        {
            frame = self.backend.capture_window(window)?;
            if frame.is_none() && self.settings.capture_backend == "window" {
                return Err(ControlError::Runtime(
                    "Named window capture failed for the target window.".into(),
                ));
            }
        }
        let frame = match frame {
            Some(frame) => {
                self.last_method = Some("window");
                frame
            }
            None => {
                let frame = self.backend.capture_screen(self.target)?;
                self.last_method = Some("screen");
                frame
            }
        };
        let geometry = match &self.geometry {
            Some(geometry)
                if geometry.target() == self.target && geometry.source_client() == client =>
            {
                geometry.clone()
            }
            _ => Arc::new(
                FrameGeometry::create(self.target, self.layout.clone(), Some(client))
                    .map_err(|e| ControlError::Runtime(e.to_string()))?,
            ),
        };
        if self.current_target()? != self.target || self.current_client_box()? != client {
            return Err(ControlError::Runtime(
                "window geometry changed during capture".into(),
            ));
        }
        let frame =
            match normalize(&geometry, &frame).map_err(|e| ControlError::Runtime(e.to_string()))? {
                std::borrow::Cow::Borrowed(_) => frame,
                std::borrow::Cow::Owned(normalized) => normalized,
            };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ControlError::Runtime(e.to_string()))?
            .as_secs_f64();
        self.geometry = Some(geometry.clone());
        Ok(CapturedFrame {
            frame: Arc::new(frame),
            target: self.target,
            geometry: Some(geometry),
            wall_captured_at: Some(timestamp),
        })
    }
    pub fn close(&mut self) -> Result<(), ControlError> {
        if !self.closed {
            self.backend.close()?;
            self.closed = true;
        }
        Ok(())
    }
}
impl<B: CaptureBackend> ScreenSource for ScreenCapture<B> {
    fn capture_method(&self) -> Option<&'static str> {
        self.last_method
    }
    fn grab(&mut self) -> Result<CapturedFrame, ControlError> {
        Self::grab(self)
    }
    fn close(&mut self) -> Result<(), ControlError> {
        Self::close(self)
    }
}
impl<B: CaptureBackend> Drop for ScreenCapture<B> {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.close());
    }
}
