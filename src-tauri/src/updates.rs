//! `open_release_page` opens a version's GitHub release page (the What's new
//! notice's link) in the default browser. The update itself is
//! src/selfupdate.rs.

/// Where freewheel's releases live.
pub const RELEASES: &str = "https://github.com/brinkercode/freewheel/releases";

/// The GitHub release page of `version` ("0.1.1" or "v0.1.1"), or None when
/// it is not a plain semver version (so no other URL can be opened).
pub fn release_page(version: &str) -> Option<String> {
    let v = version.strip_prefix('v').unwrap_or(version);
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let numeric = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let pre_ok = pre.is_none_or(|p| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    });
    (numeric && pre_ok).then(|| format!("{RELEASES}/tag/v{v}"))
}

/// Opens `version`'s GitHub release page in the default browser (no shell).
#[tauri::command]
pub fn open_release_page(version: String) -> Result<(), String> {
    let url = release_page(&version).ok_or_else(|| format!("not a version: {version}"))?;
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("/usr/bin/open", vec![&url])
    } else if cfg!(windows) {
        ("rundll32.exe", vec!["url.dll,FileProtocolHandler", &url])
    } else {
        ("xdg-open", vec![&url])
    };
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{program} {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_release_page_is_the_tag_page() {
        let page = "https://github.com/brinkercode/freewheel/releases/tag/v0.1.1";
        assert_eq!(release_page("0.1.1").as_deref(), Some(page));
        assert_eq!(release_page("v0.1.1").as_deref(), Some(page));
        assert_eq!(
            release_page("1.2.3-beta.1").as_deref(),
            Some("https://github.com/brinkercode/freewheel/releases/tag/v1.2.3-beta.1")
        );
    }

    #[test]
    fn only_versions_make_a_release_page() {
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1.x.3",
            "1.2.3-",
            "1.2.3/../../evil",
            "1.2.3-a b",
            "https://evil.example",
            "1.2.3?x=1",
        ] {
            assert_eq!(release_page(bad), None, "{bad}");
        }
    }
}
