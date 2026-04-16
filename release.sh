#!/bin/bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO_OWNER="ljantzen"
REPO_NAME="mvnx"
WORKFLOW_NAME="Release"
MAX_WAIT_TIME=600  # 10 minutes in seconds
POLL_INTERVAL=10   # Poll every 10 seconds

# Helper functions
print_error() {
    echo -e "${RED}Error: $1${NC}" >&2
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ $1${NC}"
}

# Check prerequisites
check_prerequisites() {
    print_info "Checking prerequisites..."

    if ! command -v gh &> /dev/null; then
        print_error "GitHub CLI (gh) is not installed. Please install it: https://cli.github.com"
        exit 1
    fi

    if ! command -v cargo &> /dev/null; then
        print_error "Cargo is not installed"
        exit 1
    fi

    if ! command -v git &> /dev/null; then
        print_error "Git is not installed"
        exit 1
    fi

    print_success "All prerequisites met"
}

# Get version from Cargo.toml
get_version() {
    grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2
}

# Validate version format (semantic versioning)
validate_version() {
    local version=$1
    if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        print_error "Invalid version format: $version (expected semantic versioning: X.Y.Z)"
        return 1
    fi
    return 0
}

# Update version in Cargo.toml
update_version() {
    local new_version=$1
    local current_version=$(get_version)

    if [ "$new_version" = "$current_version" ]; then
        print_error "New version ($new_version) is the same as current version in Cargo.toml"
        return 1
    fi

    print_info "Updating version from $current_version to $new_version in Cargo.toml..."
    sed -i "s/^version = \"$current_version\"/version = \"$new_version\"/" Cargo.toml
    print_success "Version updated to $new_version"
}

# Show usage
show_usage() {
    cat << EOF
Usage: ./release.sh <version>

Arguments:
  <version>    New version to release (semantic versioning: X.Y.Z)

Example:
  ./release.sh 1.0.0
  ./release.sh 1.1.0

The script will:
1. Update Cargo.toml with the new version
2. Commit the version change
3. Create and push a version tag (v<version>)
4. Wait for GitHub Actions to complete
5. Publish to crates.io
EOF
}

# Check if working directory is clean
check_clean_working_dir() {
    if [ -n "$(git status --porcelain)" ]; then
        print_error "Working directory is not clean. Please commit or stash changes."
        git status
        exit 1
    fi
    print_success "Working directory is clean"
}

# Create and push version tag
create_and_push_tag() {
    local version=$1
    local tag="v$version"

    print_info "Creating tag $tag..."

    if git rev-parse "$tag" >/dev/null 2>&1; then
        print_error "Tag $tag already exists"
        exit 1
    fi

    git tag -a "$tag" -m "Release version $version"
    print_success "Tag $tag created"

    print_info "Pushing tag to remote..."
    git push origin "$tag"
    print_success "Tag pushed to remote"
}

# Wait for GitHub Actions workflow to complete
wait_for_workflow() {
    local tag=$1
    local elapsed=0

    print_info "Waiting for GitHub Actions workflow to complete..."
    print_info "Polling every $POLL_INTERVAL seconds (timeout: $MAX_WAIT_TIME seconds)"

    while [ $elapsed -lt $MAX_WAIT_TIME ]; do
        # Get the latest workflow run for this tag
        local run_data=$(gh api repos/$REPO_OWNER/$REPO_NAME/actions/runs \
            --jq ".workflow_runs[] | select(.head_branch == null and .event == \"push\") | select(.head_commit.tag == \"$tag\") | .[0]" 2>/dev/null || echo "")

        if [ -z "$run_data" ]; then
            # Try alternative approach: get runs for the workflow and filter by tag
            local run_data=$(gh api repos/$REPO_OWNER/$REPO_NAME/actions/workflows/release.yml/runs \
                --jq ".workflow_runs[0]" 2>/dev/null || echo "")
        fi

        if [ -z "$run_data" ]; then
            echo -n "."
            sleep $POLL_INTERVAL
            elapsed=$((elapsed + POLL_INTERVAL))
            continue
        fi

        local status=$(echo "$run_data" | jq -r '.status // "unknown"')
        local conclusion=$(echo "$run_data" | jq -r '.conclusion // "unknown"')

        if [ "$status" = "completed" ]; then
            if [ "$conclusion" = "success" ]; then
                echo ""
                print_success "GitHub Actions workflow completed successfully"
                return 0
            else
                echo ""
                print_error "GitHub Actions workflow failed with conclusion: $conclusion"
                return 1
            fi
        fi

        echo -n "."
        sleep $POLL_INTERVAL
        elapsed=$((elapsed + POLL_INTERVAL))
    done

    echo ""
    print_error "Timeout waiting for GitHub Actions workflow (${MAX_WAIT_TIME}s)"
    return 1
}

# Publish to crates.io
publish_to_crates() {
    print_info "Publishing to crates.io..."

    if ! cargo publish; then
        print_error "Failed to publish to crates.io"
        return 1
    fi

    print_success "Published to crates.io"
}

# Main function
main() {
    # Parse arguments
    if [ $# -eq 0 ]; then
        show_usage
        exit 1
    fi

    local new_version=$1

    if [ "$new_version" = "-h" ] || [ "$new_version" = "--help" ]; then
        show_usage
        exit 0
    fi

    echo "=========================================="
    echo "        Release Script for mvnx"
    echo "=========================================="
    echo ""

    # Validate version format
    if ! validate_version "$new_version"; then
        exit 1
    fi

    check_prerequisites
    check_clean_working_dir

    local current_version=$(get_version)
    print_info "Current version in Cargo.toml: $current_version"
    print_info "New version to release: $new_version"
    echo ""

    read -p "Proceed with release v$new_version to GitHub and crates.io? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Release cancelled"
        exit 0
    fi
    echo ""

    # Step 0: Update version and commit
    if ! update_version "$new_version"; then
        exit 1
    fi

    print_info "Committing version change..."
    git add Cargo.toml
    git commit -m "Bump version to $new_version

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
    print_success "Version commit created"
    echo ""

    # Step 1: Create and push tag
    create_and_push_tag "$new_version"
    echo ""

    # Step 2: Wait for GitHub Actions
    if ! wait_for_workflow "v$new_version"; then
        print_error "Cannot proceed with crates.io publication until GitHub Actions succeeds"
        exit 1
    fi
    echo ""

    # Step 3: Publish to crates.io
    if publish_to_crates; then
        echo ""
        echo "=========================================="
        print_success "Release v$new_version completed successfully!"
        echo "=========================================="
        echo "GitHub Release: https://github.com/$REPO_OWNER/$REPO_NAME/releases/tag/v$new_version"
        echo "crates.io: https://crates.io/crates/mvnx/v$new_version"
    else
        print_error "Release partially complete - tag pushed but crates.io publication failed"
        exit 1
    fi
}

main "$@"
