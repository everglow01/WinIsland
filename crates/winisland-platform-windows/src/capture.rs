mod desktop;
mod interop;
mod window;

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use parking_lot::{Condvar, Mutex};
use windows::Win32::Foundation::{HANDLE, HMODULE, HWND, LUID};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11Device1, ID3D11Device5,
    ID3D11DeviceContext, ID3D11DeviceContext4, ID3D11Fence, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::core::Interface;
use winisland_platform::{BackdropCaptureFrame, BackdropCaptureRequest, BackdropShare, Rect};

pub(super) struct Graphics {
    device: ID3D11Device5,
    context: ID3D11DeviceContext4,
}

impl Graphics {
    fn new(adapter_luid: u64) -> Result<Self, String> {
        let bytes = adapter_luid.to_le_bytes();
        let luid = LUID {
            LowPart: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            HighPart: i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        };
        // SAFETY: DXGI returns owned factory and adapter interfaces for the supplied LUID.
        let adapter: IDXGIAdapter = unsafe {
            CreateDXGIFactory1::<IDXGIFactory4>()
                .and_then(|factory| factory.EnumAdapterByLuid(luid))
        }
        .map_err(|error| format!("Capture adapter lookup failed: {error}"))?;
        Self::from_adapter(&adapter)
    }

    fn from_adapter(adapter: &IDXGIAdapter) -> Result<Self, String> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // SAFETY: The adapter is live and both outputs are initialized, owned Options.
        unsafe {
            D3D11CreateDevice(
                adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|error| format!("Capture device creation failed: {error}"))?;
        let device = device.ok_or("Capture device is missing")?;
        let context = context.ok_or("Capture context is missing")?;
        let multithread: ID3D11Multithread = context
            .cast()
            .map_err(|error| format!("Capture context protection is unavailable: {error}"))?;
        // SAFETY: Protection serializes immediate-context calls from the capture and UI threads.
        let _ = unsafe { multithread.SetMultithreadProtected(true) };
        Ok(Self {
            device: device.cast().map_err(|error| error.to_string())?,
            context: context.cast().map_err(|error| error.to_string())?,
        })
    }

    fn texture(&self, width: u32, height: u32) -> Result<ID3D11Texture2D, String> {
        let description = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            ..Default::default()
        };
        let mut texture = None;
        // SAFETY: The descriptor is initialized; the call returns an owned texture.
        unsafe {
            self.device
                .CreateTexture2D(&description, None, Some(&mut texture))
        }
        .map_err(|error| format!("Capture texture creation failed: {error}"))?;
        texture.ok_or_else(|| "Capture texture is missing".into())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Request {
    monitor: isize,
    area: Rect,
    interest: Rect,
    rate_hz: u32,
    pixel_scale: u32,
}

struct Frame {
    monitor: isize,
    bounds: Rect,
    origin: (i32, i32),
    size: (u32, u32),
    pixel_scale: u32,
    texture: ID3D11Texture2D,
}

#[derive(Default)]
struct Latest {
    frame: Option<Frame>,
    sequence: u64,
}

#[derive(Default)]
struct Control {
    request: Option<Request>,
    stopped: bool,
    frame_arrived: bool,
}

#[derive(Default)]
struct Shared {
    control: Mutex<Control>,
    changed: Condvar,
    latest: Mutex<Latest>,
    pending: AtomicBool,
}

impl Shared {
    fn notify_frame(&self) {
        let mut control = self.control.lock();
        if !control.frame_arrived {
            control.frame_arrived = true;
            self.changed.notify_one();
        }
    }

    fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            crate::window::wake();
        }
    }
}

struct Import {
    generation: u64,
    texture: ID3D11Texture2D,
    write_fence: ID3D11Fence,
    read_fence: ID3D11Fence,
}

pub(crate) struct BackdropCapture {
    hwnd: HWND,
    graphics: Arc<Graphics>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    import: RefCell<Option<Import>>,
    copied: Cell<Option<(u64, u64, i32, i32)>>,
    verified: Cell<Rect>,
    write_value: Cell<u64>,
}

impl BackdropCapture {
    pub(crate) fn new(windows: &[HWND], adapter_luid: u64) -> Result<Self, String> {
        let hwnd = *windows.first().ok_or("Overlay window is missing")?;
        let graphics = Arc::new(Graphics::new(adapter_luid)?);
        let excluded = windows.iter().map(|hwnd| hwnd.0 as isize).collect();
        let shared = Arc::new(Shared::default());
        let worker_graphics = Arc::clone(&graphics);
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("liquid-capture".into())
            .spawn(move || desktop::run(worker_graphics, worker_shared, excluded))
            .map_err(|error| format!("Capture worker failed to start: {error}"))?;
        Ok(Self {
            hwnd,
            graphics,
            shared,
            worker: Some(worker),
            import: RefCell::new(None),
            copied: Cell::new(None),
            verified: Cell::new(Rect::default()),
            write_value: Cell::new(0),
        })
    }

    pub(crate) fn pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    pub(crate) fn update(&self, request: BackdropCaptureRequest) -> Option<BackdropCaptureFrame> {
        let pixel_scale = request.share.pixel_scale.clamp(1, 2);
        let width = i32::try_from(request.share.width.checked_mul(pixel_scale)?).ok()?;
        let height = i32::try_from(request.share.height.checked_mul(pixel_scale)?).ok()?;
        let area = Rect {
            left: request.origin.x,
            top: request.origin.y,
            right: request.origin.x.checked_add(width)?,
            bottom: request.origin.y.checked_add(height)?,
        };
        // SAFETY: The window registry keeps the overlay alive while capture is running.
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        let capture = Request {
            monitor: monitor.0 as isize,
            area,
            interest: request.interest,
            rate_hz: request.max_rate_hz.clamp(1, 60),
            pixel_scale,
        };
        {
            let mut control = self.shared.control.lock();
            if control.request != Some(capture) {
                control.request = Some(capture);
                self.shared.changed.notify_one();
            }
        }
        self.shared.pending.store(false, Ordering::Release);
        self.ensure_import(request.share)?;
        let Some(latest) = self.shared.latest.try_lock() else {
            return self
                .copied
                .get()
                .filter(|copied| {
                    copied.0 == request.share.generation
                        && copied.2 == area.left
                        && copied.3 == area.top
                })
                .map(|_| BackdropCaptureFrame {
                    write_value: self.write_value.get(),
                    serial: self.write_value.get(),
                });
        };
        let frame = latest.frame.as_ref().filter(|frame| {
            frame.monitor == capture.monitor
                && frame.pixel_scale == pixel_scale
                && frame.origin == (area.left, area.top)
                && frame.size == (request.share.width, request.share.height)
        })?;
        let coverage = Rect {
            left: frame.origin.0,
            top: frame.origin.1,
            right: frame
                .origin
                .0
                .checked_add(i32::try_from(frame.size.0.checked_mul(pixel_scale)?).ok()?)?,
            bottom: frame
                .origin
                .1
                .checked_add(i32::try_from(frame.size.1.checked_mul(pixel_scale)?).ok()?)?,
        };
        let visible = intersection(area, frame.bounds);
        rect_size(visible)?;
        if intersection(visible, coverage) != visible {
            return None;
        }
        let key = (
            request.share.generation,
            latest.sequence,
            area.left,
            area.top,
        );
        let interest = intersection(request.interest, frame.bounds);
        if self.copied.get() != Some(key) || intersection(self.verified.get(), interest) != interest
        {
            match self.copy(frame, visible, request) {
                Ok(true) => {
                    self.copied.set(Some(key));
                    self.verified.set(interest);
                }
                Ok(false)
                    if self.copied.get().is_some_and(|copied| {
                        copied.0 == key.0 && copied.2 == key.2 && copied.3 == key.3
                    }) => {}
                Ok(false) => return None,
                Err(error) => {
                    log::warn!("Desktop backdrop copy failed: {error}");
                    return None;
                }
            }
        }
        Some(BackdropCaptureFrame {
            write_value: self.write_value.get(),
            serial: self.write_value.get(),
        })
    }

    fn copy(
        &self,
        frame: &Frame,
        visible: Rect,
        request: BackdropCaptureRequest,
    ) -> Result<bool, String> {
        let import = self.import.borrow();
        let import = import.as_ref().ok_or("Backdrop texture is not shared")?;
        // SAFETY: Both imported fences remain owned for this generation. Keep at most one
        // capture write queued while allowing a pending renderer read to finish on the GPU.
        let (read_completed, write_completed) = unsafe {
            (
                import.read_fence.GetCompletedValue(),
                import.write_fence.GetCompletedValue(),
            )
        };
        if read_completed == u64::MAX || write_completed == u64::MAX {
            return Err("Backdrop rendering device was removed".into());
        }
        if self.copied.get().is_some() && write_completed < self.write_value.get() {
            return Ok(false);
        }
        let offset = |value: i32, origin: i32| {
            u32::try_from(i64::from(value) - i64::from(origin))
                .map(|value| value / frame.pixel_scale)
                .map_err(|error| error.to_string())
        };
        let region = D3D11_BOX {
            left: offset(visible.left, frame.origin.0)?,
            top: offset(visible.top, frame.origin.1)?,
            front: 0,
            right: offset(visible.right, frame.origin.0)?,
            bottom: offset(visible.bottom, frame.origin.1)?,
            back: 1,
        };
        let x = offset(visible.left, request.origin.x)?;
        let y = offset(visible.top, request.origin.y)?;
        let value = self
            .write_value
            .get()
            .checked_add(1)
            .ok_or("Backdrop fence exhausted")?;
        // SAFETY: Both textures are on this device. Intersection checks bound the source and
        // destination; a GPU-side wait orders this copy after any unfinished D3D12 reads.
        unsafe {
            self.graphics
                .device
                .GetDeviceRemovedReason()
                .and_then(|()| {
                    if read_completed < request.read_value {
                        self.graphics
                            .context
                            .Wait(&import.read_fence, request.read_value)
                    } else {
                        Ok(())
                    }
                })
                .map(|()| {
                    self.graphics.context.CopySubresourceRegion(
                        &import.texture,
                        0,
                        x,
                        y,
                        0,
                        &frame.texture,
                        0,
                        Some(&region),
                    )
                })
                .and_then(|()| self.graphics.context.Signal(&import.write_fence, value))
                .map(|()| self.graphics.context.Flush())
        }
        .map_err(|error| error.to_string())?;
        self.write_value.set(value);
        Ok(true)
    }

    fn ensure_import(&self, share: BackdropShare) -> Option<()> {
        if self
            .import
            .borrow()
            .as_ref()
            .is_some_and(|import| import.generation == share.generation)
        {
            return Some(());
        }
        self.import.borrow_mut().take();
        let device1: ID3D11Device1 = self.graphics.device.cast().ok()?;
        // SAFETY: The renderer retains these NT handles for the current generation. Opening
        // them creates owned COM references; the capture and renderer use the same adapter.
        let result = (|| unsafe {
            let texture: ID3D11Texture2D =
                device1.OpenSharedResource1(HANDLE(share.texture as *mut _))?;
            let mut write_fence = None;
            let mut read_fence = None;
            self.graphics
                .device
                .OpenSharedFence(HANDLE(share.write_fence as *mut _), &mut write_fence)?;
            self.graphics
                .device
                .OpenSharedFence(HANDLE(share.read_fence as *mut _), &mut read_fence)?;
            windows::core::Result::Ok(Import {
                generation: share.generation,
                texture,
                write_fence: write_fence.ok_or_else(windows::core::Error::empty)?,
                read_fence: read_fence.ok_or_else(windows::core::Error::empty)?,
            })
        })();
        match result {
            Ok(import) => {
                // SAFETY: The owned fence stays live for this query and throughout subsequent copies.
                let completed = unsafe { import.write_fence.GetCompletedValue() };
                if completed == u64::MAX {
                    log::warn!("Backdrop sharing device was removed");
                    return None;
                }
                self.write_value.set(self.write_value.get().max(completed));
                *self.import.borrow_mut() = Some(import);
                self.copied.set(None);
                Some(())
            }
            Err(error) => {
                log::warn!("Backdrop share import failed: {error}");
                None
            }
        }
    }
}

impl Drop for BackdropCapture {
    fn drop(&mut self) {
        self.shared.control.lock().stopped = true;
        self.shared.changed.notify_one();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            log::warn!("Desktop capture worker did not stop cleanly");
        }
    }
}

fn intersection(a: Rect, b: Rect) -> Rect {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    Rect {
        left,
        top,
        right: a.right.min(b.right).max(left),
        bottom: a.bottom.min(b.bottom).max(top),
    }
}

fn rect_size(rect: Rect) -> Option<(u32, u32)> {
    let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left)).ok()?;
    let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top)).ok()?;
    (width > 0 && height > 0).then_some((width, height))
}
