use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::Deserialize;
use std::{ffi::c_void, ptr};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::{
        Foundation::HWND,
        Networking::WinHttp::*,
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    },
};

pub const UPDATER_URL: &str = "https://btu.bose.com/";
const DISCOVERY_URL: &str = "https://services.api.bose.io/discovery/discover/product/";

#[derive(Clone, Debug)]
pub struct FirmwareCheck {
    pub update_available: bool,
    pub version: String,
}

#[derive(Deserialize)]
struct Discovery {
    services: std::collections::HashMap<String, Service>,
}

#[derive(Deserialize)]
struct Service {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Availability {
    update_available: bool,
    version_info: String,
    base_url: String,
}

pub fn check(product_id: u16, guid: &str, current_version: &str) -> Result<FirmwareCheck> {
    let product_type = match product_id {
        0x4082 => "wolverine",
        0x4066 => "lonestarr",
        _ => bail!("Online firmware checks are unavailable for this headphone model"),
    };
    ensure!(valid_guid(guid), "The headphone identity is unavailable");
    let current_version = current_version.trim();
    ensure!(
        !current_version.is_empty()
            && current_version.len() <= 128
            && current_version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".+-_".contains(&c)),
        "The headphone firmware version is unavailable"
    );

    let client = HttpClient::new()?;
    let page = client.get(UPDATER_URL, None, 128 * 1024)?;
    let script_path = page
        .split("<script")
        .find(|tag| tag.contains("type=\"module\""))
        .and_then(|tag| literal_after(tag, "src="))
        .context("Bose Updater configuration is unavailable")?;
    let script_url = if script_path.starts_with('/') && !script_path.starts_with("//") {
        format!("https://btu.bose.com{script_path}")
    } else {
        script_path.to_owned()
    };
    ensure!(
        https_parts(&script_url)?.0 == "btu.bose.com",
        "Bose Updater returned an unexpected configuration address"
    );
    let script = client.get(&script_url, None, 4 * 1024 * 1024)?;
    let api_key = literal_after(&script, "X_API_KEY=")
        .filter(|key| {
            (16..=256).contains(&key.len())
                && key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        })
        .context("Bose Updater configuration has changed")?;

    let discovery: Discovery =
        serde_json::from_str(&client.get(&format!("{DISCOVERY_URL}{guid}"), None, 256 * 1024)?)
            .context("Bose returned an invalid service response")?;
    let service = discovery
        .services
        .get("svc-naota-update-service")
        .context("The Bose firmware update service is unavailable")?;
    let endpoint = format!(
        "{}/update?productID={}&productType={}&firmwareVersion={}&fields=updateAvailable,baseUrl,versionInfo,deferrable,rollback,metadata",
        service.url.trim_end_matches('/'),
        guid,
        product_type,
        encode_query(current_version)
    );
    let availability: Availability =
        serde_json::from_str(&client.get(&endpoint, Some(api_key), 256 * 1024)?)
            .context("Bose returned an invalid firmware update response")?;
    if availability.update_available {
        ensure!(
            !availability.version_info.trim().is_empty()
                && !availability.base_url.trim().is_empty(),
            "Bose returned incomplete firmware update information"
        );
        https_parts(&availability.base_url)
            .context("Bose returned an unexpected firmware download address")?;
    }
    Ok(FirmwareCheck {
        update_available: availability.update_available,
        version: availability.version_info.trim().to_owned(),
    })
}

pub fn open_official_updater() -> Result<()> {
    let url = HSTRING::from(UPDATER_URL);
    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            &url,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    ensure!(
        result.0 as isize > 32,
        "The default browser could not open Bose Updater (Windows error {})",
        result.0 as isize
    );
    Ok(())
}

fn valid_guid(guid: &str) -> bool {
    guid.len() == 36
        && guid.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
        && guid.bytes().any(|c| c.is_ascii_hexdigit() && c != b'0')
}

fn literal_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let remaining = text.split_once(marker)?.1.trim_start();
    let quote = remaining.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    remaining[1..].split_once(quote).map(|(value, _)| value)
}

fn encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn https_parts(url: &str) -> Result<(&str, &str)> {
    let remaining = url
        .strip_prefix("https://")
        .context("Only secure Bose update addresses are supported")?;
    let split = remaining.find('/').unwrap_or(remaining.len());
    let host = &remaining[..split];
    ensure!(
        host.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
            && (host == "bose.com"
                || host.ends_with(".bose.com")
                || host == "bose.io"
                || host.ends_with(".bose.io")),
        "The Bose update service returned an unexpected address"
    );
    Ok((
        host,
        remaining
            .get(split..)
            .filter(|s| !s.is_empty())
            .unwrap_or("/"),
    ))
}

struct HttpHandle(*mut c_void);

impl HttpHandle {
    fn new(handle: *mut c_void) -> Result<Self> {
        if handle.is_null() {
            Err(anyhow!(std::io::Error::last_os_error()))
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for HttpHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

struct HttpClient(HttpHandle);

impl HttpClient {
    fn new() -> Result<Self> {
        let session = HttpHandle::new(unsafe {
            WinHttpOpen(
                w!("BoseHeadphoneControl/0.1"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        })
        .context("Could not start a secure connection to Bose")?;
        unsafe {
            WinHttpSetTimeouts(session.0, 5_000, 8_000, 8_000, 12_000)?;
        }
        Ok(Self(session))
    }

    fn get(&self, url: &str, api_key: Option<&str>, limit: usize) -> Result<String> {
        let (host, path) = https_parts(url)?;
        let host = HSTRING::from(host);
        let path = HSTRING::from(path);
        let connection = HttpHandle::new(unsafe { WinHttpConnect(self.0 .0, &host, 443, 0) })
            .context("Could not connect to the Bose update service")?;
        let request = HttpHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                &path,
                PCWSTR::null(),
                PCWSTR::null(),
                ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        })
        .context("Could not create the Bose update request")?;
        let mut headers = String::from("Accept: */*\r\nCache-Control: no-cache\r\n");
        if let Some(key) = api_key {
            headers.push_str("x-api-key: ");
            headers.push_str(key);
            headers.push_str("\r\n");
        }
        let headers: Vec<u16> = headers.encode_utf16().collect();
        unsafe {
            WinHttpAddRequestHeaders(request.0, &headers, WINHTTP_ADDREQ_FLAG_ADD)?;
            WinHttpSendRequest(request.0, None, None, 0, 0, 0)
                .context("The Bose update request failed")?;
            WinHttpReceiveResponse(request.0, ptr::null_mut())
                .context("The Bose update service did not respond")?;
            let mut status = 0u32;
            let mut length = std::mem::size_of::<u32>() as u32;
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status as *mut u32).cast()),
                &mut length,
                ptr::null_mut(),
            )?;
            ensure!(
                status == 200,
                "The Bose update service returned HTTP {status}"
            );
        }
        let mut response = Vec::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            unsafe {
                WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                )
                .context("The Bose update response was interrupted")?;
            }
            if read == 0 {
                break;
            }
            ensure!(
                response.len() + read as usize <= limit,
                "The Bose update response exceeds the supported size"
            );
            response.extend_from_slice(&buffer[..read as usize]);
        }
        String::from_utf8(response).context("Bose returned an invalid text response")
    }
}
