//! ROLE: "Check for updates" on Linux: the published version read by `curl` on the menu's click
//! only, and the download page opened in the browser.
//! OWNER: The resident runs `check` on a thread of its own and shows the outcome.
//! INVARIANTS: Nothing reaches the network before the click (law 4). The app never downloads nor
//! runs anything it fetched (Mickael, 2026-11-13, A1). No HTTP library in the binary: `curl`,
//! present on nearly every distribution; without it, the click opens the download page instead.
use shotoshot_core::update::{self, DOWNLOAD_PAGE, LATEST_URL, MAX_ANSWER, Outcome, Version};
use std::process::{Command, Stdio};

/// What a click found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    Found(Outcome),
    /// The site did not answer, or not with a version.
    Failed,
    /// No `curl` here: the download page is the answer.
    NoCurl,
}

/// The `curl` arguments: HTTPS only, no redirect, a bounded wait and answer, errors for a status
/// other than 2xx.
pub fn curl_arguments() -> Vec<String> {
    [
        "--silent",
        "--show-error",
        "--fail",
        "--proto",
        "=https",
        "--max-redirs",
        "0",
        "--max-time",
        "10",
        "--max-filesize",
        &MAX_ANSWER.to_string(),
        LATEST_URL,
    ]
    .map(String::from)
    .to_vec()
}

/// Reads what `curl` gave: its exit, then its answer.
pub fn read(running: Version, success: bool, body: &[u8]) -> Checked {
    if !success || body.len() > MAX_ANSWER {
        return Checked::Failed;
    }
    std::str::from_utf8(body)
        .ok()
        .and_then(|body| update::check(running, body))
        .map_or(Checked::Failed, Checked::Found)
}

/// The one request of the click; blocks up to curl's own limit.
pub fn check(running: Version) -> Checked {
    let output = Command::new("curl")
        .args(curl_arguments())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) => read(running, output.status.success(), &output.stdout),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Checked::NoCurl,
        Err(_) => Checked::Failed,
    }
}

/// Opens the download page in the default browser.
pub fn open_download_page() -> std::io::Result<()> {
    let mut child = Command::new("xdg-open")
        .arg(DOWNLOAD_PAGE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Version {
        Version::parse("0.3.45").unwrap()
    }

    #[test]
    fn curl_asks_the_site_over_https_only_and_follows_nothing() {
        let args = curl_arguments();
        assert_eq!(args.last().map(String::as_str), Some(LATEST_URL));
        for pair in [["--proto", "=https"], ["--max-redirs", "0"]] {
            assert!(args.windows(2).any(|w| w == pair), "{pair:?}");
        }
        assert!(args.iter().any(|a| a == "--fail"));
        assert!(args.iter().any(|a| a == "--max-time"));
    }

    #[test]
    fn curl_answers_are_read_as_the_windows_ones() {
        let newer = Version::parse("0.3.46").unwrap();
        assert_eq!(
            read(running(), true, b"{\"version\":\"0.3.46\"}"),
            Checked::Found(Outcome::Available(newer))
        );
        assert_eq!(
            read(running(), true, b"{\"version\":\"0.3.45\"}"),
            Checked::Found(Outcome::UpToDate)
        );
        // curl failed (no network, HTTP error): never "up to date".
        assert_eq!(
            read(running(), false, b"{\"version\":\"0.3.45\"}"),
            Checked::Failed
        );
        assert_eq!(read(running(), true, b"<html>"), Checked::Failed);
        assert_eq!(read(running(), true, &[0xff, 0xfe]), Checked::Failed);
    }
}
