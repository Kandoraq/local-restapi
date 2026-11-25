use std::process::Command;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";

pub fn get(url: &str, max_size: &str, timeout: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["-s", "-L", "--max-time", timeout, "--max-filesize", max_size, "-A", UA, url])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("curl failed".into());
    }
    if output.stdout.is_empty() {
        return Err("empty response".into());
    }
    Ok(output.stdout)
}

pub fn get_text(url: &str, max_size: &str, timeout: &str) -> Result<String, String> {
    let bytes = get(url, max_size, timeout)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
