//! Filters bun/bunx output and auto-injects the "run" subcommand when appropriate.

use crate::core::runner;
use crate::core::utils::resolved_command;
use anyhow::Result;
use std::ffi::OsString;

/// Known bun subcommands that should NOT get "run" injected.
const BUN_SUBCOMMANDS: &[&str] = &[
    "install",
    "i",
    "add",
    "a",
    "remove",
    "rm",
    "update",
    "up",
    "upgrade",
    "link",
    "unlink",
    "outdated",
    "audit",
    "init",
    "create",
    "publish",
    "pack",
    "build",
    "run",
    "test",
    "x",
    "exec",
    "repl",
    "pm",
    "bunx",
    "help",
    "--help",
    "--version",
    "-v",
];

pub fn run(args: &[String], verbose: u8, skip_env: bool) -> Result<i32> {
    let mut cmd = resolved_command("bun");

    let first_arg = args.first().map(|s| s.as_str());
    let is_run_explicit = first_arg == Some("run");
    let is_bun_subcommand = first_arg
        .map(|a| BUN_SUBCOMMANDS.contains(&a) || a.starts_with('-'))
        .unwrap_or(false);

    let effective_args = if is_run_explicit {
        // "rtk bun run build" → "bun run build"
        cmd.arg("run");
        &args[1..]
    } else if is_bun_subcommand {
        // "rtk bun install" → "bun install"
        args
    } else {
        // "rtk bun build" → "bun run build" (assume script name)
        cmd.arg("run");
        args
    };

    for arg in effective_args {
        cmd.arg(arg);
    }

    if skip_env {
        cmd.env("SKIP_ENV_VALIDATION", "1");
    }

    if verbose > 0 {
        eprintln!("Running: bun {}", args.join(" "));
    }

    runner::run_filtered(
        cmd,
        "bun",
        &args.join(" "),
        |raw| filter_bun_output(raw),
        runner::RunOptions::default(),
    )
}

/// Filter bun output — strip spinners, progress, install boilerplate; keep summary
fn filter_bun_output(output: &str) -> String {
    let mut result = Vec::new();

    for line in output.lines() {
        // Skip spinner/progress lines (bun uses Unicode spinners)
        if line.starts_with('\r') {
            continue;
        }
        // Skip blank lines
        if line.trim().is_empty() {
            continue;
        }
        // Skip bun install boilerplate header
        if line.contains("bun install") && line.contains("bun v") {
            continue;
        }
        // Skip "Resolving dependencies" / "Resolved, downloaded" progress lines
        if line.trim_start().starts_with("Resolving")
            || line.trim_start().starts_with("Resolved")
            || line.trim_start().starts_with("Downloading")
        {
            continue;
        }
        // Skip "$ script-name" echo lines that bun emits before running a script
        if line.trim_start().starts_with("$ ") {
            continue;
        }
        result.push(line.to_string());
    }

    if result.is_empty() {
        "ok".to_string()
    } else {
        result.join("\n")
    }
}

pub fn run_passthrough(args: &[OsString], verbose: u8) -> Result<i32> {
    crate::core::runner::run_passthrough("bun", args, verbose)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_bun_output_strips_boilerplate() {
        let output = r#"$ next build
bun install v1.1.0

Resolving dependencies
Resolved, downloaded 0, no changes

Creating an optimized production build...
✓ Build completed
"#;
        let result = filter_bun_output(output);
        assert!(!result.contains("$ next build"));
        assert!(!result.contains("Resolving"));
        assert!(!result.contains("Resolved"));
        assert!(result.contains("Build completed"));
    }

    #[test]
    fn test_filter_bun_output_empty() {
        let output = "\n\n\n";
        let result = filter_bun_output(output);
        assert_eq!(result, "ok");
    }

    #[test]
    fn test_bun_subcommand_routing() {
        fn needs_run_injection(args: &[&str]) -> bool {
            let first = args.first().copied();
            let is_run_explicit = first == Some("run");
            let is_subcommand = first
                .map(|a| BUN_SUBCOMMANDS.contains(&a) || a.starts_with('-'))
                .unwrap_or(false);
            !is_run_explicit && !is_subcommand
        }

        // Known subcommands should NOT get "run" injected
        for subcmd in BUN_SUBCOMMANDS {
            assert!(
                !needs_run_injection(&[subcmd]),
                "'bun {}' should NOT inject 'run'",
                subcmd
            );
        }

        // Script names SHOULD get "run" injected
        for script in &["dev", "lint", "typecheck", "deploy", "test:unit"] {
            assert!(
                needs_run_injection(&[script]),
                "'bun {}' SHOULD inject 'run'",
                script
            );
        }

        // Flags should NOT get "run" injected
        assert!(!needs_run_injection(&["--version"]));

        // Explicit "run" should NOT inject another "run"
        assert!(!needs_run_injection(&["run", "dev"]));
    }
}
