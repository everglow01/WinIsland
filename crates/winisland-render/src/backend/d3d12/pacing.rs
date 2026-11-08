use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain3;
use windows::Win32::System::Threading::{
    CloseThreadpoolWait, CreateThreadpoolWait, PTP_CALLBACK_INSTANCE, PTP_WAIT, SetThreadpoolWait,
    WaitForSingleObject, WaitForThreadpoolWaitCallbacks,
};
use windows::core::Owned;

use crate::error::RenderResult;

struct Notification {
    ready: AtomicBool,
    status: AtomicU32,
    redraw: Arc<dyn Fn() + Send + Sync>,
}

pub(super) struct FrameGate {
    event: Owned<HANDLE>,
    wait: PTP_WAIT,
    notification: Arc<Notification>,
    waiting: bool,
    permitted: bool,
}

impl FrameGate {
    pub(super) fn new(
        swap_chain: &IDXGISwapChain3,
        redraw: Arc<dyn Fn() + Send + Sync>,
    ) -> RenderResult<Self> {
        // SAFETY: The swap chain enables latency notification and transfers ownership of this handle.
        let event = unsafe { swap_chain.GetFrameLatencyWaitableObject() };
        if event.is_invalid() {
            return Err("Swap-chain frame readiness handle is missing"
                .to_string()
                .into());
        }
        // SAFETY: The returned handle is closed once, after its wait has been cancelled.
        let event = unsafe { Owned::new(event) };
        let notification = Arc::new(Notification {
            ready: AtomicBool::new(false),
            status: AtomicU32::new(WAIT_OBJECT_0.0),
            redraw,
        });
        // SAFETY: The Arc allocation stays fixed and alive until Drop joins all callbacks.
        let wait = unsafe {
            CreateThreadpoolWait(
                Some(frame_ready),
                Some(Arc::as_ptr(&notification).cast_mut().cast()),
                None,
            )
        }
        .map_err(|error| format!("Frame readiness wait creation failed: {error}"))?;
        Ok(Self {
            event,
            wait,
            notification,
            waiting: false,
            permitted: false,
        })
    }

    pub(super) fn poll(&mut self) -> RenderResult<bool> {
        if self.permitted {
            return Ok(true);
        }
        let status = if self.waiting {
            if !self.notification.ready.swap(false, Ordering::Acquire) {
                return Ok(false);
            }
            self.waiting = false;
            self.notification.status.load(Ordering::Relaxed)
        } else {
            // SAFETY: This owned DXGI handle is live. A zero timeout never stalls the UI thread.
            unsafe { WaitForSingleObject(*self.event, 0) }.0
        };
        if status == WAIT_OBJECT_0.0 {
            self.permitted = true;
            return Ok(true);
        }
        if status != WAIT_TIMEOUT.0 {
            return Err(format!("Frame readiness wait failed: {status:#x}").into());
        }
        self.waiting = true;
        let due = -1_000_000_i64;
        let timeout = FILETIME {
            dwLowDateTime: due as u32,
            dwHighDateTime: (due >> 32) as u32,
        };
        // SAFETY: The wait owns its context and copies this relative 100ms timeout. A timeout
        // requests another health check if a removed device stops signaling its frame semaphore.
        unsafe { SetThreadpoolWait(self.wait, Some(*self.event), Some(&timeout)) };
        Ok(false)
    }

    pub(super) fn permitted(&self) -> bool {
        self.permitted
    }

    pub(super) fn submitted(&mut self) {
        self.permitted = false;
    }
}

impl Drop for FrameGate {
    fn drop(&mut self) {
        // SAFETY: Cancel and join before releasing the callback context or event. The callback
        // only posts a redraw notification, so it never waits for this UI thread.
        unsafe {
            SetThreadpoolWait(self.wait, None, None);
            WaitForThreadpoolWaitCallbacks(self.wait, true);
            CloseThreadpoolWait(self.wait);
        }
    }
}

unsafe extern "system" fn frame_ready(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
    _wait: PTP_WAIT,
    status: u32,
) {
    // SAFETY: FrameGate retains this allocation and joins the callback before dropping it.
    let notification = unsafe { &*context.cast::<Notification>() };
    notification.status.store(status, Ordering::Relaxed);
    notification.ready.store(true, Ordering::Release);
    (notification.redraw)();
}
