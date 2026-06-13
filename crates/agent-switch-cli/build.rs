use std::{
    env, fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

fn main() {
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    println!("cargo:rerun-if-env-changed=BUILD_DATE");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    watch_git_head();

    if let Ok(target) = env::var("TARGET") {
        println!("cargo:rustc-env=TARGET={target}");
    }

    if let Some(sha) = env::var("GIT_SHA")
        .ok()
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .or_else(|| git_output(["rev-parse", "--short", "HEAD"]))
    {
        println!("cargo:rustc-env=GIT_SHA={sha}");
    }

    if let Some(rustc_version) = rustc_version() {
        println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");
    }
    if let Some(lock_hash) = lock_hash() {
        println!("cargo:rustc-env=CARGO_LOCK_SHA256={lock_hash}");
    }

    let build_date = env::var("BUILD_DATE")
        .ok()
        .map(|date| date.trim().to_string())
        .filter(|date| !date.is_empty())
        .or_else(|| {
            env::var("SOURCE_DATE_EPOCH")
                .ok()
                .and_then(|epoch| iso8601_from_epoch(&epoch))
        })
        .or_else(current_build_date)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_DATE={build_date}");
}

fn rustc_version() -> Option<String> {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(rustc).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn lock_hash() -> Option<String> {
    let bytes = fs::read("../../Cargo.lock").ok()?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Some(format!("{:x}", hasher.finalize()))
}

fn watch_git_head() {
    let head_path = Path::new("../../.git/HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    let Ok(head) = fs::read_to_string(head_path) else {
        return;
    };
    let Some(reference) = head.strip_prefix("ref: ").map(str::trim) else {
        return;
    };
    if reference.is_empty() {
        return;
    }

    println!("cargo:rerun-if-changed=../../.git/{reference}");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn current_build_date() -> Option<String> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(format_epoch(seconds))
}

fn iso8601_from_epoch(epoch: &str) -> Option<String> {
    let seconds = epoch.trim().parse::<u64>().ok()?;
    Some(format_epoch(seconds))
}

fn format_epoch(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Converts days since 1970-01-01 to a Gregorian date.
// Algorithm by Howard Hinnant, public domain.
fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);

    (year as i32, month as u32, day as u32)
}
