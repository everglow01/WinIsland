use std::path::PathBuf;
use std::time::SystemTime;

use windows::UI::Composition::Visual;
use windows::Win32::Foundation::{COLORREF, E_FAIL, E_INVALIDARG, POINT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BITMAP_PROPERTIES1,
    D2D1_BRUSH_PROPERTIES, D2D1_EXTEND_MODE_WRAP, D2D1_INTERPOLATION_MODE_LINEAR, ID2D1Bitmap1,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionSurfaceFactory, IDCompositionVisual2,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    DESKTOP_WALLPAPER_POSITION, DWPOS_CENTER, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH, DWPOS_TILE,
    DesktopWallpaper, IDesktopWallpaper,
};
use windows::core::{Error, HSTRING, Interface, PWSTR, Result};
use windows_numerics::{Matrix3x2, Vector2};
use winisland_platform::Rect;

use super::interop::Composition;
use super::rect_size;

const MAX_IMAGE_EDGE: u32 = 2048;

pub(super) struct Wallpaper {
    pub(super) visual: IDCompositionVisual2,
    surfaces: IDCompositionSurfaceFactory,
    service: IDesktopWallpaper,
    settings: Option<Settings>,
    image: Option<WallpaperImage>,
    viewport: Option<(Rect, u32)>,
}

#[derive(PartialEq)]
struct Settings {
    path: PathBuf,
    modified: Option<SystemTime>,
    position: DESKTOP_WALLPAPER_POSITION,
    color: COLORREF,
    bounds: Rect,
}

struct WallpaperImage {
    bitmap: ID2D1Bitmap1,
    width: f32,
    height: f32,
}

impl Wallpaper {
    pub(super) fn new(composition: &Composition) -> Result<Self> {
        // SAFETY: The worker apartment is initialized; all interfaces remain on that worker.
        let (visual, surfaces, service) = unsafe {
            (
                composition.native.CreateVisual()?,
                composition
                    .native
                    .CreateSurfaceFactory(&composition.drawing)?,
                CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?,
            )
        };
        Ok(Self {
            visual,
            surfaces,
            service,
            settings: None,
            image: None,
            viewport: None,
        })
    }

    pub(super) fn refresh(&mut self, bounds: Rect, area: Rect, pixel_scale: u32) -> Result<()> {
        let settings = Settings::read(&self.service, bounds)?;
        if self.settings.as_ref() != Some(&settings) {
            if self.settings.as_ref().is_none_or(|previous| {
                previous.path != settings.path || previous.modified != settings.modified
            }) {
                self.image = None;
            }
            self.viewport = None;
            self.settings = Some(settings);
        }
        self.set_area(area, pixel_scale)
    }

    pub(super) fn set_area(&mut self, area: Rect, pixel_scale: u32) -> Result<()> {
        if self.viewport == Some((area, pixel_scale)) {
            return Ok(());
        }
        let Some(settings) = &self.settings else {
            return Ok(());
        };
        let (width, height) =
            rect_size(area).ok_or_else(|| Error::new(E_INVALIDARG, "Invalid wallpaper area"))?;
        let (width, height) = (width.div_ceil(pixel_scale), height.div_ceil(pixel_scale));
        // SAFETY: The factory retains the D2D device; the surface is fully initialized below.
        let surface = unsafe {
            self.surfaces.CreateSurface(
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_IGNORE,
            )?
        };
        let mut offset = POINT::default();
        // SAFETY: The surface and writable offset are live; EndDraw is called on every path.
        let context: ID2D1DeviceContext = unsafe { surface.BeginDraw(None, &mut offset)? };
        // SAFETY: Clip to this surface's atlas allocation before clearing or drawing.
        unsafe {
            context.SetDpi(96.0, 96.0);
            context.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
            context.PushAxisAlignedClip(
                &D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: width as f32,
                    bottom: height as f32,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );
        }
        let result = (|| {
            if self.image.is_none() && !settings.path.as_os_str().is_empty() {
                match WallpaperImage::load(&context, &settings.path) {
                    Ok(image) => self.image = Some(image),
                    Err(error) => log::debug!("Wallpaper image is unavailable: {error}"),
                }
            }
            let color = settings.color.0;
            // SAFETY: BeginDraw supplied the render target. Clear initializes every pixel, and
            // the offset is honored without calling BeginDraw/EndDraw on the D2D context.
            unsafe {
                context.Clear(Some(&D2D1_COLOR_F {
                    r: (color & 255) as f32 / 255.0,
                    g: ((color >> 8) & 255) as f32 / 255.0,
                    b: ((color >> 16) & 255) as f32 / 255.0,
                    a: 1.0,
                }));
            }
            if let Some(image) = &self.image {
                image.draw(&context, settings, area, pixel_scale)?;
            }
            Ok(())
        })();
        // SAFETY: This balances the successful surface BeginDraw, including drawing failures.
        let end = unsafe {
            context.PopAxisAlignedClip();
            surface.EndDraw()
        };
        result.and(end)?;
        let visual: Visual = self.visual.cast()?;
        visual.SetSize(Vector2 {
            X: width as f32,
            Y: height as f32,
        })?;
        // SAFETY: The initialized surface belongs to the visual's device and is retained by it.
        unsafe { self.visual.SetContent(&surface)? };
        self.viewport = Some((area, pixel_scale));
        Ok(())
    }
}

impl Settings {
    fn read(service: &IDesktopWallpaper, monitor: Rect) -> Result<Self> {
        // SAFETY: COM outputs are copied to owned strings and freed before the next operation.
        unsafe {
            let mut id = HSTRING::new();
            let mut desktop = monitor;
            for index in 0..service.GetMonitorDevicePathCount()? {
                let candidate = take_string(service.GetMonitorDevicePathAt(index)?);
                let rect = service.GetMonitorRECT(&candidate)?;
                let bounds = Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                if bounds == monitor {
                    id = candidate;
                }
                desktop.left = desktop.left.min(bounds.left);
                desktop.top = desktop.top.min(bounds.top);
                desktop.right = desktop.right.max(bounds.right);
                desktop.bottom = desktop.bottom.max(bounds.bottom);
            }
            if id.is_empty() {
                return Err(Error::new(E_FAIL, "Wallpaper monitor is unavailable"));
            }
            let path = PathBuf::from(take_string(service.GetWallpaper(&id)?).to_os_string());
            let modified = std::fs::metadata(&path)
                .and_then(|value| value.modified())
                .ok();
            let position = service.GetPosition()?;
            Ok(Self {
                path,
                modified,
                position,
                color: service.GetBackgroundColor()?,
                bounds: if matches!(position, DWPOS_SPAN | DWPOS_TILE) {
                    desktop
                } else {
                    monitor
                },
            })
        }
    }
}

impl WallpaperImage {
    fn load(context: &ID2D1DeviceContext, path: &std::path::Path) -> Result<Self> {
        let decode_error = |error: image::ImageError| Error::new(E_FAIL, error.to_string());
        let decoded = image::ImageReader::open(path)
            .map_err(|error| Error::new(E_FAIL, error.to_string()))?
            .with_guessed_format()
            .map_err(|error| Error::new(E_FAIL, error.to_string()))?
            .decode()
            .map_err(decode_error)?;
        let (width, height) = (decoded.width() as f32, decoded.height() as f32);
        // Only the bounded GPU image is retained; the full-resolution decode is released here.
        let pixels = decoded
            .thumbnail(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE)
            .into_rgba8();
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        // SAFETY: The checked, bounded RGBA buffer covers every row and is copied by CreateBitmap.
        let bitmap = unsafe {
            context.CreateBitmap(
                D2D_SIZE_U {
                    width: pixels.width(),
                    height: pixels.height(),
                },
                Some(pixels.as_ptr().cast()),
                pixels.width() * 4,
                &properties,
            )?
        };
        Ok(Self {
            bitmap,
            width,
            height,
        })
    }

    fn draw(
        &self,
        context: &ID2D1DeviceContext,
        settings: &Settings,
        area: Rect,
        pixel_scale: u32,
    ) -> Result<()> {
        let (width, height) = rect_size(settings.bounds)
            .ok_or_else(|| Error::new(E_INVALIDARG, "Invalid wallpaper bounds"))?;
        let (width, height) = (width as f32, height as f32);
        let (x_scale, y_scale) = (width / self.width, height / self.height);
        let (x_scale, y_scale) = match settings.position {
            DWPOS_CENTER | DWPOS_TILE => (1.0, 1.0),
            DWPOS_STRETCH => (x_scale, y_scale),
            DWPOS_FIT => (x_scale.min(y_scale), x_scale.min(y_scale)),
            _ => (x_scale.max(y_scale), x_scale.max(y_scale)),
        };
        let (image_width, image_height) = (self.width * x_scale, self.height * y_scale);
        let (centre_x, centre_y) = if settings.position == DWPOS_TILE {
            (0.0, 0.0)
        } else {
            ((width - image_width) * 0.5, (height - image_height) * 0.5)
        };
        let scale = pixel_scale as f32;
        let left =
            ((i64::from(settings.bounds.left) - i64::from(area.left)) as f32 + centre_x) / scale;
        let top =
            ((i64::from(settings.bounds.top) - i64::from(area.top)) as f32 + centre_y) / scale;
        // SAFETY: All drawing resources belong to this device and the surface owns the active draw.
        unsafe {
            if settings.position == DWPOS_TILE {
                let size = self.bitmap.GetSize();
                let brush = context.CreateBitmapBrush(
                    &self.bitmap,
                    Some(&D2D1_BITMAP_BRUSH_PROPERTIES1 {
                        extendModeX: D2D1_EXTEND_MODE_WRAP,
                        extendModeY: D2D1_EXTEND_MODE_WRAP,
                        interpolationMode: D2D1_INTERPOLATION_MODE_LINEAR,
                    }),
                    Some(&D2D1_BRUSH_PROPERTIES {
                        opacity: 1.0,
                        transform: Matrix3x2 {
                            M11: image_width / size.width / scale,
                            M22: image_height / size.height / scale,
                            M31: left,
                            M32: top,
                            ..Default::default()
                        },
                    }),
                )?;
                context.FillRectangle(
                    &D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: (i64::from(area.right) - i64::from(area.left)) as f32 / scale,
                        bottom: (i64::from(area.bottom) - i64::from(area.top)) as f32 / scale,
                    },
                    &brush,
                );
            } else {
                context.DrawBitmap(
                    &self.bitmap,
                    Some(&D2D_RECT_F {
                        left,
                        top,
                        right: left + image_width / scale,
                        bottom: top + image_height / scale,
                    }),
                    1.0,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    None,
                    None,
                );
            }
        }
        Ok(())
    }
}

unsafe fn take_string(value: PWSTR) -> HSTRING {
    // SAFETY: The caller transfers a COM-allocated string from IDesktopWallpaper, including null.
    let result = if value.is_null() {
        HSTRING::new()
    } else {
        unsafe { value.to_hstring() }
    };
    // SAFETY: This allocation is released exactly once after copying.
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    result
}
