use crate::{Image, ImageExt};
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Imaging::*,
        Storage::FileSystem::*,
        System::{Com::*, DataExchange::*, Memory::*},
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
    image.validate()?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let outcome = encode(image, &temporary).and_then(|()| unsafe {
        MoveFileExW(
            PCWSTR(wide(&temporary).as_ptr()),
            PCWSTR(wide(path).as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    });
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    outcome
}
// Never replace a numbered capture, even if another editor saved after the dialog opened.
pub fn save_numbered_png(image: &Image, requested: &Path) -> Result<std::path::PathBuf> {
    image.validate()?;
    let temporary = requested.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        encode(image, &temporary)?;
        let mut destination = requested.to_path_buf();
        loop {
            match unsafe {
                MoveFileExW(
                    PCWSTR(wide(&temporary).as_ptr()),
                    PCWSTR(wide(&destination).as_ptr()),
                    MOVEFILE_WRITE_THROUGH,
                )
            } {
                Ok(()) => return Ok(destination),
                Err(error)
                    if error.code() == ERROR_ALREADY_EXISTS.to_hresult()
                        || error.code() == ERROR_FILE_EXISTS.to_hresult() =>
                {
                    destination = crate::capture_name::next_path(
                        requested.parent().unwrap_or(Path::new(".")),
                    )
                    .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
                }
                Err(error) => return Err(error),
            }
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn encode(image: &Image, path: &Path) -> Result<()> {
    unsafe {
        let wic = factory()?;
        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(PCWSTR(wide(path).as_ptr()), GENERIC_WRITE.0)?;
        encode_stream(image, &wic, &stream)
    }
}
fn encode_stream(image: &Image, wic: &IWICImagingFactory, stream: &IStream) -> Result<()> {
    unsafe {
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(|| Error::new(E_FAIL, "PNG frame missing"))?;
        frame.Initialize(None)?;
        frame.SetSize(image.width, image.height)?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut format)?;
        let source = wic.CreateBitmapFromMemory(
            image.width,
            image.height,
            &GUID_WICPixelFormat32bppBGRA,
            image.width * 4,
            &image.pixels,
        )?;
        frame.WriteSource(&source, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()
    }
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
        let frame = decoder.GetFrame(0)?;
        let mut width = 0;
        let mut height = 0;
        frame.GetSize(&mut width, &mut height)?;
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

pub(crate) fn png_bytes(image: &Image) -> Result<Vec<u8>> {
    image.validate()?;
    unsafe {
        let stream = windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal(
            HGLOBAL::default(),
            true,
        )?;
        encode_stream(image, &factory()?, &stream)?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME)?;
        if stat.cbSize > 20 * 1024 * 1024 {
            return Err(Error::new(
                E_FAIL,
                "The image exceeds the Google Lens limit of 20 MB.",
            ));
        }
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
