use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::System::DispatcherQueueController;
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND, NTSTATUS, RECT, SIZE, WAIT_TIMEOUT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Device, ID2D1Factory1,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Device5;
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDesktopDevice, IDCompositionVisual2,
};
use windows::Win32::Graphics::Dwm::{DWM_THUMBNAIL_PROPERTIES, DwmUnregisterThumbnail};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::{
    BOOL, Error, GUID, HRESULT, IInspectable_Vtbl, IUnknown_Vtbl, Interface, PCSTR, Result, s, w,
};

// ABI reference: https://gist.github.com/ADeltaX/c0e565a50d2cedb62dab5c13674cb8b5
windows::core::imp::define_interface!(
    CompositorFactory,
    CompositorFactoryVtbl,
    0x22118adf_23f1_4801_bcfa_66cbf48cc51b
);
#[repr(C)]
pub struct CompositorFactoryVtbl {
    base: IInspectable_Vtbl,
    create: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *const GUID,
        *mut *mut c_void,
    ) -> HRESULT,
    enabled: unsafe extern "system" fn(*mut c_void, *mut bool, *mut bool) -> HRESULT,
}

windows::core::imp::define_interface!(
    CompositorPartner,
    CompositorPartnerVtbl,
    0xe7894c70_af56_4f52_b382_4b3cd263dc6f
);
#[repr(C)]
pub struct CompositorPartnerVtbl {
    base: IUnknown_Vtbl,
    mark_dirty: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    clear_callback: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    create_transform: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const GUID,
        *mut *mut c_void,
    ) -> HRESULT,
    close: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

pub(super) struct Apartment {
    dispatcher: Option<DispatcherQueueController>,
}

impl Apartment {
    pub(super) fn new() -> Result<Self> {
        // SAFETY: The capture worker is a dedicated thread; this guard balances initialization.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
        let mut apartment = Self { dispatcher: None };
        let options = DispatcherQueueOptions {
            dwSize: u32::try_from(size_of::<DispatcherQueueOptions>())
                .map_err(|_| Error::new(E_INVALIDARG, "Invalid dispatcher options size"))?,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        };
        // SAFETY: This worker has initialized WinRT and owns its message loop and queue shutdown.
        apartment.dispatcher = Some(unsafe { CreateDispatcherQueueController(options)? });
        Ok(apartment)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if let Some(dispatcher) = self.dispatcher.take()
            && let Err(error) = shutdown_dispatcher(&dispatcher)
        {
            log::warn!("Capture dispatcher shutdown failed: {error}");
        }
        // SAFETY: All worker resources are released before this guard on its initializing thread.
        unsafe { RoUninitialize() };
    }
}

fn shutdown_dispatcher(dispatcher: &DispatcherQueueController) -> Result<()> {
    let shutdown = dispatcher.ShutdownQueueAsync()?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    shutdown.when(move |result| {
        let _ = sender.send(result);
    })?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        super::window::pump_messages();
        match receiver.recv_timeout(Duration::from_millis(1)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::new(E_FAIL, "Capture dispatcher completion was lost"));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                return Err(Error::from_hresult(HRESULT::from_win32(WAIT_TIMEOUT.0)));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(super) struct Composition {
    pub(super) compositor: Compositor,
    pub(super) native: IDCompositionDesktopDevice,
    pub(super) drawing: ID2D1Device,
    partner: CompositorPartner,
}

impl Composition {
    pub(super) fn new(device: &ID3D11Device5) -> Result<Self> {
        let dxgi: IDXGIDevice = device.cast()?;
        // SAFETY: The D2D factory and its device stay on this worker. The compositor retains the device.
        let d2d = unsafe {
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            factory.CreateDevice(&dxgi)?
        };
        let factory: CompositorFactory = windows::core::factory::<Compositor, CompositorFactory>()?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: The private factory ABI matches its IID; the device is live and the output is owned.
        unsafe {
            (factory.vtable().create)(
                factory.as_raw(),
                d2d.as_raw(),
                std::ptr::null_mut(),
                &CompositorPartner::IID,
                &mut raw,
            )
            .ok()?;
        }
        if raw.is_null() {
            return Err(Error::new(E_FAIL, "Capture compositor is missing"));
        }
        // SAFETY: CreateInteropCompositor transferred one reference of the requested interface.
        let partner = unsafe { CompositorPartner::from_raw(raw) };
        let interfaces = partner
            .cast()
            .and_then(|compositor| partner.cast().map(|native| (compositor, native)));
        match interfaces {
            Ok((compositor, native)) => Ok(Self {
                compositor,
                native,
                drawing: d2d,
                partner,
            }),
            Err(error) => {
                close_compositor(&partner);
                Err(error)
            }
        }
    }
}

impl Drop for Composition {
    fn drop(&mut self) {
        close_compositor(&self.partner);
    }
}

fn close_compositor(partner: &CompositorPartner) {
    // SAFETY: The owned partner is closed once on its creating worker, after capture and target close.
    if let Err(error) = unsafe { (partner.vtable().close)(partner.as_raw()).ok() } {
        log::debug!("Capture compositor close failed: {error}");
    }
}

#[repr(C)]
struct WindowAttribute {
    attribute: i32,
    data: *mut c_void,
    size: usize,
}

type SetAttribute = unsafe extern "system" fn(HWND, *const WindowAttribute) -> BOOL;
type CreateDesktop =
    unsafe extern "system" fn(HWND, *mut c_void, *mut *mut c_void, *mut isize) -> HRESULT;
type UpdateDesktop = unsafe extern "system" fn(
    isize,
    *const HWND,
    u32,
    *const HWND,
    u32,
    *mut RECT,
    *mut SIZE,
) -> HRESULT;
type UpdateMultiWindow = unsafe extern "system" fn(
    isize,
    *const HWND,
    u32,
    *const HWND,
    u32,
    *mut RECT,
    *mut SIZE,
    u32,
) -> HRESULT;

pub(super) struct Thumbnail(isize);

enum DesktopUpdate {
    Legacy(UpdateDesktop),
    MultiWindow(UpdateMultiWindow),
}

pub(super) struct Desktop {
    host: HWND,
    thumbnail: Thumbnail,
    update: DesktopUpdate,
    excluded: Vec<HWND>,
    source: RECT,
    size: SIZE,
}

impl Desktop {
    pub(super) fn new(
        host: HWND,
        device: &IDCompositionDesktopDevice,
        excluded: &[HWND],
        source: RECT,
        size: SIZE,
    ) -> Result<(Self, IDCompositionVisual2)> {
        // SAFETY: Both system DLLs are loaded by this crate's static imports and remain loaded.
        let (create, update, setter) = unsafe {
            let dwm = GetModuleHandleW(w!("dwmapi.dll"))?;
            let user = GetModuleHandleW(w!("user32.dll"))?;
            (
                GetProcAddress(dwm, PCSTR(163_usize as *const u8)),
                GetProcAddress(dwm, PCSTR(164_usize as *const u8)),
                GetProcAddress(user, s!("SetWindowCompositionAttribute")),
            )
        };
        let create =
            create.ok_or_else(|| Error::new(E_FAIL, "Shared desktop creation is unavailable"))?;
        let update =
            update.ok_or_else(|| Error::new(E_FAIL, "Shared desktop update is unavailable"))?;
        let setter =
            setter.ok_or_else(|| Error::new(E_FAIL, "Desktop preview exclusion is unavailable"))?;
        // SAFETY: These exports have the declared ABI on the supported Windows 10/11 builds.
        let (create, setter) = unsafe {
            (
                std::mem::transmute::<unsafe extern "system" fn() -> isize, CreateDesktop>(create),
                std::mem::transmute::<unsafe extern "system" fn() -> isize, SetAttribute>(setter),
            )
        };
        let mut enabled = BOOL(1);
        let attribute = WindowAttribute {
            attribute: 13, // WCA_EXCLUDED_FROM_LIVEPREVIEW is required only on the private host.
            data: (&mut enabled as *mut BOOL).cast(),
            size: size_of::<BOOL>(),
        };
        // SAFETY: The host belongs to this worker; attribute points to a live BOOL for this call.
        if !unsafe { setter(host, &attribute) }.as_bool() {
            return Err(Error::from_thread());
        }
        let mut visual = std::ptr::null_mut();
        let mut handle = 0;
        // SAFETY: The host and device are live; outputs are initialized and transferred on success.
        unsafe { create(host, device.as_raw(), &mut visual, &mut handle).ok()? };
        let thumbnail = Thumbnail(handle);
        if visual.is_null() {
            return Err(Error::new(E_FAIL, "Shared desktop visual is missing"));
        }
        // SAFETY: The export returns one owned IDCompositionVisual2 reference.
        let visual = unsafe { IDCompositionVisual2::from_raw(visual) };
        let build = windows_build()?;
        log::info!("Liquid glass DWM capture backend: Windows build {build}");
        // SAFETY: The ordinal gained a final flags argument in build 20xxx; select its matching ABI.
        let update = unsafe {
            if build < 20000 {
                DesktopUpdate::Legacy(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    UpdateDesktop,
                >(update))
            } else {
                DesktopUpdate::MultiWindow(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    UpdateMultiWindow,
                >(update))
            }
        };
        let mut desktop = Self {
            host,
            thumbnail,
            update,
            excluded: excluded.to_vec(),
            source,
            size,
        };
        desktop.refresh()?;
        Ok((desktop, visual))
    }

    pub(super) fn refresh(&mut self) -> Result<()> {
        // Exclusion HWNDs can disappear when the shell or companion windows are recreated.
        self.excluded.retain(|hwnd| {
            // SAFETY: IsWindow validates the opaque handle without accessing caller memory.
            unsafe { IsWindow(Some(*hwnd)).as_bool() }
        });
        let count = u32::try_from(self.excluded.len())
            .map_err(|_| Error::new(E_INVALIDARG, "Too many excluded windows"))?;
        // SAFETY: The stored function matches this OS's ABI. All arrays and outputs are owned
        // by this desktop, and its host and thumbnail remain alive throughout the call.
        unsafe {
            match self.update {
                DesktopUpdate::Legacy(update) => update(
                    self.thumbnail.0,
                    std::ptr::null(),
                    0,
                    self.excluded.as_ptr(),
                    count,
                    &mut self.source,
                    &mut self.size,
                )
                .ok(),
                DesktopUpdate::MultiWindow(update) => update(
                    self.thumbnail.0,
                    std::ptr::null(),
                    0,
                    self.excluded.as_ptr(),
                    count,
                    &mut self.source,
                    &mut self.size,
                    1,
                )
                .ok(),
            }
        }
    }

    pub(super) fn recreate(
        &self,
        device: &IDCompositionDesktopDevice,
    ) -> Result<(Self, IDCompositionVisual2)> {
        Self::new(self.host, device, &self.excluded, self.source, self.size)
    }

    pub(super) fn set_excluded(&mut self, excluded: Vec<HWND>) -> Result<()> {
        if self.excluded == excluded {
            return Ok(());
        }
        self.excluded = excluded;
        self.refresh()
    }
}

impl Thumbnail {
    pub(super) fn window(
        host: HWND,
        source: HWND,
        device: &IDCompositionDesktopDevice,
        properties: &DWM_THUMBNAIL_PROPERTIES,
    ) -> Result<(Self, IDCompositionVisual2)> {
        type Create = unsafe extern "system" fn(
            HWND,
            HWND,
            u32,
            *const DWM_THUMBNAIL_PROPERTIES,
            *mut c_void,
            *mut *mut c_void,
            *mut isize,
        ) -> HRESULT;
        // SAFETY: dwmapi is statically imported; ordinal 147 has this shared-thumbnail ABI.
        let create: Create = unsafe {
            let module = GetModuleHandleW(w!("dwmapi.dll"))?;
            let address = GetProcAddress(module, PCSTR(147_usize as *const u8))
                .ok_or_else(|| Error::new(E_FAIL, "Desktop wallpaper visual is unavailable"))?;
            std::mem::transmute(address)
        };
        let mut visual = std::ptr::null_mut();
        let mut handle = 0;
        // SAFETY: Both HWNDs and the device are live, properties are initialized, and outputs
        // receive owned references. DWM validates source lifetime if Explorer restarts.
        unsafe {
            create(
                host,
                source,
                2,
                properties,
                device.as_raw(),
                &mut visual,
                &mut handle,
            )
            .ok()?
        };
        let thumbnail = Self(handle);
        if visual.is_null() {
            return Err(Error::new(E_FAIL, "Desktop wallpaper visual is missing"));
        }
        // SAFETY: The private export transferred one owned IDCompositionVisual2 reference.
        Ok((thumbnail, unsafe { IDCompositionVisual2::from_raw(visual) }))
    }
}

impl Drop for Thumbnail {
    fn drop(&mut self) {
        // SAFETY: The handle was created by DWM and is released before its destination host.
        if let Err(error) = unsafe { DwmUnregisterThumbnail(self.0) } {
            log::debug!("Capture thumbnail release failed: {error}");
        }
    }
}

fn windows_build() -> Result<u32> {
    type Version = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> NTSTATUS;
    // SAFETY: ntdll is loaded for the process lifetime; RtlGetVersion has the declared system ABI.
    let version: Version = unsafe {
        let module = GetModuleHandleW(w!("ntdll.dll"))?;
        let function = GetProcAddress(module, s!("RtlGetVersion"))
            .ok_or_else(|| Error::new(E_FAIL, "Windows build query is unavailable"))?;
        std::mem::transmute(function)
    };
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>())
            .map_err(|_| Error::new(E_FAIL, "Invalid Windows version structure size"))?,
        ..Default::default()
    };
    // SAFETY: info is writable, initialized, and carries the exact structure size.
    unsafe { version(&mut info).ok()? };
    Ok(info.dwBuildNumber)
}
