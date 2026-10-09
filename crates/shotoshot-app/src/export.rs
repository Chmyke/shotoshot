use crate::{Image, ImageExt};
use shotoshot_core::settings::SaveFormat;
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Imaging::*,
        Storage::FileSystem::*,
        System::{
            Com::{StructuredStorage::*, *},
            DataExchange::*,
            Memory::*,
            Variant::*,
        },
    },
    core::*,
};

fn factory() -> Result<IWICImagingFactory> {
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
}
pub fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
pub fn save_png(image: &Image, path: &Path) -> Result<()> {
    save_image(image, path, SaveFormat::Png)
}
/// Writes a file under a temporary name beside `requested`, then moves it into place: whole
/// or not at all. A numbered capture never replaces another, even if another editor saved
/// after the dialog opened: it takes the next number. Returns where the file went.
fn publish(
    requested: &Path,
    numbered: bool,
    write: impl FnOnce(&Path) -> Result<()>,
) -> Result<std::path::PathBuf> {
    let temporary = requested.with_extension(format!("{}.tmp", std::process::id()));
    let result = write(&temporary).and_then(|()| {
        let mut destination = requested.to_path_buf();
        let flags = if numbered {
            MOVEFILE_WRITE_THROUGH
        } else {
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
        };
        loop {
            match unsafe {
                MoveFileExW(
                    PCWSTR(wide(&temporary).as_ptr()),
                    PCWSTR(wide(&destination).as_ptr()),
                    flags,
                )
            } {
                Ok(()) => return Ok(destination),
                Err(error)
                    if numbered
                        && (error.code() == ERROR_ALREADY_EXISTS.to_hresult()
                            || error.code() == ERROR_FILE_EXISTS.to_hresult()) =>
                {
                    // Every kind of file shares the sequence and keeps its own extension.
                    destination = crate::capture_name::next_path(
                        requested.parent().unwrap_or(Path::new(".")),
                        SaveFormat::Png,
                    )
                    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
                    if let Some(extension) = requested.extension() {
                        destination.set_extension(extension);
                    }
                }
                Err(error) => return Err(error),
            }
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
/// Writes `image` in `format`; `path` is replaced only by a complete file.
pub fn save_image(image: &Image, path: &Path, format: SaveFormat) -> Result<()> {
    image.validate()?;
    publish(path, false, |temporary| encode(image, temporary, format)).map(|_| ())
}
// Never replace a numbered capture, even if another editor saved after the dialog opened.
pub fn save_numbered(
    image: &Image,
    requested: &Path,
    format: SaveFormat,
) -> Result<std::path::PathBuf> {
    image.validate()?;
    publish(requested, true, |temporary| {
        encode(image, temporary, format)
    })
}
/// Saves an already encoded file; `numbered` as for `save_numbered`.
pub fn save_bytes(bytes: &[u8], path: &Path, numbered: bool) -> Result<std::path::PathBuf> {
    publish(path, numbered, |temporary| {
        std::fs::write(temporary, bytes).map_err(|e| Error::new(E_FAIL, e.to_string()))
    })
}

fn encode(image: &Image, path: &Path, format: SaveFormat) -> Result<()> {
    unsafe {
        let wic = factory()?;
        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(PCWSTR(wide(path).as_ptr()), GENERIC_WRITE.0)?;
        encode_stream(image, &wic, &stream, format)
    }
}
fn encode_stream(
    image: &Image,
    wic: &IWICImagingFactory,
    stream: &IStream,
    format: SaveFormat,
) -> Result<()> {
    unsafe {
        // JPEG stores no alpha: the same pixels are read as opaque.
        let (container, mut stored, memory) = match format {
            SaveFormat::Png => (
                &GUID_ContainerFormatPng,
                GUID_WICPixelFormat32bppBGRA,
                &GUID_WICPixelFormat32bppBGRA,
            ),
            SaveFormat::Jpeg => (
                &GUID_ContainerFormatJpeg,
                GUID_WICPixelFormat24bppBGR,
                &GUID_WICPixelFormat32bppBGR,
            ),
        };
        let encoder = wic.CreateEncoder(container, std::ptr::null())?;
        encoder.Initialize(stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut options = None;
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(|| Error::new(E_FAIL, "Image frame missing"))?;
        if format == SaveFormat::Jpeg {
            let options = options
                .as_ref()
                .ok_or_else(|| Error::new(E_FAIL, "JPEG options missing"))?;
            best_jpeg(options)?;
        }
        frame.Initialize(options.as_ref())?;
        frame.SetSize(image.width, image.height)?;
        frame.SetPixelFormat(&mut stored)?;
        let source = wic.CreateBitmapFromMemory(
            image.width,
            image.height,
            memory,
            image.width * 4,
            &image.pixels,
        )?;
        frame.WriteSource(&source, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()
    }
}
/// The best the JPEG encoder can do; there is no quality setting. Its defaults (quality
/// 0.9, color stored at half resolution) smear one-pixel annotations.
fn best_jpeg(options: &IPropertyBag2) -> Result<()> {
    let option = |name: PCWSTR| PROPBAG2 {
        pstrName: PWSTR(name.0.cast_mut()),
        ..Default::default()
    };
    let value = |vt, value| VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt,
                Anonymous: value,
                ..Default::default()
            }),
        },
    };
    let names = [
        option(w!("ImageQuality")),
        option(w!("JpegYCrCbSubsampling")),
    ];
    let values = [
        value(VT_R4, VARIANT_0_0_0 { fltVal: 1. }),
        value(
            VT_UI1,
            VARIANT_0_0_0 {
                bVal: WICJpegYCrCbSubsampling444.0 as u8,
            },
        ),
    ];
    unsafe { options.Write(names.len() as u32, names.as_ptr(), values.as_ptr()) }
}
pub fn load_png(path: &Path) -> Result<Image> {
    unsafe {
        let wic = factory()?;
        let decoder = wic.CreateDecoderFromFilename(
            PCWSTR(wide(path).as_ptr()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnLoad,
        )?;
        decoded(&wic, &decoder, None)
    }
}
/// Decodes a PNG of exactly `size` held in memory. Any other container or size is refused
/// before a pixel is allocated: a document read from a file only ever reaches the PNG codec.
pub fn decode_png(bytes: &[u8], size: (u32, u32)) -> Result<Image> {
    unsafe {
        let wic = factory()?;
        let stream = wic.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder =
            wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)?;
        if decoder.GetContainerFormat()? != GUID_ContainerFormatPng {
            return Err(Error::new(E_INVALIDARG, "Not a PNG image"));
        }
        decoded(&wic, &decoder, Some(size))
    }
}
fn decoded(
    wic: &IWICImagingFactory,
    decoder: &IWICBitmapDecoder,
    expected: Option<(u32, u32)>,
) -> Result<Image> {
    unsafe {
        let frame = decoder.GetFrame(0)?;
        let mut width = 0;
        let mut height = 0;
        frame.GetSize(&mut width, &mut height)?;
        if expected.is_some_and(|size| size != (width, height)) {
            return Err(Error::new(E_INVALIDARG, "Unexpected image size"));
        }
        let converter = wic.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        let mut image = Image::solid(width, height, [0, 0, 0, 255]);
        converter.CopyPixels(std::ptr::null(), width * 4, &mut image.pixels)?;
        Ok(image)
    }
}
pub fn copy_image(image: &Image, owner: HWND) -> Result<()> {
    image.validate()?;
    unsafe {
        // PITFALL: CF_DIB owns the HGLOBAL only after successful SetClipboardData.
        let header = image.info().bmiHeader;
        let header_size = std::mem::size_of_val(&header);
        let allocation = GlobalAlloc(GMEM_MOVEABLE, header_size + image.pixels.len())?;
        let data = GlobalLock(allocation).cast::<u8>();
        if data.is_null() {
            let _ = GlobalFree(Some(allocation));
            return Err(Error::from_thread());
        }
        std::ptr::copy_nonoverlapping((&header as *const _ as *const u8).cast(), data, header_size);
        std::ptr::copy_nonoverlapping(
            image.pixels.as_ptr(),
            data.add(header_size),
            image.pixels.len(),
        );
        let _ = GlobalUnlock(allocation);
        if let Err(error) = OpenClipboard(Some(owner)) {
            let _ = GlobalFree(Some(allocation));
            return Err(error);
        }
        let result = EmptyClipboard()
            .and_then(|()| SetClipboardData(8, Some(HANDLE(allocation.0))).map(|_| ()));
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(allocation));
        }
        result
    }
}

/// Copies UTF-16 text, transferring the allocation to Windows only on success.
pub fn copy_text(owner: HWND, text: &str) -> Result<()> {
    if text.contains('\0') {
        return Err(Error::new(
            E_INVALIDARG,
            "Clipboard text contains a null character",
        ));
    }
    let bytes: Vec<u8> = text
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_ne_bytes)
        .collect();
    copy_clipboard_bytes(owner, 13, &bytes).map(|_| ())
}

pub fn copy_annotation_marker(owner: HWND) -> Result<u32> {
    copy_clipboard_bytes(owner, annotation_format()?, &[1])
}

pub fn annotation_clipboard_matches(sequence: u32) -> bool {
    unsafe {
        sequence != 0
            && GetClipboardSequenceNumber() == sequence
            && annotation_format().is_ok_and(|format| IsClipboardFormatAvailable(format).is_ok())
    }
}

fn annotation_format() -> Result<u32> {
    let format = unsafe { RegisterClipboardFormatW(w!("Shotoshot.Annotations.Session")) };
    if format == 0 {
        Err(Error::from_thread())
    } else {
        Ok(format)
    }
}

fn copy_clipboard_bytes(owner: HWND, format: u32, bytes: &[u8]) -> Result<u32> {
    unsafe {
        // PITFALL: SetClipboardData owns the HGLOBAL only after success.
        let allocation = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let data = GlobalLock(allocation).cast::<u8>();
        if data.is_null() {
            let error = Error::from_thread();
            let _ = GlobalFree(Some(allocation));
            return Err(error);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        let _ = GlobalUnlock(allocation);
        if let Err(error) = OpenClipboard(Some(owner)) {
            let _ = GlobalFree(Some(allocation));
            return Err(error);
        }
        let result = EmptyClipboard().and_then(|()| {
            SetClipboardData(format, Some(HANDLE(allocation.0)))
                .map(|_| GetClipboardSequenceNumber())
        });
        let _ = CloseClipboard();
        if result.is_err() {
            let _ = GlobalFree(Some(allocation));
        }
        result
    }
}

/// The PNG encoding of `image`, in memory.
pub(crate) fn png_data(image: &Image) -> Result<Vec<u8>> {
    image.validate()?;
    unsafe {
        let stream = windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal(
            HGLOBAL::default(),
            true,
        )?;
        encode_stream(image, &factory()?, &stream, SaveFormat::Png)?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME)?;
        let mut data = vec![0u8; stat.cbSize as usize];
        stream.Seek(0, STREAM_SEEK_SET, None)?;
        let mut read = 0;
        stream
            .Read(data.as_mut_ptr().cast(), data.len() as u32, Some(&mut read))
            .ok()?;
        if read as usize != data.len() {
            return Err(Error::new(E_FAIL, "Incomplete PNG"));
        }
        Ok(data)
    }
}
pub(crate) fn png_bytes(image: &Image) -> Result<Vec<u8>> {
    let data = png_data(image)?;
    if data.len() > 20 * 1024 * 1024 {
        return Err(Error::new(
            E_FAIL,
            "The image exceeds the Google Lens limit of 20 MB.",
        ));
    }
    Ok(data)
}
