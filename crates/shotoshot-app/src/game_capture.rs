//! ROLE: Capture one monitor through DXGI without activating a window.
//! OWNER: Each request owns and releases its D3D device and duplication session.
//! INVARIANTS: No idle capture, no disk/network IO, bounded frame/readback wait.
use crate::ImageExt;
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            Dxgi::{Common::*, *},
            Gdi::HMONITOR,
        },
    },
    core::*,
};

const CAPTURE_WAIT: Duration = Duration::from_millis(500);
const PROTECTED_CONTENT: &str = "Windows blocked protected content in this capture";

struct AcquiredFrame<'a>(&'a IDXGIOutputDuplication);
impl Drop for AcquiredFrame<'_> {
    fn drop(&mut self) {
        // PITFALL: Every successful AcquireNextFrame must release, including error paths.
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}

struct MappedTexture<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}
impl Drop for MappedTexture<'_> {
    fn drop(&mut self) {
        // PITFALL: Keep the texture and context alive until mapped memory is no longer read.
        unsafe {
            self.context.Unmap(self.texture, 0);
        }
    }
}

/// The caller supplies the target monitor before moving this work off the UI thread.
pub fn capture_monitor(monitor: HMONITOR) -> Result<crate::Image> {
    // PITFALL: COM objects and mapped GPU memory remain local to this capture request.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapter_index = 0;
        loop {
            let adapter = match factory.EnumAdapters1(adapter_index) {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(error),
            };
            adapter_index += 1;
            let mut output_index = 0;
            loop {
                let output = match adapter.EnumOutputs(output_index) {
                    Ok(output) => output,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(error),
                };
                output_index += 1;
                let description = output.GetDesc()?;
                if description.Monitor != monitor || !description.AttachedToDesktop.as_bool() {
                    continue;
                }
                return capture_output(&adapter, &output.cast()?, &description);
            }
        }
        Err(Error::new(
            DXGI_ERROR_NOT_FOUND,
            "The game monitor is no longer available",
        ))
    }
}

fn capture_output(
    adapter: &IDXGIAdapter1,
    output: &IDXGIOutput1,
    output_desc: &DXGI_OUTPUT_DESC,
) -> Result<crate::Image> {
    // PITFALL: A duplication device must be created on the adapter owning this output.
    unsafe {
        let mut device = None;
        let mut context = None;
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let duplication = output.DuplicateOutput(&device)?;
        let deadline = Instant::now() + CAPTURE_WAIT;
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::from_hresult(DXGI_ERROR_WAIT_TIMEOUT));
            }
            duplication.AcquireNextFrame(remaining.as_millis() as u32, &mut info, &mut resource)?;
            if info.LastPresentTime != 0 {
                break;
            }
            // PITFALL: A pointer-only update does not establish an initial desktop image.
            duplication.ReleaseFrame()?;
            resource = None;
        }
        let _frame = AcquiredFrame(&duplication);
        if info.ProtectedContentMaskedOut.as_bool() {
            return Err(Error::new(E_FAIL, PROTECTED_CONTENT));
        }
        let source: ID3D11Texture2D = resource
            .ok_or_else(|| Error::from_hresult(E_FAIL))?
            .cast()?;
        let mut image = read_back(&device, &context, &source, output_desc.Rotation, deadline)?;
        image.left = output_desc.DesktopCoordinates.left;
        image.top = output_desc.DesktopCoordinates.top;
        Ok(image)
    }
}

/// Copies a frame out of video memory. Apart from the duplication above, so that it is
/// checked on a texture of known content: no screen, no monitor, any Direct3D device.
fn read_back(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    source: &ID3D11Texture2D,
    rotation: DXGI_MODE_ROTATION,
    deadline: Instant,
) -> Result<crate::Image> {
    unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        source.GetDesc(&mut desc);
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(Error::new(
                E_FAIL,
                "Unsupported desktop duplication pixel format",
            ));
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        context.CopyResource(&staging, source);
        context.Flush();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        loop {
            match context.Map(
                &staging,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            ) {
                Ok(()) => break,
                Err(error)
                    if error.code() == DXGI_ERROR_WAS_STILL_DRAWING
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => return Err(error),
            }
        }
        let _mapped = MappedTexture {
            context,
            texture: &staging,
        };
        let length = (mapped.RowPitch as usize)
            .checked_mul(desc.Height as usize)
            .filter(|&length| length <= isize::MAX as usize)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        if mapped.pData.is_null() {
            return Err(Error::from_hresult(E_FAIL));
        }
        let bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), length);
        unpack(
            bytes,
            desc.Width,
            desc.Height,
            mapped.RowPitch as usize,
            rotation,
        )
    }
}

fn unpack(
    bytes: &[u8],
    width: u32,
    height: u32,
    pitch: usize,
    rotation: DXGI_MODE_ROTATION,
) -> Result<crate::Image> {
    let invalid = || Error::new(E_INVALIDARG, "Invalid desktop surface layout");
    let row = (width as usize).checked_mul(4).ok_or_else(invalid)?;
    let required = (height as usize)
        .checked_sub(1)
        .and_then(|rows| rows.checked_mul(pitch))
        .and_then(|offset| offset.checked_add(row))
        .ok_or_else(invalid)?;
    if width == 0 || height == 0 || pitch < row || bytes.len() < required {
        return Err(invalid());
    }
    let (out_width, out_height) = match rotation {
        DXGI_MODE_ROTATION_IDENTITY
        | DXGI_MODE_ROTATION_UNSPECIFIED
        | DXGI_MODE_ROTATION_ROTATE180 => (width, height),
        DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (height, width),
        _ => return Err(invalid()),
    };
    let length = row.checked_mul(height as usize).ok_or_else(invalid)?;
    let mut image = crate::Image {
        width: out_width,
        height: out_height,
        pixels: vec![0; length],
        left: 0,
        top: 0,
    };
    if rotation == DXGI_MODE_ROTATION_IDENTITY || rotation == DXGI_MODE_ROTATION_UNSPECIFIED {
        for (y, destination) in image.pixels.chunks_exact_mut(row).enumerate() {
            destination.copy_from_slice(&bytes[y * pitch..y * pitch + row]);
            for pixel in destination.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        image.validate()?;
        return Ok(image);
    }
    for y in 0..height {
        for x in 0..width {
            let (out_x, out_y) = match rotation {
                DXGI_MODE_ROTATION_ROTATE90 => (height - 1 - y, x),
                DXGI_MODE_ROTATION_ROTATE180 => (width - 1 - x, height - 1 - y),
                DXGI_MODE_ROTATION_ROTATE270 => (y, width - 1 - x),
                _ => (x, y),
            };
            let source = y as usize * pitch + x as usize * 4;
            let destination = (out_y as usize * out_width as usize + out_x as usize) * 4;
            image.pixels[destination..destination + 3].copy_from_slice(&bytes[source..source + 3]);
            image.pixels[destination + 3] = 255;
        }
    }
    image.validate()?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Three by two pixels, four padding bytes after the first row, alpha left at zero as a
    // duplicated desktop gives it. The first byte of each pixel numbers it.
    const SURFACE: [u8; 28] = [
        1, 10, 20, 0, 2, 11, 21, 0, 3, 12, 22, 0, 99, 99, 99, 99, 4, 13, 23, 0, 5, 14, 24, 0, 6,
        15, 25, 0,
    ];
    fn numbers(image: &crate::Image) -> Vec<u8> {
        let pixels = image.pixels.as_chunks::<4>().0;
        assert!(pixels.iter().all(|pixel| pixel[3] == 255));
        pixels.iter().map(|pixel| pixel[0]).collect()
    }

    /// A window protected from capture anywhere on the monitor (a password manager, the toast
    /// of a test run next door) blanks the frame. That says nothing about the duplication:
    /// wait for it to leave, and fail only if it stays.
    fn capture_past_protected_content(monitor: HMONITOR) -> crate::Image {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match capture_monitor(monitor) {
                Err(error) if error.message() == PROTECTED_CONTENT && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                outcome => return outcome.expect("native GPU capture"),
            }
        }
    }

    /// The one test that needs a real display, since Windows duplicates nothing else. A plain
    /// `cargo test` skips it; `check.ps1` runs it on its own.
    #[test]
    #[ignore = "real desktop: needs a display Windows can duplicate, run alone by check.ps1"]
    fn real_desktop_capture_releases_duplication_and_preserves_monitor_dimensions() {
        use windows::Win32::{Foundation::POINT, Graphics::Gdi::*};
        // PITFALL: Read desktop pixels only into memory; tests never save or display them.
        unsafe {
            let monitor = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            assert!(GetMonitorInfoW(monitor, &mut info).as_bool());
            for _ in 0..2 {
                let started = Instant::now();
                let image = capture_past_protected_content(monitor);
                eprintln!(
                    "GPU capture: {} ms, {}x{}",
                    started.elapsed().as_millis(),
                    image.width,
                    image.height
                );
                assert_eq!(
                    (image.left, image.top),
                    (info.rcMonitor.left, info.rcMonitor.top)
                );
                assert_eq!(
                    (image.width, image.height),
                    (
                        (info.rcMonitor.right - info.rcMonitor.left) as u32,
                        (info.rcMonitor.bottom - info.rcMonitor.top) as u32
                    )
                );
                image.validate().expect("valid GPU image");
                assert!(
                    image
                        .pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|pixel| pixel[3] == 255)
                );
            }
        }
    }

    #[test]
    fn a_frame_is_read_back_from_video_memory_without_a_screen() {
        // WARP is the software Direct3D device shipped with Windows: no GPU, no display.
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .expect("software Direct3D device");
            let (device, context) = (device.unwrap(), context.unwrap());
            // Described as Windows describes a duplicated frame.
            let desc = D3D11_TEXTURE2D_DESC {
                Width: 3,
                Height: 2,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let content = D3D11_SUBRESOURCE_DATA {
                pSysMem: SURFACE.as_ptr().cast(),
                SysMemPitch: 16,
                SysMemSlicePitch: 0,
            };
            let mut frame = None;
            device
                .CreateTexture2D(&desc, Some(&content), Some(&mut frame))
                .expect("frame texture");
            let frame = frame.unwrap();
            for (rotation, size, expected) in [
                (DXGI_MODE_ROTATION_IDENTITY, (3, 2), [1, 2, 3, 4, 5, 6]),
                (DXGI_MODE_ROTATION_ROTATE90, (2, 3), [4, 1, 5, 2, 6, 3]),
            ] {
                let deadline = Instant::now() + CAPTURE_WAIT;
                let image =
                    read_back(&device, &context, &frame, rotation, deadline).expect("read back");
                assert_eq!((image.width, image.height), size);
                assert_eq!(numbers(&image), expected);
            }
        }
    }

    #[test]
    fn stride_rotation_and_alpha_are_correct() {
        for (rotation, width, height, expected) in [
            (DXGI_MODE_ROTATION_IDENTITY, 3, 2, vec![1, 2, 3, 4, 5, 6]),
            (DXGI_MODE_ROTATION_ROTATE90, 2, 3, vec![4, 1, 5, 2, 6, 3]),
            (DXGI_MODE_ROTATION_ROTATE180, 3, 2, vec![6, 5, 4, 3, 2, 1]),
            (DXGI_MODE_ROTATION_ROTATE270, 2, 3, vec![3, 6, 2, 5, 1, 4]),
        ] {
            let image = unpack(&SURFACE, 3, 2, 16, rotation).expect("valid surface");
            assert_eq!((image.width, image.height), (width, height));
            assert_eq!(numbers(&image), expected);
            image.validate().expect("valid image");
        }
    }

    #[test]
    fn invalid_surface_layouts_are_rejected() {
        assert!(unpack(&[0; 24], 3, 2, 8, DXGI_MODE_ROTATION_IDENTITY).is_err());
        assert!(unpack(&[0; 23], 3, 2, 12, DXGI_MODE_ROTATION_IDENTITY).is_err());
        assert!(unpack(&[], 0, 0, 0, DXGI_MODE_ROTATION_IDENTITY).is_err());
        assert!(unpack(&[0; 24], 3, 2, 12, DXGI_MODE_ROTATION(99)).is_err());
    }
}
