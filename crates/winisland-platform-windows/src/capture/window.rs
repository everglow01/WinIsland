use std::cell::Cell;
use std::time::{Duration, Instant};

use windows::UI::Color;
use windows::UI::Composition::{CompositionTarget, ContainerVisual, Visual};
use windows::Win32::Foundation::{
    E_FAIL, E_INVALIDARG, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT,
    SIZE, WPARAM,
};
use windows::Win32::Graphics::DirectComposition::{IDCompositionVisual, IDCompositionVisual2};
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_RECTSOURCE,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DWMWA_CLOAKED, DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    COLOR_DESKTOP, GetMonitorInfoW, GetSysColor, HMONITOR, MONITORINFO,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows,
    EnumWindows, GA_PARENT, GA_ROOT, GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor,
    GetClassNameW, GetParent, GetShellWindow, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassW, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, ShowWindow,
    TranslateMessage, WM_SETTINGCHANGE, WM_THEMECHANGED, WNDCLASSW, WS_CHILD, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{BOOL, Error, Interface, Result, w};
use windows_numerics::{Vector2, Vector3};
use winisland_platform::Rect;

use super::interop::{Composition, Desktop, Thumbnail};
use super::wallpaper::Wallpaper;
use super::{Graphics, intersection, rect_size};

const DWM_TNP_ENABLE3D: u32 = 0x04000000;

thread_local! {
    static WALLPAPER_CHANGED: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED) {
        WALLPAPER_CHANGED.set(true);
    }
    // SAFETY: Windows supplied these parameters to the registered procedure.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

struct Host(HWND);

impl Host {
    fn new() -> Result<Self> {
        // SAFETY: The process-lifetime procedure belongs to this module. The worker owns the
        // 1px host outside the virtual desktop and pumps both sent and posted messages.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("WinIslandBackdropSource");
            let descriptor = WNDCLASSW {
                lpfnWndProc: Some(host_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            if RegisterClassW(&descriptor) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_thread());
            }
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
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
    desktop_visual: IDCompositionVisual2,
    desktop_visible: bool,
    desktop_dirty: bool,
    wrapper: IDCompositionVisual2,
    wallpapers: WallpaperLayers,
    wallpaper: Option<Wallpaper>,
    excluded: Vec<HWND>,
    windows: Vec<DesktopWindow>,
    pending_windows: Vec<DesktopWindow>,
    hidden_windows: Vec<HWND>,
    pending_hidden: Vec<HWND>,
    refresh_error: Option<String>,
    checked: Instant,
    wallpaper_dirty: bool,
    wallpaper_checked: Instant,
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
        interest: Rect,
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
        let mut windows = Vec::new();
        let mut hidden_windows = Vec::new();
        let desktop_windows = desktop_windows()?;
        window_stack(
            &excluded,
            &desktop_windows.roots,
            interest,
            &mut windows,
            &mut hidden_windows,
        )?;
        let desktop_excluded =
            capture_exclusions(&excluded, &desktop_windows.roots, &hidden_windows);
        let (desktop, visual) = Desktop::new(
            host.0,
            &composition.native,
            &desktop_excluded,
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
        let wallpapers =
            WallpaperLayers::new(&composition, host.0, bounds, desktop_windows, false)?;
        let desktop_visible = !windows.is_empty();
        // SAFETY: The live desktop visual is placed above its wallpaper layers on the same device.
        unsafe {
            wrapper.AddVisual(&wallpapers.visual, false, None::<&IDCompositionVisual>)?;
            if desktop_visible {
                wrapper.AddVisual(&visual, true, &wallpapers.visual)?;
            }
        }
        let content: Visual = wrapper.cast()?;
        content.SetSize(vector)?;
        let compositor = &composition.compositor;
        let root = compositor.CreateContainerVisual()?;
        root.SetSize(vector)?;
        root.SetClip(&compositor.CreateInsetClip()?)?;
        let background = compositor.CreateSpriteVisual()?;
        background.SetSize(vector)?;
        // SAFETY: GetSysColor only reads the current desktop background color.
        let color = unsafe { GetSysColor(COLOR_DESKTOP) };
        background.SetBrush(&compositor.CreateColorBrushWithColor(Color {
            A: 255,
            R: (color & 255) as u8,
            G: ((color >> 8) & 255) as u8,
            B: ((color >> 16) & 255) as u8,
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
            desktop_visual: visual,
            desktop_visible,
            desktop_dirty: true,
            wrapper,
            wallpapers,
            wallpaper: None,
            excluded,
            windows,
            pending_windows: Vec::new(),
            hidden_windows,
            pending_hidden: Vec::new(),
            refresh_error: None,
            checked: Instant::now(),
            wallpaper_dirty: true,
            wallpaper_checked: Instant::now(),
            bounds,
            area: bounds,
            pixel_scale: 0,
            composition,
            _host: host,
        };
        scene.set_area(area, pixel_scale)?;
        if let Err(error) = scene.refresh_wallpaper() {
            log::warn!("Initial wallpaper fallback is unavailable: {error}");
        }
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
        if let Some(wallpaper) = &mut self.wallpaper
            && let Err(error) = wallpaper.set_area(area, pixel_scale)
        {
            log::warn!("Wallpaper viewport update failed: {error}");
        }
        // SAFETY: The retained compositor owns the updated visual tree on this worker.
        unsafe { self.composition.native.Commit()? };
        self.area = area;
        self.pixel_scale = pixel_scale;
        Ok(())
    }

    pub(super) fn refresh(&mut self, interest: Rect) -> Result<Option<Instant>> {
        if self.wallpaper_dirty || self.checked.elapsed() >= Duration::from_secs(1) {
            match self.refresh_wallpaper() {
                Ok(()) => self.refresh_error = None,
                Err(error) => {
                    let message = error.to_string();
                    if self.refresh_error.as_ref() != Some(&message) {
                        log::warn!("Keeping the previous desktop background: {message}");
                        self.refresh_error = Some(message);
                    }
                }
            }
            self.checked = Instant::now();
            self.wallpaper_dirty = false;
        }
        window_stack(
            &self.excluded,
            &self.wallpapers.sources.roots,
            interest,
            &mut self.pending_windows,
            &mut self.pending_hidden,
        )?;
        let visibility_changed = self.desktop_dirty
            || self
                .windows
                .iter()
                .map(|window| window.hwnd)
                .ne(self.pending_windows.iter().map(|window| window.hwnd));
        let mut changed_at = None;
        if self.desktop_dirty || self.windows != self.pending_windows {
            if self.desktop_dirty {
                // SAFETY: Capture has started; publish the host's visual tree before refreshing
                // its initially hidden desktop source. This wait runs only during initialization.
                unsafe { self.composition.native.WaitForCommitCompletion()? };
            }
            let hidden_source = self.windows.iter().any(|window| {
                self.pending_hidden
                    .binary_search_by_key(&(window.hwnd.0 as usize), |hwnd| hwnd.0 as usize)
                    .is_ok()
            });
            let excluded = capture_exclusions(
                &self.excluded,
                &self.wallpapers.sources.roots,
                &self.pending_hidden,
            );
            changed_at = visibility_changed.then(Instant::now);
            self.refresh_desktop(excluded, hidden_source, !self.pending_windows.is_empty())?;
            self.desktop_dirty = false;
            std::mem::swap(&mut self.windows, &mut self.pending_windows);
        }
        std::mem::swap(&mut self.hidden_windows, &mut self.pending_hidden);
        Ok(changed_at)
    }

    fn refresh_desktop(
        &mut self,
        excluded: Vec<HWND>,
        hidden_source: bool,
        visible: bool,
    ) -> Result<()> {
        let replacement = if hidden_source {
            Some(
                self.desktop
                    .recreate(&self.composition.native, Some(&excluded))?,
            )
        } else {
            match self.desktop.refresh(Some(excluded)) {
                Ok(()) => None,
                Err(error) if error.code() == E_INVALIDARG => {
                    log::warn!(
                        "DWM desktop update was invalidated; recreating the source: {error}"
                    );
                    Some(
                        self.desktop
                            .recreate(&self.composition.native, None)
                            .map_err(|error| {
                                Error::new(
                                    error.code(),
                                    format!("Recreate shared desktop: {error}"),
                                )
                            })?,
                    )
                }
                Err(error) => {
                    return Err(Error::new(
                        error.code(),
                        format!("Update shared desktop: {error}"),
                    ));
                }
            }
        };
        // SAFETY: Visual replacement is committed atomically on its owning compositor. The old
        // thumbnail stays alive until that commit completes; steady geometry updates do not wait.
        unsafe {
            if let Some((_, visual)) = &replacement {
                if self.desktop_visible {
                    self.wrapper.RemoveVisual(&self.desktop_visual)?;
                }
                if visible {
                    self.wrapper
                        .AddVisual(visual, true, &self.wallpapers.visual)?;
                }
            } else if visible != self.desktop_visible {
                if visible {
                    self.wrapper
                        .AddVisual(&self.desktop_visual, true, &self.wallpapers.visual)?;
                } else {
                    self.wrapper.RemoveVisual(&self.desktop_visual)?;
                }
            }
            self.composition.native.Commit().map_err(|error| {
                Error::new(error.code(), format!("Commit desktop geometry: {error}"))
            })?;
            if self.desktop_dirty || replacement.is_some() {
                self.composition.native.WaitForCommitCompletion()?;
            }
            if let Some((desktop, visual)) = replacement {
                self.desktop_visual = visual;
                self.desktop = desktop;
                log::debug!("Liquid glass desktop source replaced");
            }
        }
        self.desktop_visible = visible;
        Ok(())
    }

    pub(super) fn settings_changed(&mut self) {
        self.wallpaper_dirty = true;
    }

    fn refresh_wallpaper(&mut self) -> Result<()> {
        let background_result = (|| {
            if self.wallpaper.is_none() {
                let wallpaper = Wallpaper::new(&self.composition)?;
                self.root
                    .Children()?
                    .InsertAbove(&wallpaper.visual, &self.background)?;
                self.wallpaper = Some(wallpaper);
            }
            if (self.wallpaper_dirty || self.wallpaper_checked.elapsed() >= Duration::from_secs(30))
                && let Some(wallpaper) = &mut self.wallpaper
            {
                self.wallpaper_checked = Instant::now();
                wallpaper.refresh(self.bounds, self.area, self.pixel_scale)?;
            }
            Ok(())
        })();
        let sources = desktop_windows()?;
        let system_background = self.wallpaper.as_ref().is_some_and(Wallpaper::ready);
        if self.wallpapers.sources != sources
            || self.wallpapers.system_background != system_background
        {
            let next = WallpaperLayers::new(
                &self.composition,
                self._host.0,
                self.bounds,
                sources,
                system_background,
            )?;
            let excluded =
                capture_exclusions(&self.excluded, &next.sources.roots, &self.hidden_windows);
            self.desktop.set_excluded(excluded)?;
            // SAFETY: Both wallpaper trees and the window visual share this compositor. The
            // old thumbnails remain registered until the replacement has reached DWM.
            unsafe {
                self.wrapper
                    .AddVisual(&next.visual, false, None::<&IDCompositionVisual>)?;
                self.wrapper.RemoveVisual(&self.wallpapers.visual)?;
                self.composition.native.Commit()?;
                self.composition.native.WaitForCommitCompletion()?;
            }
            self.wallpapers = next;
        } else {
            // SAFETY: Publishes any updated wallpaper surface on its owning worker.
            unsafe { self.composition.native.Commit()? };
        }
        background_result
    }
}

struct WallpaperLayers {
    visual: IDCompositionVisual2,
    _thumbnails: Vec<Thumbnail>,
    sources: DesktopSources,
    system_background: bool,
}

impl WallpaperLayers {
    fn new(
        composition: &Composition,
        host: HWND,
        bounds: Rect,
        sources: DesktopSources,
        system_background: bool,
    ) -> Result<Self> {
        // SAFETY: The visual is retained with its creating compositor.
        let visual = unsafe { composition.native.CreateVisual()? };
        let live_hosts = live_wallpaper_hosts(&sources, bounds);
        let mut thumbnails = Vec::new();
        let mut captured = Vec::new();
        let mut unavailable = 0;
        for source in &sources.windows {
            // SAFETY: Enumeration supplied the HWND; these calls only inspect window metadata.
            let (shell, style, extended) = unsafe {
                let mut process = 0;
                GetWindowThreadProcessId(source.hwnd, Some(&mut process));
                (
                    process == sources.process,
                    GetWindowLongPtrW(source.hwnd, GWL_STYLE) as u32,
                    GetWindowLongPtrW(source.hwnd, GWL_EXSTYLE) as u32,
                )
            };
            let live_host = live_hosts.contains(&source.hwnd);
            if shell
                && !live_host
                && (system_background || extended & WS_EX_NOREDIRECTIONBITMAP.0 != 0)
            {
                continue;
            }
            if !shell && extended & WS_EX_NOREDIRECTIONBITMAP.0 != 0 {
                // Unredirected child surfaces can be absent from their own thumbnails. Keep
                // the containing desktop host's composition instead of covering it with that snapshot.
                // SAFETY: The enumerated handle is queried without accessing caller memory.
                let host = unsafe { GetAncestor(source.hwnd, GA_ROOT) };
                let visible = intersection(bounds, source.bounds);
                if captured.iter().any(|(window, region)| {
                    *window == host && intersection(*region, visible) == visible
                }) {
                    continue;
                }
            }
            let mut capture_window = source.hwnd;
            let mut capture_bounds = source.bounds;
            if style & WS_CHILD.0 != 0 && extended & WS_EX_LAYERED.0 == 0 {
                let mut rect = RECT::default();
                // SAFETY: DWM shared thumbnails require a top-level or layered source. Capture
                // the owning window and crop to the original wallpaper child's screen rectangle.
                unsafe {
                    capture_window = GetAncestor(source.hwnd, GA_ROOT);
                    if capture_window.is_invalid()
                        || GetWindowRect(capture_window, &mut rect).is_err()
                    {
                        unavailable += usize::from(!shell);
                        continue;
                    }
                }
                capture_bounds = Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                };
            }
            let visible = intersection(intersection(bounds, source.bounds), capture_bounds);
            if rect_size(visible).is_none() {
                continue;
            }
            if captured.contains(&(capture_window, visible)) {
                continue;
            }
            let relative = |origin: Rect| -> Result<RECT> {
                let offset = |value: i32, origin: i32| {
                    value
                        .checked_sub(origin)
                        .ok_or_else(|| Error::new(E_INVALIDARG, "Invalid wallpaper coordinates"))
                };
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
                rcDestination: relative(visible)?,
                rcSource: relative(capture_bounds)?,
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
            };
            match Thumbnail::window(host, capture_window, &composition.native, &properties) {
                Ok((thumbnail, wallpaper)) => {
                    let destination = relative(bounds)?;
                    // SAFETY: Shared thumbnail destinations start at zero; the owning visual
                    // supplies screen placement without changing the thumbnail's source crop.
                    unsafe {
                        wallpaper.SetOffsetX2(destination.left as f32)?;
                        wallpaper.SetOffsetY2(destination.top as f32)?;
                        visual.AddVisual(&wallpaper, false, None::<&IDCompositionVisual>)?;
                    }
                    thumbnails.push(thumbnail);
                    captured.push((capture_window, visible));
                    if system_background && live_host {
                        log::info!(
                            "Liquid glass preserves live wallpaper host {:?}",
                            source.hwnd
                        );
                    }
                    if system_background && !shell {
                        log::info!(
                            "Liquid glass live wallpaper source {:?}, capture window {:?}",
                            source.hwnd,
                            capture_window
                        );
                    }
                }
                Err(error) => {
                    unavailable += usize::from(!shell);
                    log::warn!(
                        "Desktop background layer {:?} is unavailable: {error}",
                        source.hwnd
                    );
                }
            }
        }
        if system_background && unavailable > 0 && thumbnails.is_empty() {
            log::warn!(
                "Live wallpaper windows have no capturable thumbnail surface ({unavailable} windows)"
            );
        }
        Ok(Self {
            visual,
            _thumbnails: thumbnails,
            sources,
            system_background,
        })
    }
}

fn live_wallpaper_hosts(sources: &DesktopSources, bounds: Rect) -> Vec<HWND> {
    let mut hosts = Vec::new();
    for source in &sources.windows {
        if rect_size(intersection(bounds, source.bounds)).is_none() {
            continue;
        }
        let mut process = 0;
        // SAFETY: All handles originate from enumeration; ancestor queries are read-only and
        // stop at the actual root, so an owner window is not mistaken for a composition parent.
        unsafe {
            GetWindowThreadProcessId(source.hwnd, Some(&mut process));
            if process == 0 || process == sources.process {
                continue;
            }
            let root = GetAncestor(source.hwnd, GA_ROOT);
            let mut parent = source.hwnd;
            while parent != root && !parent.is_invalid() {
                parent = GetAncestor(parent, GA_PARENT);
                if parent.is_invalid() || hosts.contains(&parent) {
                    break;
                }
                hosts.push(parent);
            }
        }
    }
    hosts
}

#[derive(PartialEq)]
struct DesktopWindow {
    hwnd: HWND,
    bounds: Rect,
}

#[derive(PartialEq)]
struct DesktopSources {
    process: u32,
    roots: Vec<HWND>,
    windows: Vec<DesktopWindow>,
}

fn desktop_windows() -> Result<DesktopSources> {
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
    let roots = enumeration
        .windows
        .iter()
        .map(|source| source.hwnd)
        .collect();
    let mut windows = Vec::new();
    for root in enumeration.windows {
        let hwnd = root.hwnd;
        windows.push(root);
        desktop_children(hwnd, enumeration.process, &mut windows);
    }
    Ok(DesktopSources {
        process: enumeration.process,
        roots,
        windows,
    })
}

fn desktop_children(parent: HWND, shell_process: u32, windows: &mut Vec<DesktopWindow>) {
    struct Children {
        parent: HWND,
        shell_process: u32,
        windows: Vec<(HWND, Option<Rect>)>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, parameter: LPARAM) -> BOOL {
        // SAFETY: EnumChildWindows invokes this synchronously with the live Children below.
        let children = unsafe { &mut *(parameter.0 as *mut Children) };
        let mut class = [0_u16; 64];
        let mut process = 0;
        let mut rect = RECT::default();
        let mut bounds = None;
        // SAFETY: HWND comes from enumeration; buffers are initialized and writable.
        unsafe {
            if GetParent(hwnd).ok() != Some(children.parent) {
                return true.into();
            }
            let length = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let worker = class[..length] == *w!("WorkerW").as_wide();
            GetWindowThreadProcessId(hwnd, Some(&mut process));
            if (worker || process != children.shell_process)
                && IsWindowVisible(hwnd).as_bool()
                && !IsIconic(hwnd).as_bool()
                && GetWindowRect(hwnd, &mut rect).is_ok()
            {
                let rectangle = Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                if rect_size(rectangle).is_some() {
                    bounds = Some(rectangle);
                }
            }
        }
        children.windows.push((hwnd, bounds));
        true.into()
    }
    let mut children = Children {
        parent,
        shell_process,
        windows: Vec::new(),
    };
    // SAFETY: The callback uses stack storage only during synchronous enumeration. Child
    // WorkerW hosts on newer Windows also contain live wallpapers owned by other processes.
    let _ = unsafe {
        EnumChildWindows(
            Some(parent),
            Some(collect),
            LPARAM((&mut children as *mut Children) as isize),
        )
    };
    for (hwnd, bounds) in children.windows.into_iter().rev() {
        if let Some(bounds) = bounds {
            windows.push(DesktopWindow { hwnd, bounds });
        }
        // Shell containers can host foreign wallpaper windows at any depth.
        desktop_children(hwnd, shell_process, windows);
    }
}

fn capture_exclusions(owned: &[HWND], shell: &[HWND], hidden: &[HWND]) -> Vec<HWND> {
    let mut excluded: Vec<_> = owned.iter().chain(shell).chain(hidden).copied().collect();
    excluded.sort_unstable_by_key(|hwnd| hwnd.0 as usize);
    excluded.dedup();
    excluded
}

fn window_stack(
    excluded: &[HWND],
    shell: &[HWND],
    interest: Rect,
    windows: &mut Vec<DesktopWindow>,
    hidden: &mut Vec<HWND>,
) -> Result<()> {
    struct Enumeration<'a> {
        excluded: &'a [HWND],
        shell: &'a [HWND],
        interest: Rect,
        windows: &'a mut Vec<DesktopWindow>,
        hidden: &'a mut Vec<HWND>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, parameter: LPARAM) -> BOOL {
        // SAFETY: EnumWindows invokes this synchronously while the enumeration and slice are live.
        let enumeration = unsafe { &mut *(parameter.0 as *mut Enumeration<'_>) };
        let mut rect = RECT::default();
        // SAFETY: HWND is supplied by EnumWindows; these calls only query window state.
        unsafe {
            if enumeration.excluded.contains(&hwnd) || enumeration.shell.contains(&hwnd) {
                return true.into();
            }
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                enumeration.hidden.push(hwnd);
                return true.into();
            }
            let owner = GetAncestor(hwnd, GA_ROOTOWNER);
            if !owner.is_invalid() && owner != hwnd && IsIconic(owner).as_bool() {
                enumeration.hidden.push(hwnd);
                return true.into();
            }
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return true.into();
            }
        }
        let bounds = Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        if rect_size(intersection(bounds, enumeration.interest)).is_none() {
            return true.into();
        }
        let mut cloaked = 0_u32;
        // SAFETY: The output is correctly sized; a closed window is ignored by the next refresh.
        if unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        }
        .is_ok()
            && cloaked != 0
        {
            enumeration.hidden.push(hwnd);
            return true.into();
        }
        enumeration.windows.push(DesktopWindow { hwnd, bounds });
        true.into()
    }
    windows.clear();
    hidden.clear();
    let mut enumeration = Enumeration {
        excluded,
        shell,
        interest,
        windows,
        hidden,
    };
    // SAFETY: All callback data remains live until synchronous enumeration returns.
    unsafe {
        EnumWindows(
            Some(collect),
            LPARAM((&mut enumeration as *mut Enumeration<'_>) as isize),
        )?
    };
    enumeration
        .hidden
        .sort_unstable_by_key(|hwnd| hwnd.0 as usize);
    Ok(())
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

pub(super) fn pump_messages() -> bool {
    let mut message = MSG::default();
    // SAFETY: This dedicated worker dispatches only its own queued messages and valid MSG values.
    unsafe {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    WALLPAPER_CHANGED.replace(false)
}
