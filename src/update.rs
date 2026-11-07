use anyhow::{ensure, Context, Result};
use semver::Version;
use serde_json::Value;
use std::time::Duration;

const REPOSITORY: &str = "https://github.com/SShnoodles/iot-toolbox";
const LATEST: &str = "https://api.github.com/repos/SShnoodles/iot-toolbox/releases/latest";

pub enum UpdateState {
    Checking,
    Current,
    Available { version: Version, url: String },
    PackageUnavailable,
    Failed,
}

pub fn check() -> Result<UpdateState> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(0)
        .build()
        .new_agent();
    let response = agent
        .get(LATEST)
        .header(
            "User-Agent",
            concat!("iot-toolbox/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call();
    let mut response = match response {
        Err(ureq::Error::StatusCode(404)) => return Ok(UpdateState::PackageUnavailable),
        result => result?,
    };
    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()?;
    let release = serde_json::from_str(&body)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    release_state(
        &release,
        &current,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

fn release_state(release: &Value, current: &Version, os: &str, arch: &str) -> Result<UpdateState> {
    ensure!(
        release["draft"].as_bool() == Some(false) && release["prerelease"].as_bool() == Some(false),
        "Expected a public stable release"
    );
    let tag = release["tag_name"]
        .as_str()
        .context("Missing release tag")?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))?;
    ensure!(version.pre.is_empty(), "Expected a stable version");
    if version.cmp_precedence(current).is_le() {
        return Ok(UpdateState::Current);
    }

    let suffixes: &[&str] = match (os, arch) {
        ("macos", "aarch64") => &["macos-aarch64.dmg"],
        ("macos", "x86_64") => &["macos-x86_64.dmg"],
        ("windows", "x86_64") => &["windows-x86_64-setup.exe", "windows-x86_64.zip"],
        ("linux", "x86_64") => &["linux-x86_64.AppImage", "linux-x86_64.tar.gz", "amd64.deb"],
        _ => return Ok(UpdateState::PackageUnavailable),
    };
    let assets = release["assets"]
        .as_array()
        .context("Missing release assets")?;
    let filename = suffixes.iter().find_map(|suffix| {
        let filename = if *suffix == "amd64.deb" {
            format!("iot-toolbox_{version}_{suffix}")
        } else {
            format!("iot-toolbox-{version}-{suffix}")
        };
        assets
            .iter()
            .any(|asset| {
                asset["name"].as_str() == Some(&filename)
                    && asset["state"].as_str() == Some("uploaded")
                    && asset["size"].as_u64().is_some_and(|size| size > 0)
            })
            .then_some(filename)
    });
    let Some(filename) = filename else {
        return Ok(UpdateState::PackageUnavailable);
    };
    // Build the URL from the known repository and validated version.
    let url = if os == "macos" {
        format!("{REPOSITORY}/releases/download/{tag}/{filename}")
    } else {
        // Windows and Linux ship multiple formats. Let users keep their installation type.
        format!("{REPOSITORY}/releases/tag/{tag}")
    };
    Ok(UpdateState::Available { version, url })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn offers_only_newer_releases_with_matching_packages() {
        let mut release = json!({
            "draft": false, "prerelease": false, "tag_name": "v1.10.0",
            "assets": [{"name": "iot-toolbox-1.10.0-macos-aarch64.dmg", "state": "uploaded", "size": 1024}]
        });
        let current = Version::parse("1.9.0").unwrap();
        assert!(
            matches!(release_state(&release, &current, "macos", "aarch64").unwrap(),
            UpdateState::Available { url, .. } if url.ends_with("/v1.10.0/iot-toolbox-1.10.0-macos-aarch64.dmg"))
        );
        assert!(matches!(
            release_state(&release, &current, "macos", "x86_64").unwrap(),
            UpdateState::PackageUnavailable
        ));
        let current = Version::parse("1.10.0").unwrap();
        assert!(matches!(
            release_state(&release, &current, "macos", "aarch64").unwrap(),
            UpdateState::Current
        ));
        release["prerelease"] = json!(true);
        assert!(release_state(&release, &current, "macos", "aarch64").is_err());
    }
}
