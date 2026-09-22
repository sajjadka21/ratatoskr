use dm_core::browser::BrowserHandoff;
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::{self, Read, Write},
    path::PathBuf,
    process::Command,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeRequest {
    #[serde(rename = "type")]
    message_type: String,
    url: Option<String>,
    text: Option<String>,
    filename_hint: Option<String>,
    referrer: Option<String>,
    user_agent: Option<String>,
}

#[derive(Debug, Serialize)]
struct NativeResponse {
    accepted: bool,
    error: Option<String>,
}

fn main() -> io::Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    loop {
        let Some(payload) = read_message(&mut stdin)? else {
            return Ok(());
        };
        let response = handle_message(&payload);
        write_message(&mut stdout, &response)?;
    }
}

fn handle_message(payload: &[u8]) -> NativeResponse {
    let request = match serde_json::from_slice::<NativeRequest>(payload) {
        Ok(request) => request,
        Err(_) => return rejected("invalid native message"),
    };
    if request.message_type == "inspect" {
        let Some(application) = application_path() else {
            return rejected("Download Manager executable was not found");
        };
        let links =
            dm_core::linkgrabber::extract_links(request.text.as_deref().unwrap_or_default());
        if links.is_empty() {
            return rejected("no HTTP links found");
        }
        for link in links {
            if Command::new(&application)
                .arg("--browser-handoff")
                .arg(link.url)
                .spawn()
                .is_err()
            {
                return rejected("Download Manager could not be started");
            }
        }
        return NativeResponse {
            accepted: true,
            error: None,
        };
    }
    if request.message_type != "download" {
        return rejected("unsupported native message");
    }
    let handoff = BrowserHandoff {
        url: request.url.unwrap_or_default(),
        filename_hint: request.filename_hint,
        referrer: request.referrer,
        user_agent: request.user_agent,
    };
    let Ok(handoff) = handoff.validate() else {
        return rejected("browser handoff validation failed");
    };
    let Some(application) = application_path() else {
        return rejected("Download Manager executable was not found");
    };
    match Command::new(application)
        .arg("--browser-handoff")
        .arg(handoff.url)
        .spawn()
    {
        Ok(_) => NativeResponse {
            accepted: true,
            error: None,
        },
        Err(_) => rejected("Download Manager could not be started"),
    }
}

fn rejected(error: &str) -> NativeResponse {
    NativeResponse {
        accepted: false,
        error: Some(error.to_owned()),
    }
}

fn application_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("DOWNLOAD_MANAGER_APP_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = env::current_exe().ok()?.parent()?.to_owned();
    ["download-manager.exe", "tauri-app.exe"]
        .into_iter()
        .map(|name| current.join(name))
        .find(|path| path.is_file())
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native message too large",
        ));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_message(writer: &mut impl Write, response: &NativeResponse) -> io::Result<()> {
    let payload = serde_json::to_vec(response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "native response too large"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::{handle_message, read_message, write_message};
    use std::io::Cursor;

    #[test]
    fn invalid_messages_are_rejected_without_echoing_sensitive_payloads() {
        let response = handle_message(br#"{"type":"download","url":"not-a-url"}"#);
        assert!(!response.accepted);
        assert_eq!(
            response.error.as_deref(),
            Some("browser handoff validation failed")
        );
    }

    #[test]
    fn native_messages_use_little_endian_length_prefixes() {
        let payload = br#"{"type":"unknown"}"#;
        let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(payload);
        let decoded = read_message(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(decoded, payload);

        let mut output = Vec::new();
        write_message(
            &mut output,
            &super::NativeResponse {
                accepted: true,
                error: None,
            },
        )
        .unwrap();
        assert_eq!(
            u32::from_le_bytes(output[..4].try_into().unwrap()) as usize,
            output.len() - 4
        );
    }
}
