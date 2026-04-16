# mvnx

Maven wrapper written in Rust that provides clean and readable output for multi-module projects.

## Features

- **Clean output**: Shows only essential information
- **Reactor build order**: Shows the order modules are built in
- **Progress indicator**: Shows which module is being built currently
- **Build summary**: Shows:
  - Reactor build order
  - Module status (OK/FAIL) with time spent
  - Overall build status and total time spent
- **Test failure details**: Shows stacktraces for failed tests (from both `.txt` and XML reports)
- **XML-based error parsing**: Parses Maven Surefire XML reports for detailed error messages
- **Dad jokes**: Optional humor during the build
- **Clipboard copying**: Copies stacktraces to clipboard automatically

## Usage

The wrapper takes the same arguments as `mvn`:

```bash
# Basic build
mvnx clean install

# Skip tests
mvnx clean install -DskipTests

# Specific goal
mvnx clean test

# Custom settings
mvnx -s ~/settings.xml clean package
```

### Special flags

- `-h, --help`: Show help message
- `--mvnhelp`: Show Maven's help message (mvn --help)
- `--clip`: Copy stacktrace to clipboard when exactly one test fails
- `-j`: Show dad jokes every 30 seconds during the build
- `-ji <seconds>`: Show dad jokes with custom interval (implies `-j`)

Examples:

```bash
mvnx clean install
mvnx --clip test
mvnx -j clean install
mvnx -ji 20 test
mvnx --clip -j package
```

## Output Example

```
> Building module-a
> Building module-b

================================================================================
BUILD SUMMARY
================================================================================

Reactor Build Order:
  1. module-a
  2. module-b

Module Status:
  OK module-a [2.34s]
  OK module-b [5.67s]

Overall Status: OK SUCCESS
Total Time: 8.01s
Tests: 45 run, 43 passed, 2 failed

================================================================================
TEST FAILURES
================================================================================

[module-b]

--- TestFailureTest.txt ---
java.lang.AssertionError: Expected 42 but got 41
  at TestFailureTest.testSomething(TestFailureTest.java:15)

Stacktrace copied to clipboard.
```

## Testing

Run unit tests:

```bash
cargo test
```

The tests cover:
- Parsing reactor modules from Maven output
- Parsing module build start lines
- Parsing test result summaries
- Filtering stacktraces (removes framework lines, keeps user code)
- Parsing Maven Surefire XML reports for error messages and errors

## Installation

### Build from source

```bash
cargo build --release
# Binary at ./target/release/mvnx
```

### Add to PATH

Copy the binary to a location in PATH:

```bash
cp target/release/mvnx ~/.local/bin/
# or
sudo cp target/release/mvnx /usr/local/bin/
```

Use it then as:
```bash
mvnx clean install
```

## How it works

The wrapper:
1. Starts Maven as a subprocess
2. Captures and parses its stdout output
3. Extracts key information:
   - Reactor module order
   - Module build progress
   - Build completion status and time spent
   - Test failure information
4. Parses Maven Surefire reports:
   - Reads `.txt` files for summaries
   - Reads `TEST-*.xml` files for detailed stacktraces and error messages
   - Filters out framework-related stacktrace lines
5. Shows a clean summary with error details
6. Exits with Maven's exit code

## Requirements

- Maven installed and available in PATH
- A clipboard tool:
  - `wl-copy` (Wayland)
  - `xclip` (X11)
  - `pbcopy` (macOS)

## Limitations

- Optimized for standard Maven output format
- Test failure parsing may need adjustment based on your test framework
- Requires `mvn` installed and in PATH
