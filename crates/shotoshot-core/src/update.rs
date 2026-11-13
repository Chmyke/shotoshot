//! ROLE: The "Check for updates" button, without its network: where the published version is
//! read, how its answer is read, and whether it is newer than the version running.
//! OWNER: The platform fetches `LATEST_URL` on the user's click only, hands the body to
//! `check`, and shows the `Outcome`; a newer version opens `DOWNLOAD_PAGE` in the browser.
//! INVARIANTS: Nothing here touches the network. The app never downloads nor runs an installer
//! itself (Mickael, 2026-11-13, choice A1): the browser does, as before. An answer that is not
//! exactly a version is a failure, never "up to date".

/// The site's answer is `{"version":"X.Y.Z"}`, the version of the files it serves.
pub const LATEST_HOST: &str = "shotoshot.com";
pub const LATEST_PATH: &str = "/api/shotoshot/latest";
pub const LATEST_URL: &str = "https://shotoshot.com/api/shotoshot/latest";
/// Where a newer version is downloaded, by the browser.
pub const DOWNLOAD_PAGE: &str = "https://shotoshot.com/#download";
/// The longest answer read: the site's is about twenty bytes.
pub const MAX_ANSWER: usize = 4096;

/// A release number, `major.minor.patch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Reads `X.Y.Z`, digits only, nothing around it.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let mut number = || -> Option<u32> {
            let part = parts.next()?;
            if part.is_empty() || part.len() > 6 || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Self {
            major: number()?,
            minor: number()?,
            patch: number()?,
        };
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The version running is the one published, or newer (a build not yet published).
    UpToDate,
    /// This version is published and newer than the one running.
    Available(Version),
}

/// Reads the version out of the site's answer, `{"version":"X.Y.Z"}`, spaces allowed.
pub fn published(body: &str) -> Option<Version> {
    let body = body.trim();
    let inner = body.strip_prefix('{')?.strip_suffix('}')?.trim();
    let (key, value) = inner.split_once(':')?;
    if key.trim() != "\"version\"" {
        return None;
    }
    let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    Version::parse(value)
}

/// Compares the site's answer with the version running; None when the answer is not readable.
pub fn check(running: Version, body: &str) -> Option<Outcome> {
    let latest = published(body)?;
    Some(if latest > running {
        Outcome::Available(latest)
    } else {
        Outcome::UpToDate
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: Version = Version {
        major: 0,
        minor: 3,
        patch: 45,
    };

    #[test]
    fn versions_compare_by_number_not_by_text() {
        let v = |text| Version::parse(text).unwrap();
        assert!(v("0.3.46") > v("0.3.45"));
        assert!(v("0.3.100") > v("0.3.99"));
        assert!(v("0.4.0") > v("0.3.99"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert_eq!(v("0.3.45").to_string(), "0.3.45");
    }

    #[test]
    fn only_a_plain_version_is_read() {
        for bad in [
            "",
            "0.3",
            "0.3.45.1",
            "0.3.x",
            "v0.3.45",
            " 0.3.45",
            "0..45",
            "-1.0.0",
            "0.3.45-beta",
            "1234567.0.0",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_site_answer_is_read() {
        assert_eq!(
            published("{\"version\":\"0.3.46\"}"),
            Version::parse("0.3.46")
        );
        assert_eq!(
            published(" { \"version\" : \"0.3.46\" }\n"),
            Version::parse("0.3.46")
        );
    }

    #[test]
    fn anything_else_is_a_failure_not_up_to_date() {
        for bad in [
            "",
            "<html>Service unavailable</html>",
            "{\"error\":\"Introuvable.\"}",
            "{\"version\":0.3.46}",
            "{\"version\":\"0.3.46\",\"x\":1}",
            "0.3.46",
        ] {
            assert_eq!(check(RUNNING, bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_newer_version_is_offered_and_an_older_one_is_not() {
        let answer = |v: &str| format!("{{\"version\":\"{v}\"}}");
        assert_eq!(
            check(RUNNING, &answer("0.3.46")),
            Some(Outcome::Available(Version::parse("0.3.46").unwrap()))
        );
        assert_eq!(check(RUNNING, &answer("0.3.45")), Some(Outcome::UpToDate));
        // A build ahead of the site (being tried before publication) is not told to go back.
        assert_eq!(check(RUNNING, &answer("0.3.44")), Some(Outcome::UpToDate));
    }

    #[test]
    fn the_address_is_the_one_the_site_serves() {
        assert_eq!(LATEST_URL, format!("https://{LATEST_HOST}{LATEST_PATH}"));
        assert!(DOWNLOAD_PAGE.starts_with("https://shotoshot.com/"));
    }

    #[test]
    fn this_build_has_a_readable_version() {
        assert!(Version::parse(env!("CARGO_PKG_VERSION")).is_some());
    }
}
