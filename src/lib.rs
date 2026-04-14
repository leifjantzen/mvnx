use regex::Regex;

pub fn parse_reactor_module(line: &str) -> Option<String> {
    // Pattern: "1. com.example:module-name"
    let trimmed = line.trim();
    let re = Regex::new(r"^\d+\.\s+(.+)$").ok()?;
    let caps = re.captures(trimmed)?;
    let full_module = caps.get(1)?.as_str().trim();

    // Return the full module identifier (groupId:artifactId or just name)
    Some(full_module.to_string())
}

pub fn parse_module_start(line: &str) -> Option<String> {
    // Pattern: "[INFO] Building com.example:module-name 1.0"
    let re = Regex::new(r"\[INFO\]\s+Building\s+([^\s]+)").ok()?;
    let caps = re.captures(line)?;
    let full_module = caps.get(1)?.as_str();

    // Return the full module identifier (groupId:artifactId or just name)
    Some(full_module.to_string())
}

pub fn parse_test_results(line: &str) -> Option<(u32, u32, u32, u32)> {
    // Pattern: "Tests run: 5, Failures: 1, Errors: 0, Skipped: 0"
    let re =
        Regex::new(r"Tests run: (\d+), Failures: (\d+), Errors: (\d+), Skipped: (\d+)").ok()?;
    let caps = re.captures(line)?;

    let run = caps.get(1)?.as_str().parse::<u32>().ok()?;
    let failed = caps.get(2)?.as_str().parse::<u32>().ok()?;
    let errored = caps.get(3)?.as_str().parse::<u32>().ok()?;
    let skipped = caps.get(4)?.as_str().parse::<u32>().ok()?;

    Some((run, failed, errored, skipped))
}

pub fn filter_stack_trace(content: &str) -> String {
    // Filter out lines from uninteresting packages
    let uninteresting_prefixes = [
        "at io",
        "at java",
        "at kotlin",
        "at org",
        "at feign",
        "at jdk",
        "at com",
    ];

    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !uninteresting_prefixes
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn extract_xml_failures(content: &str) -> Option<String> {
    let mut failures = Vec::new();

    // Extract failure messages from CDATA sections
    let failure_re =
        Regex::new(r#"<failure[^>]*message="([^"]*)"[^>]*><!\[CDATA\[([\s\S]*?)\]\]></failure>"#)
            .ok()?;

    for caps in failure_re.captures_iter(content) {
        if let (Some(msg), Some(trace)) = (caps.get(1), caps.get(2)) {
            let trace_text = trace.as_str();
            let filtered = filter_stack_trace(trace_text);
            failures.push(format!("Assertion: {}\n{}", msg.as_str(), filtered));
        }
    }

    // Extract error messages from CDATA sections
    let error_re =
        Regex::new(r#"<error[^>]*message="([^"]*)"[^>]*><!\[CDATA\[([\s\S]*?)\]\]></error>"#)
            .ok()?;

    for caps in error_re.captures_iter(content) {
        if let (Some(msg), Some(trace)) = (caps.get(1), caps.get(2)) {
            let trace_text = trace.as_str();
            let filtered = filter_stack_trace(trace_text);
            failures.push(format!("Error: {}\n{}", msg.as_str(), filtered));
        }
    }

    if failures.is_empty() {
        None
    } else {
        Some(failures.join("\n\n"))
    }
}
