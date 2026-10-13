//! Optional inspection backend: DXGI GPU capture, region-only staging readback.
//! Native GDI presents a nearest-neighbor bitmap. Live magnification stays separate.
use crate::geometry::Rect;
use windows::{
    core::Interface,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
};
pub struct Capture {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    pub bounds: Rect,
    stage: Option<ID3D11Texture2D>,
    desktop: Option<ID3D11Texture2D>,
    size: (u32, u32),
    pub pixels: Vec<u8>,
    pub source: Rect,
}
impl Capture {
    pub unsafe fn new(bounds: Rect) -> Result<Self, String> {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.to_string())?;
        let mut ai = 0;
        while let Ok(adapter) = factory.EnumAdapters1(ai) {
            let mut oi = 0;
            while let Ok(output) = adapter.EnumOutputs(oi) {
                let desc = output.GetDesc().map_err(|e| e.to_string())?;
                let r = desc.DesktopCoordinates;
                if (r.left, r.top, r.right, r.bottom)
                    == (bounds.left, bounds.top, bounds.right, bounds.bottom)
                {
                    if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
                        && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED
                    {
                        return Err("Pixel inspection on rotated displays is not supported yet; live magnification remains available.".into());
                    }
                    let mut device = None;
                    let mut context = None;
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )
                    .map_err(|e| e.to_string())?;
                    let device = device.ok_or("D3D11 did not return a device")?;
                    let context = context.ok_or("D3D11 did not return a context")?;
                    let out: IDXGIOutput1 = output.cast().map_err(|e| e.to_string())?;
                    let dup = out
                        .DuplicateOutput(&device)
                        .map_err(|e| format!("Desktop duplication unavailable: {e}"))?;
                    return Ok(Self {
                        device,
                        context,
                        dup,
                        bounds,
                        stage: None,
                        desktop: None,
                        size: (0, 0),
                        pixels: vec![],
                        source: Rect::default(),
                    });
                }
                oi += 1;
            }
            ai += 1;
        }
        Err("No DXGI output matches the source monitor".into())
    }
    pub unsafe fn update(&mut self, source: Rect) -> Result<bool, String> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        let fresh = match self.dup.AcquireNextFrame(0, &mut info, &mut resource) {
            Ok(()) => {
                // Keep a GPU-side desktop copy so a new source region works on a static screen.
                let result = (|| -> Result<(), String> {
                    let tex: ID3D11Texture2D = resource
                        .ok_or("No captured texture")?
                        .cast()
                        .map_err(|e| e.to_string())?;
                    if self.desktop.is_none() {
                        let mut desc = D3D11_TEXTURE2D_DESC::default();
                        tex.GetDesc(&mut desc);
                        desc.Usage = D3D11_USAGE_DEFAULT;
                        desc.CPUAccessFlags = 0;
                        desc.BindFlags = 0;
                        desc.MiscFlags = 0;
                        self.device
                            .CreateTexture2D(&desc, None, Some(&mut self.desktop))
                            .map_err(|e| e.to_string())?;
                    }
                    self.context
                        .CopyResource(self.desktop.as_ref().unwrap(), &tex);
                    Ok(())
                })();
                let release = self.dup.ReleaseFrame().map_err(|e| e.to_string());
                result?;
                release?;
                true
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => false,
            Err(e) => return Err(format!("Capture invalidated: {e}")),
        };
        if self.desktop.is_none() {
            return Ok(false);
        }
        if !fresh && self.source == source {
            return Ok(false);
        }
        {
            let tex = self.desktop.as_ref().unwrap();
            let size = (source.width() as u32, source.height() as u32);
            if self.size != size {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: size.0,
                    Height: size.1,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut self.stage))
                    .map_err(|e| e.to_string())?;
                self.size = size;
            }
            let stage = self.stage.as_ref().ok_or("No staging texture")?;
            let area = D3D11_BOX {
                left: (source.left - self.bounds.left) as u32,
                top: (source.top - self.bounds.top) as u32,
                front: 0,
                right: (source.right - self.bounds.left) as u32,
                bottom: (source.bottom - self.bounds.top) as u32,
                back: 1,
            };
            self.context
                .CopySubresourceRegion(stage, 0, 0, 0, 0, tex, 0, Some(&area));
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(stage, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| e.to_string())?;
            self.pixels.resize(size.0 as usize * size.1 as usize * 4, 0);
            for y in 0..size.1 as usize {
                let row = std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                    size.0 as usize * 4,
                );
                self.pixels[y * size.0 as usize * 4..(y + 1) * size.0 as usize * 4]
                    .copy_from_slice(row);
            }
            self.context.Unmap(stage, 0);
            self.source = source;
        }
        Ok(true)
    }
}
