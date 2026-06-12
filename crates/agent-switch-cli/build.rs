use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    if let Ok(target) = env::var("TARGET") {
        println!("cargo:rustc-env=TARGET={target}");
    }

    if env::var_os("GIT_SHA").is_none()
        && let Ok(output) = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
        && output.status.success()
    {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            println!("cargo:rustc-env=GIT_SHA={sha}");
        }
    }
}
