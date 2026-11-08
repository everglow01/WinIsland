use std::any::Any;
use std::sync::Arc;

use skia_safe::{
    ColorType, Surface,
    gpu::{self, DirectContext, SurfaceOrigin, d3d::TextureResourceInfo},
};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D12::D3D12_RESOURCE_STATE_PRESENT,
            DirectComposition::{IDCompositionDevice, IDCompositionTarget, IDCompositionVisual},
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGISwapChain3,
            },
        },
    },
    core::{IUnknown, Interface},
};

use super::D3DDevice;
use super::pacing::FrameGate;
use crate::error::{RenderError, RenderResult};
use crate::surface::NativeSurface;

const BUFFER_COUNT: u32 = 2;

pub(crate) struct RenderTarget {
    surfaces: Vec<Surface>,
    visual: IDCompositionVisual,
    target: IDCompositionTarget,
    swap_chain: IDXGISwapChain3,
    composition: IDCompositionDevice,
    width: u32,
    height: u32,
    flags: DXGI_SWAP_CHAIN_FLAG,
    frame_gate: Option<FrameGate>,
    _keepalive: Option<Arc<dyn Any + Send + Sync>>,
}

impl RenderTarget {
    pub(crate) fn new(
        device: &mut D3DDevice,
        surface: NativeSurface,
        width: u32,
        height: u32,
    ) -> RenderResult<Self> {
        device.validate_size(width, height)?;
        let redraw = surface.redraw_callback();
        let flags = if redraw.is_some() {
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT
        } else {
            DXGI_SWAP_CHAIN_FLAG(0)
        };
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: BUFFER_COUNT,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: flags.0 as u32,
            ..Default::default()
        };
        // SAFETY: The initialized descriptor and retained DIRECT queue satisfy composition swap-chain requirements.
        let swap_chain: IDXGISwapChain3 = unsafe {
            device
                .factory
                .CreateSwapChainForComposition(&device.backend.queue, &desc, None)
        }
        .and_then(|swap_chain| swap_chain.cast())
        .map_err(|error| format!("Composition swap chain creation failed: {error}"))?;
        let frame_gate = if let Some(redraw) = redraw {
            // SAFETY: This swap chain enables latency control and consumes one permit per frame.
            unsafe { swap_chain.SetMaximumFrameLatency(1) }
                .map_err(|error| format!("Presentation queue limit failed: {error}"))?;
            Some(FrameGate::new(&swap_chain, redraw)?)
        } else {
            None
        };
        let surfaces = wrap_buffers(&swap_chain, &mut device.context, width, height)?;
        let hwnd = surface.handle();
        // SAFETY: The window is kept alive by the retained keepalive below, and each window has one
        // foreground composition target.
        let target = unsafe {
            device
                .composition
                .CreateTargetForHwnd(HWND(hwnd as *mut _), true)
        }
        .map_err(|error| format!("DirectComposition target creation failed: {error}"))?;
        // SAFETY: The composition device is live and returns an owned visual.
        let visual = unsafe { device.composition.CreateVisual() }
            .map_err(|error| format!("DirectComposition visual creation failed: {error}"))?;
        let target = Self {
            surfaces,
            visual,
            target,
            swap_chain,
            composition: device.composition.clone(),
            width,
            height,
            flags,
            frame_gate,
            _keepalive: surface.into_keepalive(),
        };
        // SAFETY: All interfaces are retained by target, which detaches them even if attachment fails.
        unsafe {
            target
                .visual
                .SetContent(&target.swap_chain)
                .and_then(|()| target.target.SetRoot(&target.visual))
                .and_then(|()| target.composition.Commit())
        }
        .map_err(|error| format!("DirectComposition attachment failed: {error}"))?;
        Ok(target)
    }

    pub(crate) fn prepare_frame(&mut self) -> RenderResult<bool> {
        match &mut self.frame_gate {
            Some(gate) => gate.poll(),
            None => Ok(true),
        }
    }

    pub(crate) fn current_surface(&mut self) -> RenderResult<&mut Surface> {
        if self
            .frame_gate
            .as_ref()
            .is_some_and(|gate| !gate.permitted())
        {
            return Err("Frame preparation is required before drawing"
                .to_string()
                .into());
        }
        // SAFETY: The swap chain is retained and accessed only on the render thread.
        let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() } as usize;
        self.surfaces
            .get_mut(index)
            .ok_or_else(|| RenderError::Backend("DXGI back buffer is unavailable".to_string()))
    }

    pub(crate) fn present(&mut self) -> RenderResult<()> {
        // SAFETY: Renderer flushes the current surface with Present access and submits on this swap chain's queue.
        unsafe { self.swap_chain.Present(1, DXGI_PRESENT(0)).ok() }
            .map_err(|error| format!("DXGI presentation failed: {error}"))?;
        if let Some(gate) = &mut self.frame_gate {
            gate.submitted();
        }
        Ok(())
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn release_buffers(&mut self) {
        self.surfaces.clear();
    }

    pub(crate) fn resize(
        &mut self,
        context: &mut DirectContext,
        width: u32,
        height: u32,
    ) -> RenderResult<()> {
        // SAFETY: D3DDevice synchronized the queue and released all Skia back-buffer references.
        unsafe {
            self.swap_chain.ResizeBuffers(
                BUFFER_COUNT,
                width,
                height,
                DXGI_FORMAT_R8G8B8A8_UNORM,
                self.flags,
            )
        }
        .map_err(|error| format!("DXGI buffer resize failed: {error}"))?;
        self.surfaces = wrap_buffers(&self.swap_chain, context, width, height)?;
        self.width = width;
        self.height = height;
        Ok(())
    }
}

impl Drop for RenderTarget {
    fn drop(&mut self) {
        drop(self.frame_gate.take());
        // SAFETY: The composition interfaces and window remain live throughout detachment.
        unsafe {
            let root_result = self.target.SetRoot(None::<&IDCompositionVisual>);
            let content_result = self.visual.SetContent(None::<&IUnknown>);
            let commit_result = self
                .composition
                .Commit()
                .and_then(|()| self.composition.WaitForCommitCompletion());
            if let Err(error) = root_result.and(content_result).and(commit_result) {
                log::warn!("DirectComposition detachment failed: {error}");
            }
        }
    }
}

fn wrap_buffers(
    swap_chain: &IDXGISwapChain3,
    context: &mut DirectContext,
    width: u32,
    height: u32,
) -> RenderResult<Vec<Surface>> {
    (0..BUFFER_COUNT)
        .map(|index| {
            // SAFETY: index is within the swap chain's buffer count; the returned resource is owned.
            let resource = unsafe { swap_chain.GetBuffer(index) }
                .map_err(|error| format!("DXGI buffer {index} acquisition failed: {error}"))?;
            let info = TextureResourceInfo {
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                level_count: 1,
                ..TextureResourceInfo::from_resource(resource)
                    .with_state(D3D12_RESOURCE_STATE_PRESENT)
            };
            let backend =
                gpu::backend_render_targets::make_d3d((width as i32, height as i32), &info);
            gpu::surfaces::wrap_backend_render_target(
                context,
                &backend,
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .ok_or_else(|| {
                RenderError::Backend(format!("Skia D3D12 buffer {index} wrapping failed"))
            })
        })
        .collect()
}
