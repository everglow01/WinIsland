use std::time::{Duration, Instant};

use windows::UI::Color;
use windows::UI::Composition::{CompositionTarget, ContainerVisual, Visual};
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND, LPARAM, RECT, SIZE};
use windows::Win32::Graphics::DirectComposition::IDCompositionVisual;
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_RECTSOURCE,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, EnumWindows, GetClassNameW, GetShellWindow,
    GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, MSG, PM_REMOVE,
    PeekMessageW, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, ShowWindow,
    TranslateMessage, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, Error, Interface, Result, w};
use windows_numerics::{Vector2, Vector3};
use winisland_platform::Rect;

use super::interop::{Composition, Desktop, Thumbnail};
use super::{Graphics, intersection, rect_size};

const DWM_TNP_ENABLE3D: u32 = 0x04000000;

struct Host(HWND);

impl Host {
    fn new() -> Result<Self> {
        // SAFETY: The predefined class uses the system procedure. This worker creates and destroys
        // its 1px host outside the virtual desktop and pumps its messages.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("WinIsland backdrop source"),
                WS_POPUP,
                GetSystemMetrics(SM_XVIRTUALSCREEN).saturating_sub(1),
                GetSystemMetrics(SM_YVIRTUALSCREEN).saturating_sub(1),
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        Ok(Self(hwnd))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // SAFETY: This worker owns the host and releases all composition objects before it.
        if let Err(error) = unsafe { DestroyWindow(self.0) } {
            log::debug!("Capture host destruction failed: {error}");
        }
    }
}

struct Target(CompositionTarget);

impl Drop for Target {
    fn drop(&mut self) {
        if let Err(error) = self.0.SetRoot(None::<&Visual>) {
            log::debug!("Capture target detach failed: {error}");
        }
        if let Err(error) = self.0.Close() {
            log::debug!("Capture target close failed: {error}");
        }
    }
}

pub(super) struct Scene {
    _target: Target,
    pub(super) root: ContainerVisual,
    content: Visual,
    background: windows::UI::Composition::SpriteVisual,
    desktop: Desktop,
    _wallpapers: Vec<Thumbnail>,
    desktop_windows: Vec<DesktopWindow>,
    checked: Instant,
    bounds: Rect,
    area: Rect,
    pixel_scale: u32,
    composition: Composition,
    _host: Host,
}

impl Scene {
    pub(super) fn new(
        graphics: &Graphics,
        excluded: &[isize],
        bounds: Rect,
        area: Rect,
        pixel_scale: u32,
    ) -> Result<Self> {
        let invalid_size = || Error::new(E_INVALIDARG, "Invalid backdrop dimensions");
        let (width, height) = rect_size(bounds).ok_or_else(invalid_size)?;
        let size = SIZE {
            cx: i32::try_from(width).map_err(|_| invalid_size())?,
            cy: i32::try_from(height).map_err(|_| invalid_size())?,
        };
        let vector = Vector2 {
            X: width as f32,
            Y: height as f32,
        };
        let host = Host::new()?;
        let composition = Composition::new(&graphics.device)?;
        let mut excluded: Vec<_> = excluded.iter().map(|&raw| HWND(raw as *mut _)).collect();
        excluded.push(host.0);
        let (desktop, visual) = Desktop::new(
            host.0,
            &composition.native,
            &excluded,
            RECT {
                left: bounds.left,
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
            },
            size,
        )?;
        // SAFETY: All visuals attached to this native wrapper belong to the same compositor.
        let wrapper = unsafe { composition.native.CreateVisual()? };
        let desktop_windows = desktop_windows()?;
        let mut wallpapers = Vec::new();
        for source in &desktop_windows {
            let visible = intersection(bounds, source.bounds);
            if rect_size(visible).is_none() {
                continue;
            }
            let relative = |origin: Rect| -> Result<RECT> {
                let offset =
                    |value: i32, origin: i32| value.checked_sub(origin).ok_or_else(invalid_size);
                Ok(RECT {
                    left: offset(visible.left, origin.left)?,
                    top: offset(visible.top, origin.top)?,
                    right: offset(visible.right, origin.left)?,
                    bottom: offset(visible.bottom, origin.top)?,
                })
            };
            let properties = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_RECTSOURCE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_SOURCECLIENTAREAONLY
                    | DWM_TNP_ENABLE3D,
                rcDestination: relative(bounds)?,
                rcSource: relative(source.bounds)?,
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
            };
            let (thumbnail, wallpaper) =
                Thumbnail::window(host.0, source.hwnd, &composition.native, &properties)?;
            // SAFETY: The thumbnail is created on this device and retained with its host.
            unsafe { wrapper.AddVisual(&wallpaper, false, None::<&IDCompositionVisual>)? };
            wallpapers.push(thumbnail);
        }
        // SAFETY: The live desktop visual is placed above its wallpaper layers on the same device.
        unsafe { wrapper.AddVisual(&visual, false, None::<&IDCompositionVisual>)? };
        let content: Visual = wrapper.cast()?;
        content.SetSize(vector)?;
        let compositor = &composition.compositor;
        let root = compositor.CreateContainerVisual()?;
        root.SetSize(vector)?;
        root.SetClip(&compositor.CreateInsetClip()?)?;
        let background = compositor.CreateSpriteVisual()?;
        background.SetSize(vector)?;
        background.SetBrush(&compositor.CreateColorBrushWithColor(Color {
            A: 255,
            R: 0,
            G: 0,
            B: 0,
        })?)?;
        root.Children()?.InsertAtBottom(&background)?;
        root.Children()?.InsertAtTop(&content)?;
        // SAFETY: The host and native device stay alive until after target destruction.
        let native_target = unsafe { composition.native.CreateTargetForHwnd(host.0, true)? };
        let target = Target(native_target.cast()?);
        target.0.SetRoot(&root)?;
        // SAFETY: All visual mutations are complete. Showing the offscreen host lets DWM update
        // its private visual without changing the display affinity of application windows.
        unsafe {
            composition.native.Commit()?;
            let _ = ShowWindow(host.0, SW_SHOWNOACTIVATE);
        }
        let mut scene = Self {
            _target: target,
            root,
            content,
            background,
            desktop,
            _wallpapers: wallpapers,
            desktop_windows,
            checked: Instant::now(),
            bounds,
            area: bounds,
            pixel_scale: 0,
            composition,
            _host: host,
        };
        scene.set_area(area, pixel_scale)?;
        Ok(scene)
    }

    pub(super) fn set_area(&mut self, area: Rect, pixel_scale: u32) -> Result<()> {
        if self.area == area && self.pixel_scale == pixel_scale {
            return Ok(());
        }
        let (width, height) =
            rect_size(area).ok_or_else(|| Error::new(E_INVALIDARG, "Invalid capture area"))?;
        let size = Vector2 {
            X: width.div_ceil(pixel_scale) as f32,
            Y: height.div_ceil(pixel_scale) as f32,
        };
        self.root.SetSize(size)?;
        self.background.SetSize(size)?;
        let scale = pixel_scale as f32;
        self.content.SetScale(Vector3 {
            X: 1.0 / scale,
            Y: 1.0 / scale,
            Z: 1.0,
        })?;
        self.content.SetOffset(Vector3 {
            X: (i64::from(self.bounds.left) - i64::from(area.left)) as f32 / scale,
            Y: (i64::from(self.bounds.top) - i64::from(area.top)) as f32 / scale,
            Z: 0.0,
        })?;
        // SAFETY: The retained compositor owns the updated visual tree on this worker.
        unsafe { self.composition.native.Commit()? };
        self.area = area;
        self.pixel_scale = pixel_scale;
        Ok(())
    }

    pub(super) fn refresh(&mut self) -> Result<()> {
        if self.checked.elapsed() >= Duration::from_secs(1) {
            if self.desktop_windows != desktop_windows()? {
                return Err(Error::new(E_FAIL, "Desktop shell windows changed"));
            }
            self.checked = Instant::now();
        }
        self.desktop.refresh()
    }
}

#[derive(PartialEq)]
struct DesktopWindow {
    hwnd: HWND,
    bounds: Rect,
}

fn desktop_windows() -> Result<Vec<DesktopWindow>> {
    struct Enumeration {
        process: u32,
        windows: Vec<DesktopWindow>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, parameter: LPARAM) -> BOOL {
        // SAFETY: EnumWindows invokes this synchronously with the live Enumeration below.
        let enumeration = unsafe { &mut *(parameter.0 as *mut Enumeration) };
        let mut class = [0_u16; 64];
        let mut process = 0;
        let mut bounds = RECT::default();
        // SAFETY: HWND is supplied by EnumWindows; all output buffers are initialized and sized.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return true.into();
            }
            GetWindowThreadProcessId(hwnd, Some(&mut process));
            if process != enumeration.process {
                return true.into();
            }
            let Ok(length) = usize::try_from(GetClassNameW(hwnd, &mut class)) else {
                return true.into();
            };
            let class = String::from_utf16_lossy(&class[..length]);
            if !matches!(class.as_str(), "Progman" | "WorkerW")
                || GetWindowRect(hwnd, &mut bounds).is_err()
            {
                return true.into();
            }
        }
        let bounds = Rect {
            left: bounds.left,
            top: bounds.top,
            right: bounds.right,
            bottom: bounds.bottom,
        };
        if rect_size(bounds).is_some() {
            enumeration.windows.push(DesktopWindow { hwnd, bounds });
        }
        true.into()
    }
    let mut enumeration = Enumeration {
        process: 0,
        windows: Vec::new(),
    };
    // SAFETY: The OS supplies the shell HWND; enumeration retains writable storage until completion.
    unsafe {
        let shell = GetShellWindow();
        if shell.is_invalid() {
            return Err(Error::new(E_FAIL, "Desktop shell is unavailable"));
        }
        GetWindowThreadProcessId(shell, Some(&mut enumeration.process));
        EnumWindows(
            Some(collect),
            LPARAM((&mut enumeration as *mut Enumeration) as isize),
        )?;
    }
    enumeration.windows.reverse();
    if enumeration.windows.is_empty() {
        return Err(Error::new(
            E_FAIL,
            "Desktop wallpaper windows are unavailable",
        ));
    }
    Ok(enumeration.windows)
}

pub(super) fn monitor_bounds(monitor: isize) -> Result<Rect> {
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>())
            .map_err(|_| Error::new(E_INVALIDARG, "Invalid monitor structure size"))?,
        ..Default::default()
    };
    // SAFETY: The monitor is an opaque OS handle. info is a writable, correctly sized structure.
    if !unsafe { GetMonitorInfoW(HMONITOR(monitor as *mut _), &mut info) }.as_bool() {
        return Err(Error::from_thread());
    }
    Ok(Rect {
        left: info.rcMonitor.left,
        top: info.rcMonitor.top,
        right: info.rcMonitor.right,
        bottom: info.rcMonitor.bottom,
    })
}

pub(super) fn pump_messages() {
    let mut message = MSG::default();
    // SAFETY: This dedicated worker dispatches only its own queued messages and valid MSG values.
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
