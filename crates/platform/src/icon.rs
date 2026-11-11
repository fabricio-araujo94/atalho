//! Cached executable and shortcut icon extraction.

use core::Image;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[cfg(windows)]
const ICON_SIZE: usize = 16;
const ICON_CACHE_CAPACITY: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct IconCache {
    images: DashMap<CacheKey, Image>,
    lru: Mutex<VecDeque<CacheKey>>,
}

static ICON_CACHE: OnceLock<IconCache> = OnceLock::new();

fn cache() -> &'static IconCache {
    ICON_CACHE.get_or_init(IconCache::default)
}

/// Returns the cached or extracted icon for a file path.
///
/// Native extraction runs on Tokio's blocking thread pool. Missing files and
/// extraction failures return a generic application icon.
///
/// # Errors
/// Returns an error if the blocking task cannot be joined.
pub async fn icon_for(path: &Path) -> core::Result<Image> {
    let modified = tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok());
    let key = CacheKey {
        path: path.to_path_buf(),
        modified,
    };
    if let Some(image) = cache().images.get(&key) {
        touch_lru(&key);
        return Ok(image.clone());
    }

    let path_buf = path.to_path_buf();
    let task_result = tokio::task::spawn_blocking(move || extract_icon(&path_buf))
        .await
        .map_err(|error| {
            core::AppError::platform_failed(format!("icon extraction task failed: {error}"))
        })?;
    #[cfg(windows)]
    let image = task_result.unwrap_or_else(|_| Image::generic());
    #[cfg(not(windows))]
    let image = task_result;
    insert_cache(key, image.clone());
    Ok(image)
}

fn touch_lru(key: &CacheKey) {
    if let Ok(mut lru) = cache().lru.lock() {
        if let Some(position) = lru.iter().position(|candidate| candidate == key) {
            let _ = lru.remove(position);
        }
        lru.push_back(key.clone());
    }
}

fn insert_cache(key: CacheKey, image: Image) {
    let Ok(mut lru) = cache().lru.lock() else {
        return;
    };
    if cache().images.insert(key.clone(), image).is_none() {
        lru.push_back(key);
    } else if let Some(position) = lru.iter().position(|candidate| candidate == &key) {
        let _ = lru.remove(position);
        lru.push_back(key);
    }
    while lru.len() > ICON_CACHE_CAPACITY {
        if let Some(expired) = lru.pop_front() {
            cache().images.remove(&expired);
        }
    }
}

#[cfg(windows)]
fn extract_icon(path: &Path) -> core::Result<Image> {
    windows_icon::extract(path).map_err(|error| {
        core::AppError::platform_failed(format!(
            "failed to extract icon from {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(windows))]
fn extract_icon(_path: &Path) -> Image {
    Image::generic()
}

#[cfg(windows)]
mod windows_icon {
    use super::ICON_SIZE;
    use core::Image;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, GetDIBits, HBITMAP, HDC, ReleaseDC,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGetFileInfoW};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    use windows::core::PCWSTR;

    struct OwnedIcon(HICON);

    impl Drop for OwnedIcon {
        fn drop(&mut self) {
            // SAFETY: This wrapper uniquely owns the icon returned by SHGetFileInfoW.
            unsafe {
                let _ = DestroyIcon(self.0);
            }
        }
    }

    struct OwnedBitmap(HBITMAP);

    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            // SAFETY: GetIconInfo creates caller-owned bitmap handles.
            unsafe {
                let _ = DeleteObject(self.0);
            }
        }
    }

    struct ScreenDc(HDC);

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            // SAFETY: Releases the screen DC acquired with GetDC(None).
            unsafe {
                let _ = ReleaseDC(HWND::default(), self.0);
            }
        }
    }

    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            // SAFETY: Deletes the compatible DC created for icon rasterization.
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    pub fn extract(path: &Path) -> windows::core::Result<Image> {
        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut file_info = SHFILEINFOW::default();
        // SAFETY: path_wide is NUL-terminated and file_info points to writable storage.
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(path_wide.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES::default(),
                Some(&raw mut file_info),
                u32::try_from(size_of::<SHFILEINFOW>()).unwrap_or(u32::MAX),
                SHGFI_ICON | SHGFI_SMALLICON,
            )
        };
        if result == 0 || file_info.hIcon.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }
        let icon = OwnedIcon(file_info.hIcon);
        let mut icon_info = ICONINFO::default();
        // SAFETY: icon_info is initialized writable storage; icon owns a valid HICON.
        unsafe { GetIconInfo(icon.0, &raw mut icon_info)? };
        let color_bitmap = OwnedBitmap(icon_info.hbmColor);
        let _mask_bitmap = OwnedBitmap(icon_info.hbmMask);

        let screen_dc = ScreenDc(unsafe { GetDC(HWND::default()) });
        if screen_dc.0.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }
        let memory_dc = MemoryDc(unsafe { CreateCompatibleDC(screen_dc.0) });
        if memory_dc.0.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(u32::MAX),
                biWidth: i32::try_from(ICON_SIZE).unwrap_or(16),
                biHeight: -i32::try_from(ICON_SIZE).unwrap_or(16),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: u32::try_from(ICON_SIZE * ICON_SIZE * 4).unwrap_or(u32::MAX),
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bgra = vec![0_u8; ICON_SIZE * ICON_SIZE * 4];
        // SAFETY: The DIB buffer is large enough for a 16x16 32-bit bitmap.
        let copied = unsafe {
            GetDIBits(
                memory_dc.0,
                color_bitmap.0,
                0,
                u32::try_from(ICON_SIZE).unwrap_or(16),
                Some(bgra.as_mut_ptr().cast()),
                &raw mut bitmap_info,
                DIB_RGB_COLORS,
            )
        };
        if copied == 0 {
            return Err(windows::core::Error::from_win32());
        }
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            if pixel[3] == 0 {
                pixel[3] = 255;
            }
        }
        Ok(Image {
            width: u32::try_from(ICON_SIZE).unwrap_or(16),
            height: u32::try_from(ICON_SIZE).unwrap_or(16),
            rgba: bgra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_icon_uses_generic_fallback() {
        let image = icon_for(Path::new("nonexistent-application-icon.exe"))
            .await
            .expect("icon lookup should return a fallback");

        assert_eq!(image.width, 16);
        assert_eq!(image.height, 16);
        assert_eq!(image.rgba.len(), 16 * 16 * 4);
    }
}
