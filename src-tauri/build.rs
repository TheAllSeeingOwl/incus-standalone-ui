use std::process::Command;

fn main() {
    // Incus version from internal/version/flex.go (e.g. "6.21")
    let incus_version = read_incus_version()
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=INCUS_VERSION={}", incus_version);

    // Last commit that touched the doc/ folder (written by build-docs.sh or flake)
    let incus_commit = read_commit_file("../.docs-commit")
        .unwrap_or_else(|| git_short_commit_path("../incus-src", "doc/"));
    println!("cargo:rustc-env=INCUS_COMMIT={}", incus_commit);
    println!("cargo:rerun-if-changed=../.docs-commit");

    // Last commit of incus-ui-canonical (written by flake, or read from git)
    let ui_commit = read_commit_file("../.ui-commit")
        .unwrap_or_else(|| git_short_commit("../incus-ui-canonical"));
    println!("cargo:rustc-env=INCUS_UI_COMMIT={}", ui_commit);
    println!("cargo:rerun-if-changed=../.ui-commit");

    // Re-run if either submodule HEAD changes
    println!("cargo:rerun-if-changed=../incus-src/internal/version/flex.go");
    println!("cargo:rerun-if-changed=../incus-src/.git/HEAD");
    println!("cargo:rerun-if-changed=../incus-ui-canonical/.git/HEAD");

    tauri_build::build()
}

fn read_commit_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_incus_version() -> Option<String> {
    let src = std::fs::read_to_string("../incus-src/internal/version/flex.go").ok()?;
    for line in src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("var Version = \"") {
            return Some(rest.trim_end_matches('"').to_string());
        }
    }
    None
}

fn git_short_commit(dir: &str) -> String {
    git_short_commit_path(dir, "")
}

fn git_short_commit_path(dir: &str, path: &str) -> String {
    let mut args = vec!["-C", dir, "log", "-1", "--format=%h %ad", "--date=short"];
    if !path.is_empty() {
        args.extend_from_slice(&["--", path]);
    }
    Command::new("git")
        .args(&args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
