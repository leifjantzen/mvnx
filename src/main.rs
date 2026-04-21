use anyhow::Result;
use mvnx::{
    extract_xml_failures, filter_stack_trace, parse_module_start, parse_reactor_module,
    parse_test_results,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

struct ModuleInfo {
    name: String,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    status: BuildStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BuildStatus {
    Building,
    Success,
    Failure,
}

struct MvnOutput {
    reactor_order: Vec<String>,
    modules: HashMap<String, ModuleInfo>,
    test_failures: Vec<TestFailure>,
    overall_status: BuildStatus,
    overall_time: Option<f64>,
    tests_run: u32,
    tests_failed: u32,
    tests_errored: u32,
    tests_skipped: u32,
    maven_errors: Vec<String>,
}

struct TestFailure {
    stack_trace: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DadJoke {
    joke: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<u32>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Skip the program name
    let mut mvn_args: Vec<String> = args[1..].to_vec();

    // Check for help flag
    if mvn_args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        std::process::exit(0);
    }

    // Check for --mvnd flag (use Maven Daemon instead of mvn)
    let use_mvnd = mvn_args.iter().any(|arg| arg == "--mvnd");
    mvn_args.retain(|arg| arg != "--mvnd");
    let mvn_cmd = if use_mvnd { "mvnd" } else { "mvn" };

    // Check for mvnhelp flag (show Maven help)
    if mvn_args.iter().any(|arg| arg == "--mvnhelp") {
        let output = Command::new(mvn_cmd).arg("--help").output()?;
        println!("{}", String::from_utf8_lossy(&output.stdout));
        std::process::exit(0);
    }

    // Check for -l/--log-file flag
    let mut log_file_path: Option<String> = None;
    let log_file_index = mvn_args
        .iter()
        .position(|arg| arg == "-l" || arg == "--log-file");

    if let Some(pos) = log_file_index {
        // Get the next argument as the file path
        if pos + 1 < mvn_args.len() {
            log_file_path = Some(mvn_args[pos + 1].clone());
            mvn_args.remove(pos + 1);
        }
        mvn_args.remove(pos);
    }

    // Check for -j flag (dad jokes) and -ji flag (joke interval)
    let mut joke_interval: u64 = 30; // default 30 seconds
    let has_ji = mvn_args.iter().any(|arg| arg == "-ji");

    // Look for -ji option with interval value
    if let Some(pos) = mvn_args.iter().position(|arg| arg == "-ji") {
        if pos + 1 < mvn_args.len() {
            if let Ok(interval) = mvn_args[pos + 1].parse::<u64>() {
                joke_interval = interval;
                mvn_args.remove(pos + 1);
            }
        }
        mvn_args.remove(pos);
    }

    // -ji implies -j
    let show_jokes = mvn_args.iter().any(|arg| arg == "-j") || has_ji;

    // Remove -j flag if present
    mvn_args.retain(|arg| arg != "-j");

    // Check for --clip flag (enable clipboard)
    let enable_clipboard = mvn_args.iter().any(|arg| arg == "--clip");
    mvn_args.retain(|arg| arg != "--clip");

    let mvn_args: Vec<&str> = mvn_args.iter().map(|s| s.as_str()).collect();

    if mvn_args.is_empty() {
        eprintln!("Error: No Maven arguments provided");
        eprintln!();
        eprintln!("Usage: mvnx [-j] [-ji <seconds>] [mvn arguments]");
        eprintln!("Use 'mvnx -h' for more information");
        std::process::exit(1);
    }

    // Spawn Maven process
    let mut child = Command::new(mvn_cmd)
        .args(&mvn_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    // Start spinner animation
    let spinner_running = Arc::new(AtomicBool::new(true));
    let spinner_running_clone = spinner_running.clone();
    let spinner_thread = thread::spawn(move || {
        let frames = ['|', '/', '-', '\\'];
        let mut frame = 0;
        let mut last_joke_time = Instant::now();

        while spinner_running_clone.load(Ordering::Relaxed) {
            // Check if it's time to show a joke
            if show_jokes && last_joke_time.elapsed().as_secs() >= joke_interval {
                eprint!("\r{}\n", " ".repeat(50)); // Clear the current line
                let _ = std::io::stderr().flush();

                // Fetch and display a joke
                match fetch_dad_joke() {
                    Ok(joke) => {
                        eprintln!("{}", joke.joke);
                    }
                    Err(e) => {
                        eprintln!("Error fetching joke: {}", e);
                    }
                }
                eprintln!();
                let _ = std::io::stderr().flush();

                last_joke_time = Instant::now();
                frame = 0; // Reset frame counter
            }

            eprint!("\r{} ", frames[frame]);
            let _ = std::io::stderr().flush();
            frame = (frame + 1) % frames.len();
            thread::sleep(Duration::from_millis(100));
        }
        eprint!("\r"); // Clear the spinner line
        let _ = std::io::stderr().flush();
    });

    let mut output = MvnOutput {
        reactor_order: Vec::new(),
        modules: HashMap::new(),
        test_failures: Vec::new(),
        overall_status: BuildStatus::Building,
        overall_time: None,
        tests_run: 0,
        tests_failed: 0,
        tests_errored: 0,
        tests_skipped: 0,
        maven_errors: Vec::new(),
    };

    let mut current_test_failure: Option<TestFailure> = None;
    let mut in_failure_section = false;
    let mut current_module: Option<String> = None;
    let mut reactor_printed = false;
    let mut printed_modules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let original_reactor_order = output.reactor_order.clone();
    let build_start = Instant::now();

    // Open log file if specified
    let mut log_file = if let Some(ref path) = log_file_path {
        Some(File::create(path)?)
    } else {
        None
    };

    // Parse Maven output
    for line in reader.lines() {
        let line = line?;

        // Write to log file if specified
        if let Some(ref mut file) = log_file {
            writeln!(file, "{}", line)?;
        }

        // Parse reactor build order
        if line.contains("The Reactor build order:") {
            continue;
        }

        // Parse reactor modules
        if let Some(module) = parse_reactor_module(&line) {
            output.reactor_order.push(module.clone());
            output.modules.insert(
                module.clone(),
                ModuleInfo {
                    name: module,
                    start_time: None,
                    end_time: None,
                    status: BuildStatus::Building,
                },
            );
            continue;
        }

        // Parse module build start
        if let Some(module_name) = parse_module_start(&line) {
            // Add to reactor order if not already present (handles single-module builds and submodules)
            let is_new_module = !output.reactor_order.contains(&module_name);
            if is_new_module {
                output.reactor_order.push(module_name.clone());
            }

            // Print reactor build order when we encounter a module not in original reactor order (indicates submodules)
            if !reactor_printed && is_new_module && !original_reactor_order.is_empty() {
                println!("\nReactor Build Order:");
                for (i, module) in output.reactor_order.iter().enumerate() {
                    println!("  {}. {}", i + 1, module);
                }
                println!();
                reactor_printed = true;
            }

            // For single-module builds, print after first Building line
            if !reactor_printed && !is_new_module && output.reactor_order.len() == 1 {
                println!("\nReactor Build Order:");
                for (i, module) in output.reactor_order.iter().enumerate() {
                    println!("  {}. {}", i + 1, module);
                }
                println!();
                reactor_printed = true;
            }

            // Close out previous module's timing
            if let Some(prev_module) = current_module.take() {
                if let Some(info) = output.modules.get_mut(&prev_module) {
                    if info.end_time.is_none() {
                        info.end_time = Some(Instant::now());
                    }
                }
            }

            // Clear spinner line before printing module name
            if !printed_modules.contains(&module_name) {
                eprint!("\r{}\r", " ".repeat(50)); // Clear the spinner line
                let _ = std::io::stderr().flush();
                println!("> Building {}", module_name);
                printed_modules.insert(module_name.clone());
            }

            // Ensure module exists in tracking
            output
                .modules
                .entry(module_name.clone())
                .or_insert_with(|| ModuleInfo {
                    name: module_name.clone(),
                    start_time: None,
                    end_time: None,
                    status: BuildStatus::Building,
                });

            if let Some(info) = output.modules.get_mut(&module_name) {
                info.start_time = Some(Instant::now());
                info.status = BuildStatus::Building;
            }

            current_module = Some(module_name);
            continue;
        }

        // Parse test failure start
        if line.contains("FAILURE") && line.contains("in") {
            in_failure_section = true;
            if let Some(failure) = parse_test_failure_header(&line) {
                current_test_failure = Some(failure);
            }
            continue;
        }

        // Parse test failure stack trace
        if in_failure_section {
            if line.trim().is_empty() {
                in_failure_section = false;
                if let Some(failure) = current_test_failure.take() {
                    output.test_failures.push(failure);
                }
            } else if let Some(ref mut failure) = current_test_failure {
                failure.stack_trace.push(line);
            }
            continue;
        }

        // Parse test results
        if let Some((run, failed, errored, skipped)) = parse_test_results(&line) {
            output.tests_run += run;
            output.tests_failed += failed;
            output.tests_errored += errored;
            output.tests_skipped += skipped;
            continue;
        }

        // Collect Maven error messages (skip duplicates)
        if line.starts_with("[ERROR]") {
            let msg = line.trim_start_matches("[ERROR]").trim().to_string();
            if !msg.is_empty() && !output.maven_errors.contains(&msg) {
                output.maven_errors.push(msg);
            }
            continue;
        }

        // Parse overall build result
        if line.contains("BUILD SUCCESS") {
            output.overall_status = BuildStatus::Success;

            // Close out current module's timing
            if let Some(module) = current_module.take() {
                if let Some(info) = output.modules.get_mut(&module) {
                    if info.end_time.is_none() {
                        info.end_time = Some(Instant::now());
                    }
                    info.status = BuildStatus::Success;
                }
            }

            output.overall_time = Some(build_start.elapsed().as_secs_f64());
            continue;
        }

        if line.contains("BUILD FAILURE") {
            output.overall_status = BuildStatus::Failure;

            // Close out current module's timing
            if let Some(module) = current_module.take() {
                if let Some(info) = output.modules.get_mut(&module) {
                    if info.end_time.is_none() {
                        info.end_time = Some(Instant::now());
                    }
                    info.status = BuildStatus::Failure;
                }
            }

            output.overall_time = Some(build_start.elapsed().as_secs_f64());
            continue;
        }
    }

    // Wait for Maven to complete
    let exit_status = child.wait()?;

    // Stop the spinner
    spinner_running.store(false, Ordering::Relaxed);
    let _ = spinner_thread.join();

    // Close log file to ensure all data is flushed
    drop(log_file);

    // If Maven exited with error but we didn't see "BUILD FAILURE", mark as failure
    if exit_status.code() != Some(0) && output.overall_status == BuildStatus::Building {
        output.overall_status = BuildStatus::Failure;
    }

    // Print summary
    print_summary(&output);

    // Look for test failures in surefire-reports
    let surefire_failures = find_surefire_failures(&output.reactor_order)?;
    if !surefire_failures.is_empty() {
        println!("\n{}", "=".repeat(80));
        println!("TEST FAILURES");
        println!("{}", "=".repeat(80));
        for (module, report_content) in &surefire_failures {
            println!("\n[{}]\n{}", module, report_content);
        }

        // If exactly one failure and clipboard is enabled, copy stacktrace to clipboard
        if enable_clipboard && surefire_failures.len() == 1 {
            let stacktrace = &surefire_failures[0].1;
            if let Err(e) = copy_to_clipboard(stacktrace) {
                eprintln!("Warning: Could not copy to clipboard: {}", e);
            }
        }
    } else if output.overall_status == BuildStatus::Failure {
        // Build failed but no surefire-reports found
        if !output.maven_errors.is_empty() {
            println!("\n{}", "=".repeat(80));
            println!("BUILD ERRORS");
            println!("{}", "=".repeat(80));
            println!();
            for error in &output.maven_errors {
                println!("{}", error);
            }
        } else {
            println!("\n{}", "=".repeat(80));
            println!("TEST FAILURE DETAILS");
            println!("{}", "=".repeat(80));
            println!("\nNo surefire-reports found in target directories.");
            println!("Check the following:");
            for module in &output.reactor_order {
                println!("  - {}/target/surefire-reports/", module);
            }
        }
    }

    // Exit with Maven's exit code
    std::process::exit(exit_status.code().unwrap_or(1));
}

fn parse_test_failure_header(_line: &str) -> Option<TestFailure> {
    Some(TestFailure {
        stack_trace: Vec::new(),
    })
}

fn print_summary(output: &MvnOutput) {
    println!("\n{}", "=".repeat(80));
    println!("BUILD SUMMARY");
    println!("{}", "=".repeat(80));

    println!("\nModule Status:");
    for module_name in &output.reactor_order {
        if let Some(info) = output.modules.get(module_name) {
            let status_icon = match info.status {
                BuildStatus::Success => "OK",
                BuildStatus::Failure => "FAIL",
                BuildStatus::Building => "...",
            };

            let time_str = if let (Some(start), Some(end)) = (info.start_time, info.end_time) {
                let duration = end.duration_since(start).as_secs_f64();
                format!("{:.2}s", duration)
            } else {
                "N/A".to_string()
            };

            println!("  {} {} [{}]", status_icon, info.name, time_str);
        }
    }

    println!(
        "\nOverall Status: {}",
        match output.overall_status {
            BuildStatus::Success => "OK SUCCESS",
            BuildStatus::Failure => "FAIL FAILURE",
            BuildStatus::Building => "... BUILDING",
        }
    );

    if let Some(time) = output.overall_time {
        println!("Total Time: {:.2}s", time);
    }

    if output.tests_run > 0 {
        let passed = output
            .tests_run
            .saturating_sub(output.tests_failed + output.tests_errored);
        let failed_total = output.tests_failed + output.tests_errored;
        println!(
            "Tests: {} run, {} passed, {} failed",
            output.tests_run, passed, failed_total
        );
    }
}

fn find_surefire_failures(modules: &[String]) -> Result<Vec<(String, String)>> {
    let mut failures = Vec::new();

    // Look for surefire-reports in each module's target directory
    for module in modules {
        let surefire_path = format!("{}/target/surefire-reports", module);

        if !Path::new(&surefire_path).exists() {
            continue;
        }

        // Read all .txt and .xml files from the surefire-reports directory
        if let Ok(entries) = fs::read_dir(&surefire_path) {
            let mut reports: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    let path = e.path();
                    let ext = path.extension().and_then(|s| s.to_str());
                    ext == Some("txt") || ext == Some("xml")
                })
                .collect();

            // Sort by filename for consistent output
            reports.sort_by(|a, b| {
                let a_name = a.file_name();
                let b_name = b.file_name();
                a_name.cmp(&b_name)
            });

            for entry in reports {
                let path = entry.path();
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if let Ok(content) = fs::read_to_string(&path) {
                    // Handle XML files (TEST-*.xml) - these contain detailed failure info
                    if filename.starts_with("TEST-") && filename.ends_with(".xml") {
                        if let Some(extracted) = extract_xml_failures(&content) {
                            if !extracted.is_empty() {
                                failures.push((
                                    module.clone(),
                                    format!("--- {} ---\n{}", filename, extracted),
                                ));
                            }
                        }
                    }
                    // Handle txt files (summary reports)
                    else if !filename.starts_with("TEST-") {
                        // Only include files with actual failures or errors
                        if content.contains("FAILURE") || content.contains("ERROR") {
                            let filtered = filter_stack_trace(&content);
                            failures.push((
                                module.clone(),
                                format!("--- {} ---\n{}", filename, filtered),
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(failures)
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    // Try clipboard tools in order: wl-copy (Wayland), xclip (X11), pbcopy (macOS)
    let tools = vec!["wl-copy", "xclip", "pbcopy"];

    for tool in tools {
        // Check if tool exists
        if Command::new("which")
            .arg(tool)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            let mut child = Command::new(tool).stdin(Stdio::piped()).spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
                drop(stdin);
            }

            child.wait()?;
            return Ok(());
        }
    }

    Err(anyhow::anyhow!("No clipboard tool found"))
}

fn fetch_dad_joke() -> Result<DadJoke> {
    let joke = ureq::get("https://icanhazdadjoke.com/")
        .header("Accept", "application/json")
        .header("User-Agent", "mvnx")
        .call()?
        .into_body()
        .read_json::<DadJoke>()?;
    Ok(joke)
}

fn print_help() {
    println!("mvnx - Maven wrapper with improved output");
    println!();
    println!("USAGE:");
    println!("    mvnx [OPTIONS] [MAVEN_ARGS]...");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help              Show this help message");
    println!("    --mvnhelp               Show Maven help (mvn --help)");
    println!("    --mvnd                  Use Maven Daemon (mvnd) instead of mvn");
    println!("    -l, --log-file <file>   Write all Maven output to file");
    println!("    --clip                  Copy test stacktrace to clipboard on single failure");
    println!("    -j                      Show dad jokes every 30 seconds during build");
    println!("    -ji <seconds>           Show dad jokes at custom interval (implies -j)");
    println!();
    println!("MAVEN_ARGS:");
    println!("    Any arguments that would normally be passed to Maven");
    println!();
    println!("EXAMPLES:");
    println!("    mvnx clean install");
    println!("    mvnx --mvnd clean install");
    println!("    mvnx --clip test");
    println!("    mvnx -l build.log clean package");
    println!("    mvnx -j clean package");
    println!("    mvnx -ji 10 test");
    println!("    mvnx --mvnhelp");
}
