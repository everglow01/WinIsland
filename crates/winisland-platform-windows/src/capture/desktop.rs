use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
    GraphicsCaptureSession, IDirect3D11CaptureFramePoolStatics2, IGraphicsCaptureItemStatics,
};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Graphics::SizeInt32;
use windows::UI::Composition::Visual;
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, D3D11_TEXTURE2D_DESC, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGIDevice};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::core::{Interface, Type};
use winisland_platform::Rect;

use super::interop::Apartment;
use super::window::{Scene, monitor_bounds, pump_messages};
use super::{Frame, Graphics, Request, Shared, intersection, rect_size};

const MESSAGE_INTERVAL: Duration = Duration::from_millis(50);
const INITIAL_RETRY: Duration = Duration::from_millis(250);
const MAX_RETRY: Duration = Duration::from_secs(4);
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
const FRAME_TICKS_PER_SECOND: i64 = 10_000_000;

pub(super) fn run(graphics: Arc<Graphics>, shared: Arc<Shared>, excluded: Vec<isize>) {
    let _apartment = match Apartment::new() {
        Ok(apartment) => apartment,
        Err(error) => {
            log::warn!("Liquid glass capture initialization failed: {error}");
            shared.wake();
            return;
        }
    };
    let mut session: Option<Session> = None;
    let mut previous: Option<Request> = None;
    let mut next = Instant::now();
    let mut retry = INITIAL_RETRY;
    let mut failure: Option<String> = None;
    loop {
        if pump_messages()
            && let Some(session) = &mut session
        {
            session.scene.settings_changed();
        }
        let frame_deadline = session.as_ref().and_then(Session::pending_frame_deadline);
        let (request, refresh, arrived) = {
            let mut control = shared.control.lock();
            if control.stopped {
                return;
            }
            let configuration_changed = control.request.map(|request| {
                (
                    request.monitor,
                    request.area,
                    request.pixel_scale,
                    request.rate_hz,
                )
            }) != previous.map(|request| {
                (
                    request.monitor,
                    request.area,
                    request.pixel_scale,
                    request.rate_hz,
                )
            });
            let now = Instant::now();
            let refresh = now >= next || configuration_changed;
            let arrived = (std::mem::take(&mut control.frame_arrived) && session.is_some())
                || frame_deadline.is_some_and(|deadline| now >= deadline);
            if let Some(request) = control.request
                && (refresh || arrived)
            {
                (request, refresh, arrived)
            } else {
                let wait = if control.request.is_none() {
                    MESSAGE_INTERVAL
                } else {
                    frame_deadline
                        .map_or(next, |deadline| deadline.min(next))
                        .saturating_duration_since(Instant::now())
                        .min(MESSAGE_INTERVAL)
                };
                shared.changed.wait_for(&mut control, wait);
                continue;
            }
        };
        if previous.is_some_and(|previous| previous.monitor != request.monitor) {
            session = None;
            shared.latest.lock().frame = None;
            retry = INITIAL_RETRY;
        }
        previous = Some(request);
        let started = Instant::now();
        let result = (|| {
            if session.is_none() {
                session = Some(Session::new(&graphics, &excluded, request, &shared)?);
            }
            match session.as_mut() {
                Some(session) => session.update(request, &shared, refresh, arrived),
                None => Err("Desktop capture session is missing".into()),
            }
        })();
        match result {
            Ok(ready) => {
                if ready {
                    if failure.take().is_some() {
                        log::info!("Liquid glass desktop capture recovered");
                    }
                    retry = INITIAL_RETRY;
                }
                if refresh {
                    next = started + Duration::from_secs_f64(1.0 / f64::from(request.rate_hz));
                }
            }
            Err(error) => {
                session = None;
                let removed = shared.latest.lock().frame.take().is_some();
                if failure.as_ref() != Some(&error) {
                    log::warn!("Liquid glass desktop capture will retry: {error}");
                    failure = Some(error);
                    shared.wake();
                } else if removed {
                    shared.wake();
                }
                next = Instant::now() + retry;
                retry = (retry * 2).min(MAX_RETRY);
            }
        }
    }
}

struct Stream {
    session: Option<GraphicsCaptureSession>,
    pool: Direct3D11CaptureFramePool,
    item: GraphicsCaptureItem,
    device: IDirect3DDevice,
    frame_token: Option<i64>,
    closed_token: Option<i64>,
    closed: Arc<AtomicBool>,
    frames: Arc<Mutex<Frames>>,
    rate_hz: u32,
    rate_control: bool,
}

impl Stream {
    fn new(
        graphics: &Graphics,
        scene: &Scene,
        size: SizeInt32,
        rate_hz: u32,
        shared: &Arc<Shared>,
    ) -> windows::core::Result<Self> {
        // Static WinRT caches can outlive the worker apartment. Acquire factories per session.
        let item_factory =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemStatics>()?;
        let pool_factory = windows::core::factory::<
            Direct3D11CaptureFramePool,
            IDirect3D11CaptureFramePoolStatics2,
        >()?;
        let visual: Visual = scene.root.cast()?;
        let mut raw_item = std::ptr::null_mut();
        // SAFETY: The factory and Visual interfaces match the generated ABI; from_abi validates
        // the output and takes ownership of the reference returned by CreateFromVisual.
        let item = unsafe {
            (item_factory.vtable().CreateFromVisual)(
                item_factory.as_raw(),
                visual.as_raw(),
                &mut raw_item,
            )
            .ok()?;
            GraphicsCaptureItem::from_abi(raw_item)?
        };
        let dxgi: IDXGIDevice = graphics.device.cast()?;
        // SAFETY: The live DXGI device is wrapped in an owned WinRT interface for this capture pool.
        let device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        let mut raw_pool = std::ptr::null_mut();
        // SAFETY: The device and factory are live, size was validated, and the returned pool is owned.
        let pool = unsafe {
            (pool_factory.vtable().CreateFreeThreaded)(
                pool_factory.as_raw(),
                device.as_raw(),
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
                &mut raw_pool,
            )
            .ok()?;
            Direct3D11CaptureFramePool::from_abi(raw_pool)?
        };
        let mut stream = Self {
            session: None,
            pool,
            item: item.clone(),
            device,
            frame_token: None,
            closed_token: None,
            closed: Arc::new(AtomicBool::new(false)),
            frames: Arc::new(Mutex::new(Frames {
                accepting: true,
                latest: None,
            })),
            rate_hz: 0,
            rate_control: true,
        };
        let frame_shared = Arc::downgrade(shared);
        let frames = Arc::clone(&stream.frames);
        stream.frame_token = Some(stream.pool.FrameArrived(&TypedEventHandler::<
            Direct3D11CaptureFramePool,
            windows::core::IInspectable,
        >::new(move |pool, _| {
            let Some(pool) = pool.as_ref() else {
                return Ok(());
            };
            let mut frames = frames.lock();
            if !frames.accepting {
                return Ok(());
            }
            // Drain on arrival so a busy consumer never leaves old frames in the WGC pool.
            frames.receive(pool);
            drop(frames);
            if let Some(shared) = frame_shared.upgrade() {
                shared.notify_frame();
            }
            Ok(())
        }))?);
        let closed = Arc::clone(&stream.closed);
        let closed_shared = Arc::downgrade(shared);
        stream.closed_token = Some(stream.item.Closed(&TypedEventHandler::<
            GraphicsCaptureItem,
            windows::core::IInspectable,
        >::new(move |_, _| {
            closed.store(true, Ordering::Release);
            if let Some(shared) = closed_shared.upgrade() {
                shared.notify_frame();
            }
            Ok(())
        }))?);
        stream.session = Some(stream.pool.CreateCaptureSession(&item)?);
        stream.set_rate(rate_hz);
        if let Some(session) = &stream.session {
            session.StartCapture()?;
        }
        Ok(stream)
    }

    fn set_rate(&mut self, rate_hz: u32) {
        if self.rate_hz == rate_hz {
            return;
        }
        self.rate_hz = rate_hz;
        if self.rate_control
            && let Some(session) = &self.session
            && let Err(error) = session.SetMinUpdateInterval(TimeSpan {
                Duration: FRAME_TICKS_PER_SECOND / i64::from(rate_hz.max(1)),
            })
        {
            log::debug!(
                "Capture interval control is unavailable; using software frame pacing: {error}"
            );
            self.rate_control = false;
        }
    }

    fn resize(&self, size: (u32, u32)) -> windows::core::Result<()> {
        {
            let mut frames = self.frames.lock();
            frames.accepting = false;
            frames.latest = None;
        }
        let result = self.pool.Recreate(
            &self.device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            SizeInt32 {
                Width: i32::try_from(size.0).map_err(|_| windows::core::Error::empty())?,
                Height: i32::try_from(size.1).map_err(|_| windows::core::Error::empty())?,
            },
        );
        let mut frames = self.frames.lock();
        frames.accepting = result.is_ok();
        if frames.accepting {
            frames.receive(&self.pool);
        }
        result
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        {
            let mut frames = self.frames.lock();
            frames.accepting = false;
            frames.latest = None;
        }
        if let Some(token) = self.frame_token.take()
            && let Err(error) = self.pool.RemoveFrameArrived(token)
        {
            log::debug!("Capture frame handler removal failed: {error}");
        }
        if let Some(token) = self.closed_token.take()
            && let Err(error) = self.item.RemoveClosed(token)
        {
            log::debug!("Capture closed handler removal failed: {error}");
        }
        if let Some(session) = self.session.take()
            && let Err(error) = session.Close()
        {
            log::debug!("Backdrop capture session close failed: {error}");
        }
        if let Err(error) = self.pool.Close() {
            log::debug!("Backdrop capture pool close failed: {error}");
        }
    }
}

struct Frames {
    accepting: bool,
    latest: Option<Result<Lease, String>>,
}

impl Frames {
    fn receive(&mut self, pool: &Direct3D11CaptureFramePool) {
        for _ in 0..2 {
            match pool.TryGetNextFrame() {
                Ok(frame) => self.latest = Some(Ok(Lease(frame))),
                Err(error) if error.code() == windows::core::Error::empty().code() => break,
                Err(error) => {
                    self.latest = Some(Err(format!("Desktop visual frame failed: {error}")));
                    break;
                }
            }
        }
    }
}

struct Session {
    stream: Stream,
    scene: Scene,
    graphics: Arc<Graphics>,
    monitor: isize,
    bounds: Rect,
    area: Rect,
    pixel_scale: u32,
    size: (u32, u32),
    snapshot: ID3D11Texture2D,
    last_frame: Instant,
    next_frame: Instant,
    fresh_after: Option<Instant>,
    ready: bool,
}

impl Session {
    fn new(
        graphics: &Arc<Graphics>,
        excluded: &[isize],
        request: Request,
        shared: &Arc<Shared>,
    ) -> Result<Self, String> {
        let monitor = request.monitor;
        let bounds = monitor_bounds(monitor).map_err(|error| error.to_string())?;
        let area = request.area;
        let size = capture_size(bounds, request)?;
        let content_size = SizeInt32 {
            Width: i32::try_from(size.0).map_err(|error| error.to_string())?,
            Height: i32::try_from(size.1).map_err(|error| error.to_string())?,
        };
        let scene = Scene::new(
            graphics,
            excluded,
            bounds,
            area,
            request.interest,
            request.pixel_scale,
        )
        .map_err(|error| format!("Desktop composition source failed: {error}"))?;
        let stream = Stream::new(graphics, &scene, content_size, request.rate_hz, shared)
            .map_err(|error| format!("Desktop visual capture failed: {error}"))?;
        let snapshot = graphics.texture(size.0, size.1)?;
        log::info!(
            "Liquid glass captures a {}x{} desktop region with wallpaper and local window exclusions",
            size.0,
            size.1
        );
        Ok(Self {
            stream,
            scene,
            graphics: Arc::clone(graphics),
            monitor,
            bounds,
            area,
            pixel_scale: request.pixel_scale,
            size,
            snapshot,
            last_frame: Instant::now(),
            next_frame: Instant::now(),
            fresh_after: None,
            ready: false,
        })
    }

    fn pending_frame_deadline(&self) -> Option<Instant> {
        self.stream
            .frames
            .lock()
            .latest
            .as_ref()
            .map(|_| self.next_frame)
    }

    fn update(
        &mut self,
        request: Request,
        shared: &Shared,
        refresh: bool,
        arrived: bool,
    ) -> Result<bool, String> {
        if refresh
            && let Some(changed_at) = self
                .scene
                .refresh(request.interest)
                .map_err(|error| format!("Desktop visual refresh failed: {error}"))?
        {
            if self.fresh_after.is_none() {
                self.last_frame = changed_at;
            }
            self.fresh_after = Some(changed_at);
            self.next_frame = changed_at;
        }
        self.receive_frame(request, shared, refresh, arrived)
    }

    fn receive_frame(
        &mut self,
        request: Request,
        shared: &Shared,
        refresh: bool,
        arrived: bool,
    ) -> Result<bool, String> {
        if self.stream.closed.load(Ordering::Acquire) {
            return Err("Desktop capture source was closed".into());
        }
        if refresh
            && monitor_bounds(self.monitor).map_err(|error| error.to_string())? != self.bounds
        {
            return Err("Desktop monitor geometry changed".into());
        }
        if refresh {
            // SAFETY: The capture device is owned by this session and shared with a protected context.
            unsafe { self.graphics.device.GetDeviceRemovedReason() }
                .map_err(|error| format!("Desktop capture device was removed: {error}"))?;
            if self.stream.rate_hz != request.rate_hz {
                self.next_frame = Instant::now();
                self.stream.set_rate(request.rate_hz);
            }
        }
        let area = request.area;
        if self.area != area || self.pixel_scale != request.pixel_scale {
            let size = capture_size(self.bounds, request)?;
            self.scene
                .set_area(area, request.pixel_scale)
                .map_err(|error| error.to_string())?;
            self.stream
                .resize(size)
                .map_err(|error| error.to_string())?;
            if self.size != size {
                self.snapshot = self.graphics.texture(size.0, size.1)?;
            }
            self.area = area;
            self.pixel_scale = request.pixel_scale;
            self.size = size;
            self.ready = false;
            self.last_frame = Instant::now();
            self.next_frame = Instant::now();
            shared.latest.lock().frame = None;
        }
        // An unchanged desktop can stop producing frames; invalidated content still needs a fresh frame.
        if (!self.ready || self.fresh_after.is_some()) && self.last_frame.elapsed() >= FRAME_TIMEOUT
        {
            return Err("Desktop visual did not produce a fresh frame".into());
        }
        if !arrived && !refresh {
            return Ok(self.ready);
        }
        // Keep an early frame in the mailbox. The worker wakes at this deadline even if no
        // further frame arrives, so pacing cannot discard the final desktop update.
        if self.ready && Instant::now() < self.next_frame {
            return Ok(true);
        }
        let Some(frame) = self.stream.frames.lock().latest.take() else {
            return Ok(self.ready);
        };
        let frame = frame?;
        let captured_at = frame_time(&frame);
        if self
            .fresh_after
            .is_some_and(|after| captured_at.is_some_and(|at| at < after))
        {
            return Ok(self.ready);
        }
        let access: IDirect3DDxgiInterfaceAccess = frame
            .0
            .Surface()
            .and_then(|surface| surface.cast())
            .map_err(|error| error.to_string())?;
        // SAFETY: The capture frame retains the surface while it is queried and copied below.
        let texture: ID3D11Texture2D =
            unsafe { access.GetInterface() }.map_err(|error| error.to_string())?;
        let mut description = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: description is initialized and writable; texture remains owned by the frame lease.
        unsafe { texture.GetDesc(&mut description) };
        let size = frame.0.ContentSize().map_err(|error| error.to_string())?;
        if description.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err("Desktop visual texture format changed".into());
        }
        if u32::try_from(size.Width).ok() != Some(self.size.0)
            || u32::try_from(size.Height).ok() != Some(self.size.1)
            || description.Width < self.size.0
            || description.Height < self.size.1
        {
            if self.last_frame.elapsed() >= FRAME_TIMEOUT {
                return Err("Desktop visual resize did not complete".into());
            }
            return Ok(false);
        }
        let region = D3D11_BOX {
            right: self.size.0,
            bottom: self.size.1,
            back: 1,
            ..Default::default()
        };
        {
            let mut latest = shared.latest.lock();
            // SAFETY: The checked BGRA region fits both textures on this device. Holding latest
            // serializes snapshot writes with the UI's copies into the shared D3D12 texture.
            unsafe {
                self.graphics.context.CopySubresourceRegion(
                    &self.snapshot,
                    0,
                    0,
                    0,
                    0,
                    &texture,
                    0,
                    Some(&region),
                );
                self.graphics.context.Flush();
            }
            latest.frame = Some(Frame {
                monitor: self.monitor,
                bounds: self.area,
                origin: (self.area.left, self.area.top),
                size: self.size,
                pixel_scale: self.pixel_scale,
                texture: self.snapshot.clone(),
                captured_at,
            });
            latest.sequence = latest.sequence.wrapping_add(1);
        }
        self.ready = true;
        self.fresh_after = None;
        self.last_frame = Instant::now();
        let interval = Duration::from_secs_f64(1.0 / f64::from(request.rate_hz));
        let next = self.next_frame + interval;
        self.next_frame = if next > self.last_frame {
            next
        } else {
            self.last_frame + interval
        };
        shared.wake();
        Ok(true)
    }
}

struct Lease(Direct3D11CaptureFrame);

fn frame_time(frame: &Lease) -> Option<Instant> {
    static FREQUENCY: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
    let frequency = (*FREQUENCY.get_or_init(|| {
        let mut frequency = 0;
        // SAFETY: The output is writable; the QPC frequency is fixed for the system lifetime.
        unsafe { QueryPerformanceFrequency(&mut frequency) }.ok()?;
        (frequency > 0).then_some(frequency)
    }))?;
    let ticks = frame.0.SystemRelativeTime().ok()?.Duration;
    let mut counter = 0;
    // SAFETY: counter is writable. WGC timestamps and QPC use the same monotonic clock.
    unsafe { QueryPerformanceCounter(&mut counter) }.ok()?;
    let now = Instant::now();
    let current = i128::from(counter) * i128::from(FRAME_TICKS_PER_SECOND) / i128::from(frequency);
    let age = u64::try_from((current - i128::from(ticks)).max(0)).ok()?;
    now.checked_sub(Duration::from_nanos(age.checked_mul(100)?))
}

fn capture_size(bounds: Rect, request: Request) -> Result<(u32, u32), String> {
    rect_size(intersection(bounds, request.area)).ok_or("Capture area is outside the monitor")?;
    let (width, height) = rect_size(request.area).ok_or("Invalid desktop capture dimensions")?;
    Ok((
        width.div_ceil(request.pixel_scale),
        height.div_ceil(request.pixel_scale),
    ))
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Err(error) = self.0.Close() {
            log::debug!("Desktop visual frame close failed: {error}");
        }
    }
}
