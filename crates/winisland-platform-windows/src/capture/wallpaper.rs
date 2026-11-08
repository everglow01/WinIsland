use std::path::PathBuf;
use std::time::SystemTime;

use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Composition::{CompositionGraphicsDevice, CompositionSurfaceBrush, SpriteVisual};
use windows::Win32::Foundation::{COLORREF, E_FAIL, E_INVALIDARG, GENERIC_READ, POINT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BITMAP_PROPERTIES1,
    D2D1_BRUSH_PROPERTIES, D2D1_EXTEND_MODE_WRAP, D2D1_INTERPOLATION_MODE_LINEAR, ID2D1Bitmap1,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICImagingFactory,
    IWICPalette, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree,
};
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorInterop,
};
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
    pub(super) visual: SpriteVisual,
    brush: CompositionSurfaceBrush,
    surfaces: CompositionGraphicsDevice,
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
        let interop: ICompositorInterop = composition.compositor.cast()?;
        // SAFETY: The worker apartment is initialized; all interfaces remain on that worker.
        let (surfaces, service) = unsafe {
            (
                interop.CreateGraphicsDevice(&composition.drawing)?,
                CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)?,
            )
        };
        let visual = composition.compositor.CreateSpriteVisual()?;
        let brush = composition.compositor.CreateSurfaceBrush()?;
        visual.SetBrush(&brush)?;
        Ok(Self {
            visual,
            brush,
            surfaces,
            service,
            settings: None,
            image: None,
            viewport: None,
        })
    }

    pub(super) fn ready(&self) -> bool {
        self.viewport.is_some()
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
        let surface = self.surfaces.CreateDrawingSurface(
            Size {
                Width: width as f32,
                Height: height as f32,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )?;
        let drawing: ICompositionDrawingSurfaceInterop = surface.cast()?;
        let mut offset = POINT::default();
        // SAFETY: The surface and writable offset are live; EndDraw is called on every path.
        let context: ID2D1DeviceContext = unsafe { drawing.BeginDraw(None, &mut offset)? };
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
                    Err(error) => {
                        return Err(Error::new(
                            error.code(),
                            format!("System wallpaper decode failed: {error}"),
                        ));
                    }
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
            drawing.EndDraw()
        };
        result.and(end)?;
        self.visual.SetSize(Vector2 {
            X: width as f32,
            Y: height as f32,
        })?;
        self.brush.SetSurface(&surface)?;
        if self.viewport.is_none() {
            log::info!(
                "Liquid glass system wallpaper ready ({})",
                if self.image.is_some() {
                    "image"
                } else {
                    "solid color"
                }
            );
        }
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
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        // SAFETY: COM is initialized on this worker. WIC owns its decoding buffers, bounds the
        // scaled source, and converts to the D2D device's native premultiplied BGRA format.
        let (bitmap, width, height) = unsafe {
            let imaging: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let decoder = imaging.CreateDecoderFromFilename(
                &HSTRING::from(path.as_os_str()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnLoad,
            )?;
            let frame = decoder.GetFrame(0)?;
            let (mut width, mut height) = (0, 0);
            frame.GetSize(&mut width, &mut height)?;
            if width == 0 || height == 0 {
                return Err(Error::new(E_INVALIDARG, "Empty wallpaper image"));
            }
            let mut source: IWICBitmapSource = frame.cast()?;
            let longest = width.max(height);
            if longest > MAX_IMAGE_EDGE {
                let scaled_width = (u64::from(width) * u64::from(MAX_IMAGE_EDGE)
                    / u64::from(longest))
                .max(1) as u32;
                let scaled_height = (u64::from(height) * u64::from(MAX_IMAGE_EDGE)
                    / u64::from(longest))
                .max(1) as u32;
                let scaler = imaging.CreateBitmapScaler()?;
                scaler.Initialize(
                    &source,
                    scaled_width,
                    scaled_height,
                    WICBitmapInterpolationModeFant,
                )?;
                source = scaler.cast()?;
            }
            let converted = imaging.CreateFormatConverter()?;
            converted.Initialize(
                &source,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None::<&IWICPalette>,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
            (
                context.CreateBitmapFromWicBitmap(&converted, Some(&properties))?,
                width as f32,
                height as f32,
            )
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
