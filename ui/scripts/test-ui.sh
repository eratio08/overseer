#!/bin/bash
# UI Testing Script using agent-browser
# Optimized for AI agent feedback loops
#
# Usage: ./scripts/test-ui.sh [command] [args]
#
# Quick commands for AI agent iteration:
#   quick           - Fast snapshot + screenshot (< 3s)
#   verify [checks] - Verify specific assertions
#   flow [name]     - Run interaction flow
#
# Full commands:
#   snapshot        - Take snapshot of current UI state
#   screenshot      - Capture screenshot
#   test            - Run full test suite
#   watch           - Continuously test UI (dev mode)
#   interact        - Interactive mode (keeps browser open)

set -e

# Config
DEV_URL="${UI_TEST_URL:-http://localhost:5173}"
OUTPUT_DIR="${UI_TEST_OUTPUT:-./test-results}"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[PASS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[FAIL]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }
log_result() { echo -e "${BOLD}[RESULT]${NC} $1"; }

# Cleanup handler
cleanup() {
    npx agent-browser close 2>/dev/null || true
}

# Fast browser init (no extra sleep)
init_browser_fast() {
    npx agent-browser open "$DEV_URL" 2>/dev/null
    npx agent-browser wait --load networkidle 2>/dev/null
}

# Standard browser init with hydration wait
init_browser() {
    log_step "Opening $DEV_URL..."
    npx agent-browser open "$DEV_URL"
    npx agent-browser wait --load networkidle
    sleep 0.5  # Minimal wait for React hydration
}

# ============================================================================
# QUICK - Fast single-iteration check (< 3s)
# Optimized for AI agent feedback loop
# ============================================================================
cmd_quick() {
    trap cleanup EXIT
    mkdir -p "$OUTPUT_DIR"

    init_browser_fast

    # Capture everything in parallel-ish
    local snapshot=$(npx agent-browser snapshot -d 3 2>/dev/null)
    local interactive=$(npx agent-browser snapshot -i 2>/dev/null)
    npx agent-browser screenshot "$OUTPUT_DIR/quick.png" 2>/dev/null

    cleanup

    echo "=== UI STATE ==="
    echo ""
    echo "Screenshot: $OUTPUT_DIR/quick.png"
    echo ""
    echo "--- Interactive Elements ---"
    echo "$interactive"
    echo ""
    echo "--- Structure (depth 3) ---"
    echo "$snapshot"
    echo ""
    echo "=== QUICK CHECKS ==="

    # Quick validations
    local pass=0
    local fail=0

    if echo "$snapshot" | grep -q "Overseer"; then
        log_info "App loaded (Overseer heading)"
        ((pass++))
    else
        log_error "App not loaded"
        ((fail++))
    fi

    if echo "$snapshot" | grep -q "Error"; then
        log_error "Error state visible"
        ((fail++))
    else
        log_info "No errors"
        ((pass++))
    fi

    if echo "$interactive" | grep -q "button"; then
        local btn_count=$(echo "$interactive" | grep -c "button" || echo 0)
        log_info "Interactive: $btn_count buttons"
        ((pass++))
    else
        log_warn "No interactive buttons"
    fi

    echo ""
    log_result "Pass: $pass, Fail: $fail"

    [ $fail -eq 0 ] && return 0 || return 1
}

# ============================================================================
# VERIFY - Run specific assertion checks
# Usage: verify filters|tasks|detail|layout|all
# ============================================================================
cmd_verify() {
    local check="${1:-all}"
    trap cleanup EXIT

    init_browser_fast
    local snapshot=$(npx agent-browser snapshot 2>/dev/null)
    local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

    local pass=0
    local fail=0

    echo "=== VERIFY: $check ==="
    echo ""

    case "$check" in
        filters)
            # Check filter buttons exist
            for filter in "All" "Active" "Done" "Blocked" "Ready"; do
                if echo "$interactive" | grep -q "button.*$filter"; then
                    log_info "Filter button: $filter"
                    ((pass++))
                else
                    log_error "Missing filter: $filter"
                    ((fail++))
                fi
            done
            ;;
        tasks)
            # Check task items exist
            if echo "$interactive" | grep -q "button.*P[1-5]"; then
                local task_count=$(echo "$interactive" | grep -c "P[1-5]" || echo 0)
                log_info "Task items visible: $task_count"
                ((pass++))
            else
                log_error "No task items visible"
                ((fail++))
            fi

            # Check status indicators
            for status in "Pending" "Completed" "Blocked"; do
                if echo "$snapshot" | grep -q "status.*$status"; then
                    log_info "Status indicator: $status"
                    ((pass++))
                fi
            done
            ;;
        detail)
            # Check detail panel
            if echo "$snapshot" | grep -q "Select a task"; then
                log_info "Detail panel: empty state"
                ((pass++))
            elif echo "$snapshot" | grep -q "term:.*ID"; then
                log_info "Detail panel: task selected"
                ((pass++))
            else
                log_warn "Detail panel: unknown state"
            fi
            ;;
        graph)
            # Check graph view elements - React Flow renders as application role
            if echo "$snapshot" | grep -q "application"; then
                log_info "Graph: React Flow canvas present"
                ((pass++))
            else
                log_error "Graph: React Flow canvas missing"
                ((fail++))
            fi

            # Check for graph controls (Zoom In, Zoom Out, Fit View)
            if echo "$interactive" | grep -qi "zoom"; then
                log_info "Graph: Controls present"
                ((pass++))
            else
                log_warn "Graph: Controls may be missing"
            fi

            # Check for minimap
            if echo "$snapshot" | grep -qi "minimap\|Mini Map"; then
                log_info "Graph: Minimap present"
                ((pass++))
            else
                log_warn "Graph: Minimap may not be detected"
            fi

            # Check for task nodes (groups in React Flow)
            if echo "$snapshot" | grep -q "group"; then
                local node_count=$(echo "$snapshot" | grep -c "group" || echo 0)
                log_info "Graph: $node_count nodes rendered"
                ((pass++))
            else
                log_warn "Graph: No graph nodes (may be empty)"
            fi
            ;;
        layout)
            # Check 3-panel layout
            local panels=$(echo "$snapshot" | grep -c "complementary\|main" || echo 0)
            if [ "$panels" -ge 3 ]; then
                log_info "3-panel layout detected ($panels regions)"
                ((pass++))
            else
                log_error "Layout incorrect ($panels regions)"
                ((fail++))
            fi
            ;;
        all)
            # Run all checks and accumulate results
            local total_pass=0
            local total_fail=0
            for check in layout filters tasks detail graph; do
                cleanup
                if cmd_verify "$check"; then
                    total_pass=$((total_pass + 1))
                else
                    total_fail=$((total_fail + 1))
                fi
            done
            echo ""
            log_result "Total: $total_pass passed, $total_fail failed"
            [ $total_fail -eq 0 ] && return 0 || return 1
            ;;
    esac

    cleanup

    echo ""
    log_result "Pass: $pass, Fail: $fail"

    [ $fail -eq 0 ] && return 0 || return 1
}

# ============================================================================
# FLOW - Run interaction flows
# Usage: flow filter|select|filter-select
# ============================================================================
cmd_flow() {
    local flow="${1:-filter-select}"
    trap cleanup EXIT
    mkdir -p "$OUTPUT_DIR"

    init_browser

    echo "=== FLOW: $flow ==="
    echo ""

    case "$flow" in
        filter)
            # Test filter switching
            log_step "Testing filter buttons..."

            for filter_ref in "@e1" "@e2" "@e3" "@e4" "@e5"; do
                npx agent-browser click "$filter_ref" 2>/dev/null || continue
                sleep 0.3
                local snap=$(npx agent-browser snapshot -i 2>/dev/null)
                local task_count=$(echo "$snap" | grep -c "button.*P[1-5]" || echo 0)
                log_info "Filter $filter_ref: $task_count tasks visible"
            done

            npx agent-browser screenshot "$OUTPUT_DIR/flow-filter.png"
            ;;
        select)
            # Test task selection
            log_step "Testing task selection..."

            local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

            # Find first task button (not filter) - format is [ref=e6]
            local task_ref=$(echo "$interactive" | grep "button.*P[1-5]" | head -1 | sed -n 's/.*\[ref=\(e[0-9]*\)\].*/\1/p')
            task_ref="@$task_ref"

            if [ "$task_ref" != "@" ] && [ -n "$task_ref" ]; then
                log_step "Clicking task $task_ref..."
                npx agent-browser click "$task_ref"
                sleep 0.5

                local detail=$(npx agent-browser snapshot 2>/dev/null)
                if echo "$detail" | grep -q "term:.*ID"; then
                    log_info "Detail panel updated with task info"
                else
                    log_error "Detail panel did not update"
                fi

                npx agent-browser screenshot "$OUTPUT_DIR/flow-select.png"
            else
                log_warn "No task to select"
            fi
            ;;
        filter-select)
            # Combined flow: filter then select
            log_step "Testing filter + select flow..."

            # Click "Blocked" filter
            npx agent-browser click "@e4" 2>/dev/null
            sleep 0.3
            log_info "Clicked Blocked filter"

            # Get fresh snapshot
            local snap=$(npx agent-browser snapshot -i 2>/dev/null)
            local task_ref=$(echo "$snap" | grep "button.*Blocked.*P[1-5]" | head -1 | grep -o 'ref=e[0-9]*' | sed 's/ref=/@/')

            if [ -n "$task_ref" ]; then
                npx agent-browser click "$task_ref"
                sleep 0.5
                log_info "Selected task $task_ref"

                local detail=$(npx agent-browser snapshot 2>/dev/null)
                if echo "$detail" | grep -q "Blocked By"; then
                    log_info "Detail shows blocked task info"
                fi
            fi

            npx agent-browser screenshot "$OUTPUT_DIR/flow-filter-select.png"
            ;;
        graph-select)
            # Test clicking a node in the graph view
            log_step "Testing graph node selection..."

            # Get the full snapshot to find graph nodes
            local snap=$(npx agent-browser snapshot 2>/dev/null)
            local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

            # Check if graph is present (React Flow uses "application" role)
            if ! echo "$snap" | grep -q "application"; then
                log_error "Graph view not found"
                return 1
            fi
            log_info "Graph view present"

            # Find a clickable task node in the graph
            # Graph nodes are rendered as divs with onClick, look for Milestone/Task/Subtask text
            local graph_node_ref=$(echo "$interactive" | grep -E "Milestone|Task|Subtask" | head -1 | grep -o '@e[0-9]*' | head -1)

            if [ -n "$graph_node_ref" ]; then
                log_step "Clicking graph node $graph_node_ref..."
                npx agent-browser click "$graph_node_ref"
                sleep 0.5

                # Check if detail panel updated
                local detail=$(npx agent-browser snapshot 2>/dev/null)
                if echo "$detail" | grep -q "term:.*ID"; then
                    log_info "Graph click updated detail panel"
                else
                    log_warn "Detail panel may not have updated"
                fi
            else
                log_warn "No clickable graph nodes found (may be empty state)"
            fi

            npx agent-browser screenshot "$OUTPUT_DIR/flow-graph-select.png"
            ;;
        graph-zoom)
            # Test graph zoom controls
            log_step "Testing graph zoom controls..."

            # Look for zoom buttons in controls
            local interactive=$(npx agent-browser snapshot -i 2>/dev/null)

            # Find zoom in button (usually has + or zoom-in label)
            local zoom_in=$(echo "$interactive" | grep -i "zoom.*in\|button.*\+" | head -1 | grep -o '@e[0-9]*' | head -1)

            if [ -n "$zoom_in" ]; then
                log_step "Clicking zoom in $zoom_in..."
                npx agent-browser click "$zoom_in"
                sleep 0.3
                npx agent-browser click "$zoom_in"
                sleep 0.3
                log_info "Zoomed in twice"
            else
                log_warn "Zoom controls not found via interactive snapshot"
            fi

            npx agent-browser screenshot "$OUTPUT_DIR/flow-graph-zoom.png"
            ;;
    esac

    cleanup
    log_result "Flow complete. Screenshot saved."
}

# ============================================================================
# Standard commands (kept for compatibility)
# ============================================================================

cmd_snapshot() {
    trap cleanup EXIT
    init_browser
    log_step "Interactive elements:"
    npx agent-browser snapshot -i
    echo ""
    log_step "Full structure:"
    npx agent-browser snapshot -d 3
    cleanup
}

cmd_screenshot() {
    trap cleanup EXIT
    local output="${1:-$OUTPUT_DIR/screenshot-$TIMESTAMP.png}"
    mkdir -p "$(dirname "$output")"

    init_browser
    npx agent-browser screenshot "$output"
    log_info "Screenshot saved to $output"
    cleanup
}

cmd_test() {
    mkdir -p "$OUTPUT_DIR"
    local test_dir="$OUTPUT_DIR/test-$TIMESTAMP"
    mkdir -p "$test_dir"

    trap cleanup EXIT

    echo "=== FULL TEST SUITE ==="
    echo ""

    # 1. Initial load test
    log_step "Test 1: Initial page load"
    init_browser
    npx agent-browser screenshot "$test_dir/01-initial.png"
    npx agent-browser snapshot > "$test_dir/01-snapshot.txt"

    # 2. Check critical elements exist
    log_step "Test 2: Checking critical elements..."
    local snapshot=$(npx agent-browser snapshot)

    if echo "$snapshot" | grep -q "Overseer"; then
        log_info "Overseer heading found"
    else
        log_error "Overseer heading NOT found"
        exit 1
    fi

    if echo "$snapshot" | grep -q "complementary" && echo "$snapshot" | grep -q "main"; then
        log_info "3-panel layout detected"
    else
        log_error "Layout structure incorrect"
        exit 1
    fi

    # 3. Check for loading/error states
    log_step "Test 3: Checking data loading..."
    if echo "$snapshot" | grep -q "Loading"; then
        log_warn "Still loading data"
    elif echo "$snapshot" | grep -q "Error"; then
        log_error "Error state detected"
        npx agent-browser screenshot "$test_dir/error-state.png"
    elif echo "$snapshot" | grep -q "No tasks found"; then
        log_info "Empty state (no tasks)"
    else
        log_info "Tasks loaded successfully"
    fi

    # 4. Interactive elements test
    log_step "Test 4: Checking interactivity..."
    local interactive=$(npx agent-browser snapshot -i)

    if [ "$interactive" = "(no interactive elements)" ]; then
        log_info "No interactive elements (empty state)"
    else
        local count=$(echo "$interactive" | wc -l | tr -d ' ')
        log_info "Interactive elements found: $count"
    fi

    # 5. Filter test
    log_step "Test 5: Filter functionality..."
    if echo "$interactive" | grep -q "button.*All"; then
        log_info "Filter buttons present"
    else
        log_warn "Filter buttons not found"
    fi

    # 6. Graph view test
    log_step "Test 6: Graph view..."
    if echo "$snapshot" | grep -q "application"; then
        log_info "Graph view (React Flow) rendered"
    else
        log_error "Graph view not found"
    fi

    # Check for graph nodes
    if echo "$snapshot" | grep -q "Milestone\|Task\|Subtask"; then
        local node_count=$(echo "$snapshot" | grep -c "Milestone\|Task\|Subtask" || echo 0)
        log_info "Graph nodes rendered: $node_count"
    else
        log_warn "No graph nodes (may be empty state)"
    fi

    # Check for graph controls (zoom buttons)
    if echo "$interactive" | grep -qi "zoom\|fit"; then
        log_info "Graph controls present"
    else
        log_warn "Graph controls not detected"
    fi

    npx agent-browser screenshot "$test_dir/final.png"

    cleanup

    echo ""
    log_result "Test suite complete! Results in: $test_dir"
    echo "  Screenshots: $(ls -1 "$test_dir"/*.png 2>/dev/null | wc -l | tr -d ' ') files"
}

cmd_watch() {
    log_step "Starting watch mode (Ctrl+C to stop)..."

    while true; do
        echo ""
        echo "--- $(date +%H:%M:%S) ---"

        npx agent-browser open "$DEV_URL" 2>/dev/null
        npx agent-browser wait --load networkidle 2>/dev/null

        local snapshot=$(npx agent-browser snapshot 2>/dev/null)

        if echo "$snapshot" | grep -q "Error"; then
            log_error "UI error detected!"
            npx agent-browser screenshot "./watch-error-$(date +%s).png"
        elif echo "$snapshot" | grep -q "Overseer"; then
            log_info "UI OK"
        else
            log_warn "Unexpected state"
        fi

        npx agent-browser close 2>/dev/null || true
        sleep "${UI_TEST_INTERVAL:-5}"
    done
}

cmd_interact() {
    log_step "Interactive mode - browser stays open"
    log_step "Use 'npx agent-browser <command>' to interact"
    log_step "Run 'npx agent-browser close' when done"
    echo ""

    init_browser

    echo "--- Interactive Elements ---"
    npx agent-browser snapshot -i

    echo ""
    echo "Commands:"
    echo "  npx agent-browser snapshot -i"
    echo "  npx agent-browser click @e1"
    echo "  npx agent-browser screenshot x.png"
    echo "  npx agent-browser close"
}

cmd_click() {
    trap cleanup EXIT
    local ref="${1:-@e1}"

    init_browser

    echo "--- Before ---"
    npx agent-browser snapshot -i

    log_step "Clicking $ref..."
    npx agent-browser click "$ref" || log_warn "Click failed"

    sleep 0.5

    echo ""
    echo "--- After ---"
    npx agent-browser snapshot -i

    npx agent-browser screenshot "$OUTPUT_DIR/after-click-$TIMESTAMP.png"
    cleanup
}

# Main
case "${1:-quick}" in
    quick|q)
        cmd_quick
        ;;
    verify|v)
        cmd_verify "$2"
        ;;
    flow|f)
        cmd_flow "$2"
        ;;
    snapshot|snap|s)
        cmd_snapshot
        ;;
    screenshot|ss)
        cmd_screenshot "$2"
        ;;
    test|t)
        cmd_test
        ;;
    watch|w)
        cmd_watch
        ;;
    interact|i)
        cmd_interact
        ;;
    click|c)
        cmd_click "$2"
        ;;
    help|--help|-h)
        echo "UI Testing Script (AI Agent Optimized)"
        echo ""
        echo "Usage: $0 [command] [args]"
        echo ""
        echo "Quick commands (for AI feedback loops):"
        echo "  quick, q          Fast snapshot + checks (< 3s)"
        echo "  verify, v [what]  Verify: filters|tasks|detail|layout|graph|all"
        echo "  flow, f [name]    Flow: filter|select|filter-select|graph-select|graph-zoom"
        echo ""
        echo "Standard commands:"
        echo "  snapshot, s       Show UI structure"
        echo "  screenshot, ss    Capture screenshot"
        echo "  test, t           Run full test suite"
        echo "  watch, w          Continuous testing"
        echo "  interact, i       Open browser for manual testing"
        echo "  click, c [ref]    Click element and show result"
        echo ""
        echo "Environment:"
        echo "  UI_TEST_URL       Dev server URL (default: http://localhost:5173)"
        echo "  UI_TEST_OUTPUT    Output dir (default: ./test-results)"
        ;;
    *)
        log_error "Unknown command: $1"
        echo "Run '$0 help' for usage"
        exit 1
        ;;
esac
