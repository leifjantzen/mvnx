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

# Check if tag exists on origin
tag_exists_on_origin() {
    local tag=$1
    git ls-remote --tags origin | grep -E "refs/tags/$tag$" >/dev/null 2>&1
}

# Check if version is published on crates.io
is_published_on_crates() {
    local version=$1
    # Query crates.io API to check if the version exists
    curl -s "https://crates.io/api/v1/crates/mvnx/versions" | \
        jq -e ".versions[] | select(.num == \"$version\")" >/dev/null 2>&1
}

# Create and push version tag
create_and_push_tag() {
    local version=$1
    local tag="v$version"

    if tag_exists_on_origin "$tag"; then
        print_success "Tag $tag already exists on origin, skipping tag creation"
        return 0
    fi

    print_info "Creating tag $tag..."

    if git rev-parse "$tag" >/dev/null 2>&1; then
        print_error "Tag $tag exists locally but not on origin"
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
    local run_id=""

    print_info "Waiting for GitHub Actions workflow to complete..."
    print_info "Polling every $POLL_INTERVAL seconds (timeout: $MAX_WAIT_TIME seconds)"

    # First, wait for the workflow run to be created
    print_info "Waiting for workflow run to be triggered..."
    local wait_for_run=0
    while [ $wait_for_run -lt 120 ]; do
        # Get the most recent release workflow run
        local run_list=$(gh run list --workflow release.yml --repo $REPO_OWNER/$REPO_NAME --limit 1 --json databaseId,status,conclusion,headRef 2>/dev/null || echo "")

        if [ -n "$run_list" ]; then
            run_id=$(echo "$run_list" | jq -r '.[0].databaseId // empty')
            if [ -n "$run_id" ]; then
                print_success "Workflow run created (ID: $run_id)"
                break
            fi
        fi

        echo -n "."
        sleep 5
        wait_for_run=$((wait_for_run + 5))
    done

    if [ -z "$run_id" ]; then
        echo ""
        print_error "Workflow run was not created within 2 minutes"
        return 1
    fi

    echo ""

    # Now wait for the workflow to complete
    while [ $elapsed -lt $MAX_WAIT_TIME ]; do
        local run_info=$(gh run view $run_id --repo $REPO_OWNER/$REPO_NAME --json status,conclusion 2>/dev/null || echo "")

        if [ -n "$run_info" ]; then
            local status=$(echo "$run_info" | jq -r '.status // "unknown"')
            local conclusion=$(echo "$run_info" | jq -r '.conclusion // "unknown"')

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
    local version=$1

    if is_published_on_crates "$version"; then
        print_success "Version $version is already published on crates.io, skipping publication"
        return 0
    fi

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

    # Check if tag already exists on origin
    local tag="v$new_version"

    if ! tag_exists_on_origin "$tag"; then
        # Tag doesn't exist, proceed with full release
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
    else
        # Tag already exists on origin, skip to workflow
        print_info "Tag $tag already exists on origin, resuming release process..."
        echo ""
    fi

    # Step 2: Wait for GitHub Actions
    if ! wait_for_workflow "v$new_version"; then
        print_error "Cannot proceed with crates.io publication until GitHub Actions succeeds"
        exit 1
    fi
    echo ""

    # Step 3: Publish to crates.io
    if publish_to_crates "$new_version"; then
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
