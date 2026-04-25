use std::io::{self, Write};
use std::path::{Path, PathBuf};

const REPO: &str = "nqh98/cortex";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
}

fn platform_tag() -> Result<&'static str, String> {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match (arch, os) {
        ("x86_64", "linux") => Ok("linux-x86_64"),
        ("x86_64", "macos") => Ok("macos-x86_64"),
        ("aarch64", "macos") => Ok("macos-aarch64"),
        _ => Err(format!("Unsupported platform: {arch}-{os}")),
    }
}

async fn fetch_latest_version(client: &reqwest::Client) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", "cortex-update")
        .send()
        .await
        .map_err(|e| format!("Failed to contact GitHub: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let release: Release = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse release: {e}"))?;

    Ok(release.tag_name.trim_start_matches('v').to_string())
}

async fn download_binary(
    client: &reqwest::Client,
    tag: &str,
    platform: &str,
) -> Result<PathBuf, String> {
    let url =
        format!("https://github.com/{REPO}/releases/download/v{tag}/cortex-{platform}.tar.gz");

    println!("Downloading cortex v{tag}...");
    let resp = client
        .get(&url)
        .header("User-Agent", "cortex-update")
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download returned {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    // Extract cortex binary from tar.gz
    let mut archive = flate2::read::GzDecoder::new(&bytes[..]);
    let mut tar = tar::Archive::new(&mut archive);

    let tmp_dir = std::env::temp_dir().join("cortex-update");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create temp dir: {e}"))?;

    tar.unpack(&tmp_dir)
        .map_err(|e| format!("Failed to extract archive: {e}"))?;

    // Find the cortex binary
    let binary_path = walk_for_binary(&tmp_dir)
        .ok_or_else(|| "Could not find cortex binary in archive".to_string())?;

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    Ok(binary_path)
}

fn walk_for_binary(dir: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walk_for_binary(&path) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|f| f == "cortex") {
            return Some(path);
        }
    }
    None
}

pub async fn perform_update() -> Result<(), String> {
    let client = reqwest::Client::new();
    let latest = fetch_latest_version(&client).await?;

    if latest == CURRENT_VERSION {
        println!("Already up to date (v{CURRENT_VERSION})");
        return Ok(());
    }

    print!("Current: v{CURRENT_VERSION} → Latest: v{latest}. Update? [Y/n] ");
    io::stdout().flush().map_err(|e| format!("IO error: {e}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("IO error: {e}"))?;

    let answer = input.trim().to_lowercase();
    if answer == "n" || answer == "no" {
        println!("Update cancelled.");
        return Ok(());
    }

    let platform = platform_tag()?;
    let new_binary = download_binary(&client, &latest, platform).await?;

    let current_exe =
        std::env::current_exe().map_err(|e| format!("Cannot determine current exe: {e}"))?;

    // Replace: write to a temp file next to current exe, then rename (atomic on same filesystem)
    let staging = current_exe.with_extension("new");
    std::fs::copy(&new_binary, &staging).map_err(|e| format!("Failed to stage binary: {e}"))?;

    let old = current_exe.with_extension("old");
    // Remove any previous old binary
    let _ = std::fs::remove_file(&old);

    std::fs::rename(&current_exe, &old).map_err(|e| format!("Failed to rename old binary: {e}"))?;
    std::fs::rename(&staging, &current_exe).map_err(|e| {
        // Try to rollback
        let _ = std::fs::rename(&old, &current_exe);
        format!("Failed to install new binary: {e}")
    })?;

    let _ = std::fs::remove_file(&old);

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("cortex-update"));

    println!("Updated v{CURRENT_VERSION} → v{latest}");
    Ok(())
}
