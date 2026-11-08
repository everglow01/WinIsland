use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance};
use windows::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
use windows::Win32::UI::Shell::{
    ACTIVATEOPTIONS, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE,
    SetForegroundWindow, ShowWindow,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};
use winisland_platform::PlatformError;

use crate::com::ComGuard;
use crate::process;

pub(super) fn launch_application(name: &str) -> Result<String, PlatformError> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.contains('\0') {
        return Err(PlatformError::Backend(
            "An application name is required".into(),
        ));
    }
    std::thread::spawn(move || {
        let _com = ComGuard::sta()?;
        if name.starts_with("ms-settings:") {
            super::links::open_url(&name)?;
            return Ok(name);
        }
        let output = Command::new("powershell.exe")
            .creation_flags(0x0800_0000)
            .args([
                "-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
                "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ForEach-Object { $_.Name + [char]9 + $_.AppID }",
            ])
            .stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(PlatformError::backend)?;
        if !output.status.success() {
            return Err(PlatformError::Backend("Cannot enumerate installed applications".into()));
        }
        let normalize = |value: &str| value.chars()
            .filter(|character| !character.is_whitespace() && !matches!(character, '-' | '_'))
            .flat_map(char::to_lowercase).collect::<String>();
        let wanted = normalize(&name);
        if wanted.is_empty() { return Err(PlatformError::Backend("Invalid application name".into())); }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut candidates: Vec<_> = text.lines().filter_map(|line| {
            let (label, id) = line.split_once('\t')?;
            let label = label.trim();
            let id = id.trim();
            let candidate = normalize(label);
            let rank = if candidate == wanted || id.eq_ignore_ascii_case(&name) { 0 }
                else if candidate.starts_with(&wanted) { 1 }
                else if candidate.contains(&wanted) { 2 }
                else { return None; };
            Some((rank, label, id))
        }).collect();
        candidates.sort_unstable();
        let Some(&(rank, label, id)) = candidates.first() else {
            return Err(PlatformError::Backend(format!("No installed application matches '{name}'")));
        };
        if candidates.iter().skip(1).any(|candidate| candidate.0 == rank && candidate.1 != label) {
            let names: Vec<_> = candidates.iter().filter(|candidate| candidate.0 == rank)
                .take(8).map(|candidate| candidate.1).collect();
            return Err(PlatformError::Backend(format!("Several applications match: {}", names.join(", "))));
        }
        super::links::open_url(&format!("shell:AppsFolder\\{id}"))?;
        Ok(label.to_owned())
    }).join().map_err(|_| PlatformError::Backend("Application launch worker panicked".into()))?
}

pub(super) fn application(app_user_model_id: &str) -> Result<bool, PlatformError> {
    if app_user_model_id.is_empty() {
        return Ok(false);
    }
    let app_id = app_user_model_id.to_owned();
    std::thread::spawn(move || activate_on_sta(&app_id))
        .join()
        .map_err(|_| PlatformError::Backend("application activation thread panicked".into()))?
}

fn activate_on_sta(app_user_model_id: &str) -> Result<bool, PlatformError> {
    let _com = ComGuard::sta()?;
    let app_id = HSTRING::from(app_user_model_id);
    // SAFETY: The COM apartment is initialized here, and the app ID outlives the call.
    let result = unsafe {
        let manager: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                .map_err(PlatformError::backend)?;
        manager.ActivateApplication(&app_id, PCWSTR::null(), ACTIVATEOPTIONS::default())
    };
    match result {
        Ok(process_id) => {
            log::info!("Application activated: {app_user_model_id} (process {process_id})");
            Ok(true)
        }
        Err(error) => {
            log::debug!("Application could not be activated: {error:?}");
            Ok(false)
        }
    }
}

struct MediaWindowSearch {
    source_app_id: String,
    executable_name: String,
    window: Option<HWND>,
}

pub(super) fn media_application(source_app_id: &str) -> Result<bool, PlatformError> {
    if source_app_id.is_empty() {
        return Ok(false);
    }
    let executable_name = Path::new(source_app_id)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source_app_id)
        .to_string();
    let mut search = MediaWindowSearch {
        source_app_id: source_app_id.to_string(),
        executable_name,
        window: None,
    };
    // SAFETY: EnumWindows invokes the callback synchronously while search remains live.
    let _ = unsafe {
        EnumWindows(
            Some(find_media_window),
            LPARAM((&mut search as *mut MediaWindowSearch) as isize),
        )
    };
    if let Some(hwnd) = search.window {
        // SAFETY: hwnd was obtained from EnumWindows; both operations retain no caller data.
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            if SetForegroundWindow(hwnd).as_bool() {
                return Ok(true);
            }
        }
    }
    application(source_app_id)
}

unsafe extern "system" fn find_media_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the MediaWindowSearch pointer supplied to synchronous EnumWindows.
    let search = unsafe { &mut *(lparam.0 as *mut MediaWindowSearch) };
    // SAFETY: hwnd is supplied by EnumWindows and is valid for these synchronous queries.
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok() {
        return true.into();
    }
    let mut process_id = 0;
    // SAFETY: hwnd is supplied by EnumWindows and process_id is writable.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id == 0 || process_id == std::process::id() {
        return true.into();
    }
    let Some(process) = process::open(process_id) else {
        return true.into();
    };
    let matches = process::app_user_model_id(*process)
        .is_some_and(|id| id.eq_ignore_ascii_case(&search.source_app_id))
        || process_executable_name(*process)
            .is_some_and(|name| name.eq_ignore_ascii_case(&search.executable_name));
    if matches {
        search.window = Some(hwnd);
        false.into()
    } else {
        true.into()
    }
}

fn process_executable_name(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: The buffer is writable for length UTF-16 code units.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}
