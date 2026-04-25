#!/bin/bash
# AI-Powered UI Review using agent-browser
# Captures UI state for AI analysis in feedback loops
#
# Usage: ./scripts/ai-review.sh [mode]
#
# Designed for AI agent iteration:
# 1. Run ./scripts/ai-review.sh capture
# 2. AI analyzes output + screenshot
# 3. AI makes changes
# 4. Repeat

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UI_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${UI_TEST_OUTPUT:-$UI_DIR/test-results}"
DEV_URL="${UI_TEST_URL:-http://localhost:5173}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

cleanup() {
    npx agent-browser close 2>/dev/null || true
}

# ============================================================================
# CAPTURE - Capture full UI state for AI analysis
# Returns structured data optimized for LLM consumption
# ============================================================================
cmd_capture() {
    trap cleanup EXIT
    mkdir -p "$OUTPUT_DIR"

    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
    sleep 0.5

    # Capture all data
    local screenshot="$OUTPUT_DIR/ui-state.png"
    npx agent-browser screenshot "$screenshot" 2>/dev/null

    local snapshot=$(npx agent-browser snapshot 2>/dev/null)
    local interactive=$(npx agent-browser snapshot -i 2>/dev/null)
    local structure=$(npx agent-browser snapshot -d 4 2>/dev/null)

    cleanup

    # Output structured for AI consumption
    cat << EOF
# UI State Capture

## Screenshot
Path: $screenshot

## Interactive Elements
\`\`\`
$interactive
\`\`\`

## Accessibility Tree (depth 4)
\`\`\`
$structure
\`\`\`

## Metrics
- Total lines in snapshot: $(echo "$snapshot" | wc -l | tr -d ' ')
- Interactive elements: $(echo "$interactive" | wc -l | tr -d ' ')
- Buttons: $(echo "$interactive" | grep -c "button" || echo 0)
- Status indicators: $(echo "$snapshot" | grep -c "status" || echo 0)

## Quick Health Check
EOF

    # Health checks
    if echo "$snapshot" | grep -q "Error"; then
        echo "- [FAIL] Error state visible"
    else
        echo "- [PASS] No errors"
    fi

    if echo "$snapshot" | grep -q "Overseer"; then
        echo "- [PASS] App loaded"
    else
        echo "- [FAIL] App not loaded"
    fi

    if echo "$interactive" | grep -q "button.*All"; then
        echo "- [PASS] Filter buttons present"
    else
        echo "- [WARN] Filter buttons not found"
    fi

    if echo "$interactive" | grep -q "button.*P[1-5]"; then
        echo "- [PASS] Task items visible"
    else
        echo "- [WARN] No task items"
    fi

    if echo "$snapshot" | grep -q "React Flow"; then
        echo "- [PASS] Graph view rendered"
    else
        echo "- [WARN] Graph view not detected"
    fi

    if echo "$snapshot" | grep -q "Milestone\|Task\|Subtask"; then
        local node_count=$(echo "$snapshot" | grep -c "Milestone\|Task\|Subtask" || echo 0)
        echo "- [PASS] Graph nodes: $node_count"
    else
        echo "- [WARN] No graph nodes"
    fi
}

# ============================================================================
# FLOW - Run interaction and capture before/after
# ============================================================================
cmd_flow() {
    local action="${1:-filter}"
    trap cleanup EXIT
    mkdir -p "$OUTPUT_DIR"

    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
    sleep 0.5

    echo "# Interaction Flow: $action"
    echo ""

    echo "## Before"
    echo "\`\`\`"
    npx agent-browser snapshot -i 2>/dev/null
    echo "\`\`\`"
    npx agent-browser screenshot "$OUTPUT_DIR/flow-before.png" 2>/dev/null
    echo ""

    case "$action" in
        filter-done)
            echo "## Action: Click 'Done' filter"
            npx agent-browser click "@e3" 2>/dev/null
            ;;
        filter-blocked)
            echo "## Action: Click 'Blocked' filter"
            npx agent-browser click "@e4" 2>/dev/null
            ;;
        filter-ready)
            echo "## Action: Click 'Ready' filter"
            npx agent-browser click "@e5" 2>/dev/null
            ;;
        select-first)
            echo "## Action: Click first task"
            local first=$(npx agent-browser snapshot -i 2>/dev/null | grep "button.*P[1-5]" | head -1 | grep -o '@e[0-9]*' | head -1)
            if [ -n "$first" ]; then
                npx agent-browser click "$first" 2>/dev/null
            fi
            ;;
        graph-node)
            echo "## Action: Click graph node"
            local node=$(npx agent-browser snapshot -i 2>/dev/null | grep -E "Milestone|Task|Subtask" | head -1 | grep -o '@e[0-9]*' | head -1)
            if [ -n "$node" ]; then
                echo "Clicking node: $node"
                npx agent-browser click "$node" 2>/dev/null
            else
                echo "(no graph nodes found)"
            fi
            ;;
        *)
            echo "## Action: Click @e1"
            npx agent-browser click "@e1" 2>/dev/null
            ;;
    esac

    sleep 0.5

    echo ""
    echo "## After"
    echo "\`\`\`"
    npx agent-browser snapshot -i 2>/dev/null
    echo "\`\`\`"
    npx agent-browser screenshot "$OUTPUT_DIR/flow-after.png" 2>/dev/null

    echo ""
    echo "## Screenshots"
    echo "- Before: $OUTPUT_DIR/flow-before.png"
    echo "- After: $OUTPUT_DIR/flow-after.png"

    cleanup
}

# ============================================================================
# DIFF - Compare two states (useful after code changes)
# ============================================================================
cmd_diff() {
    trap cleanup EXIT
    mkdir -p "$OUTPUT_DIR"

    # Capture current state
    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
    sleep 0.5

    local current=$(npx agent-browser snapshot -i 2>/dev/null)
    npx agent-browser screenshot "$OUTPUT_DIR/current.png" 2>/dev/null

    cleanup

    echo "# State Comparison"
    echo ""

    # Check if previous state exists
    if [ -f "$OUTPUT_DIR/previous-interactive.txt" ]; then
        local previous=$(cat "$OUTPUT_DIR/previous-interactive.txt")

        echo "## Changes Detected"
        echo "\`\`\`diff"
        diff <(echo "$previous") <(echo "$current") || true
        echo "\`\`\`"
    else
        echo "## Current State (no previous to compare)"
        echo "\`\`\`"
        echo "$current"
        echo "\`\`\`"
    fi

    # Save current as previous for next run
    echo "$current" > "$OUTPUT_DIR/previous-interactive.txt"

    echo ""
    echo "Screenshot: $OUTPUT_DIR/current.png"
    echo "Previous state saved for next comparison."
}

# ============================================================================
# ANALYZE - Output data formatted for specific analysis tasks
# ============================================================================
cmd_analyze() {
    local focus="${1:-ux}"
    trap cleanup EXIT

    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
    sleep 0.5

    local snapshot=$(npx agent-browser snapshot 2>/dev/null)
    local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

    cleanup

    case "$focus" in
        ux)
            cat << EOF
# UX Analysis Request

## Context
Task management UI with hierarchical task list, filters, and detail panel.

## Current Interactive Elements
\`\`\`
$interactive
\`\`\`

## Questions to Evaluate
1. Is information hierarchy clear? (Most important info prominent?)
2. Are filters discoverable and their state obvious?
3. Do status indicators communicate clearly?
4. Is task selection feedback immediate and visible?
5. Is the 3-panel layout balanced and usable?

## Your Task
Identify the top 3 UX improvements with specific, actionable suggestions.
EOF
            ;;
        a11y)
            cat << EOF
# Accessibility Analysis Request

## Full Accessibility Tree
\`\`\`
$snapshot
\`\`\`

## Questions to Evaluate
1. Do all interactive elements have accessible names?
2. Are status indicators announced to screen readers?
3. Is keyboard navigation logical?
4. Are there any missing ARIA attributes?
5. Is color the only way information is conveyed?

## Your Task
List accessibility issues by severity (critical/major/minor).
EOF
            ;;
        perf)
            local element_count=$(echo "$snapshot" | wc -l | tr -d ' ')
            local depth=$(echo "$snapshot" | grep -o '^[[:space:]]*' | sort -u | tail -1 | wc -c)

            cat << EOF
# Performance Analysis Request

## Metrics
- Element count: $element_count
- Interactive elements: $(echo "$interactive" | wc -l | tr -d ' ')
- Estimated max depth: $((depth / 2))

## Structure
\`\`\`
$snapshot
\`\`\`

## Questions to Evaluate
1. Is the DOM too deep? (>10 levels concerning)
2. Too many elements? (>100 in view concerning)
3. Are lists virtualized if long?
4. Excessive re-render triggers?

## Your Task
Identify performance concerns and optimization opportunities.
EOF
            ;;
    esac
}

# ============================================================================
# VERIFY - Quick pass/fail assertions
# ============================================================================
cmd_verify() {
    trap cleanup EXIT

    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
    sleep 0.5

    local snapshot=$(npx agent-browser snapshot 2>/dev/null)
    local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

    cleanup

    local pass=0
    local fail=0

    echo "# Verification Results"
    echo ""

    # Core checks
    if echo "$snapshot" | grep -q "Overseer"; then
        echo "- [PASS] App heading present"
        ((pass++))
    else
        echo "- [FAIL] App heading missing"
        ((fail++))
    fi

    if echo "$snapshot" | grep -q "Error"; then
        echo "- [FAIL] Error state visible"
        ((fail++))
    else
        echo "- [PASS] No error states"
        ((pass++))
    fi

    # Filter checks
    for f in "All" "Active" "Done" "Blocked" "Ready"; do
        if echo "$interactive" | grep -qi "button.*$f"; then
            echo "- [PASS] Filter: $f"
            ((pass++))
        else
            echo "- [FAIL] Filter missing: $f"
            ((fail++))
        fi
    done

    # Layout checks
    local panels=$(echo "$snapshot" | grep -c "complementary\|main" || echo 0)
    if [ "$panels" -ge 3 ]; then
        echo "- [PASS] 3-panel layout ($panels regions)"
        ((pass++))
    else
        echo "- [FAIL] Layout issue ($panels regions)"
        ((fail++))
    fi

    # Status indicator checks
    if echo "$snapshot" | grep -q 'status.*Completed\|status.*Blocked\|status.*Pending'; then
        echo "- [PASS] Status indicators present"
        ((pass++))
    else
        echo "- [WARN] Status indicators may be missing"
    fi

    # Graph checks
    if echo "$snapshot" | grep -q "React Flow"; then
        echo "- [PASS] Graph view rendered"
        ((pass++))
    else
        echo "- [FAIL] Graph view missing"
        ((fail++))
    fi

    if echo "$snapshot" | grep -q "Milestone\|Task\|Subtask"; then
        echo "- [PASS] Graph nodes present"
        ((pass++))
    else
        echo "- [WARN] No graph nodes (may be empty)"
    fi

    echo ""
    echo "## Summary"
    echo "Pass: $pass, Fail: $fail"

    [ $fail -eq 0 ] && exit 0 || exit 1
}

# Main
case "${1:-capture}" in
    capture|c)
        cmd_capture
        ;;
    flow|f)
        cmd_flow "$2"
        ;;
    diff|d)
        cmd_diff
        ;;
    analyze|a)
        cmd_analyze "$2"
        ;;
    verify|v)
        cmd_verify
        ;;
    help|--help|-h)
        cat << EOF
AI-Powered UI Review

Usage: $0 [command] [args]

Commands for AI feedback loops:
  capture, c           Capture full UI state (default)
  flow, f [action]     Run interaction flow
                       Actions: filter-done, filter-blocked, filter-ready, select-first, graph-node
  diff, d              Compare current vs previous state
  analyze, a [focus]   Format data for specific analysis
                       Focus: ux, a11y, perf
  verify, v            Quick pass/fail assertions

Workflow:
  1. Run: ./scripts/ai-review.sh capture
  2. AI analyzes the output + screenshot
  3. AI makes code changes
  4. Run: ./scripts/ai-review.sh diff
  5. Repeat until satisfied

Environment:
  UI_TEST_URL      Dev server URL (default: http://localhost:5173)
  UI_TEST_OUTPUT   Output directory (default: ./test-results)
EOF
        ;;
    *)
        echo "Unknown command: $1"
        echo "Run '$0 help' for usage"
        exit 1
        ;;
esac
