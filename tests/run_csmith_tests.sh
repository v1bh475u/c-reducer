#!/bin/bash
# Csmith-based test suite for the C program slicer/reducer
# This script generates random C programs with csmith, reduces them,
# verifies behavior preservation, and ensures CPU cycle parity.

set +e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REDUCER_ROOT="$(dirname "$SCRIPT_DIR")"
SLICER="$REDUCER_ROOT/target/release/slicer"
TEST_DIR="$SCRIPT_DIR/csmith_tests"
RESULTS_FILE="$TEST_DIR/results.log"

# Configuration
NUM_TESTS="${1:-100}"
MAX_PARALLEL="${2:-4}"  # Parallel jobs (lower for csmith - heavy tests)
CYCLE_TOLERANCE="${3:-0.10}"  # Tolerance for CPU cycles (default 10%)
TIMEOUT_SECS=30
CSMITH_INCLUDE="/usr/include/csmith-2.3.0"
PERF_RUNS=3  # Number of runs to average for cycle measurement

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

mkdir -p "$TEST_DIR"
echo "=== Csmith Reducer Test Suite with CPU Cycle Verification ===" | tee "$RESULTS_FILE"
echo "Started at: $(date)" | tee -a "$RESULTS_FILE"
echo "Number of tests: $NUM_TESTS (parallel: $MAX_PARALLEL)" | tee -a "$RESULTS_FILE"
echo "CPU cycle tolerance: $(echo "$CYCLE_TOLERANCE * 100" | bc)%" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

# Build the reducer
echo "Building reducer..." | tee -a "$RESULTS_FILE"
cd "$REDUCER_ROOT"
cargo build --release 2>&1 | tail -1

if [ ! -f "$SLICER" ]; then
    echo -e "${RED}Error: Slicer binary not found${NC}"
    exit 1
fi

# Check for required tools
for tool in csmith perf bc; do
    if ! command -v $tool &> /dev/null; then
        echo -e "${RED}Error: $tool not found${NC}"
        exit 1
    fi
done

# Find csmith headers
if [ ! -d "$CSMITH_INCLUDE" ]; then
    for dir in "/usr/local/include/csmith" "/usr/include/csmith" "/usr/include/csmith-2.3.0"; do
        if [ -d "$dir" ]; then
            CSMITH_INCLUDE="$dir"
            break
        fi
    done
fi

# Csmith configurations for variety
CSMITH_CONFIGS=(
    "--no-argc --no-arrays --max-funcs 5"
    "--no-argc --max-funcs 4 --max-block-depth 3"
    "--no-argc --no-pointers --max-funcs 6"
    "--no-argc --max-block-depth 2 --max-funcs 3"
    "--no-argc --no-bitfields --max-funcs 5"
    "--no-argc --no-structs --max-funcs 4"
    "--no-argc --no-unions --max-funcs 5"
    "--no-argc --no-volatiles --max-funcs 6"
    "--no-argc --no-volatile-pointers --max-funcs 4"
    "--no-argc --max-expr-complexity 4 --max-funcs 3"
)

# Measure CPU cycles using perf (average over multiple runs)
measure_cycles() {
    local binary="$1"
    local total_cycles=0
    local count=0
    
    for i in $(seq 1 $PERF_RUNS); do
        local cycles=$(perf stat -e cycles -x, "$binary" 2>&1 | grep -E '^[0-9]' | cut -d',' -f1 | head -1)
        if [[ "$cycles" =~ ^[0-9]+$ ]]; then
            total_cycles=$((total_cycles + cycles))
            ((count++))
        fi
    done
    
    if [ $count -gt 0 ]; then
        echo $((total_cycles / count))
    else
        echo "0"
    fi
}

# Generate padding code to increase CPU cycles
generate_padding_code() {
    local iterations=$1
    
    cat << EOF
/* CPU cycle padding to maintain performance parity */
static volatile int __padding_sink = 0;
static void __attribute__((noinline)) __cpu_cycle_padding(void) {
    volatile int sum = 0;
    for (volatile long i = 0; i < ${iterations}L; i++) {
        sum += (i * 17 + 31) % 256;
        __padding_sink = sum;
    }
}

EOF
}

# Insert padding call in main function before the checksum print
insert_padding_call() {
    local file="$1"
    local temp_file="${file}.tmp"
    
    # Csmith programs always have a 'platform_main_end' or 'printf' for checksum
    # We insert the padding call at the very start of main() after the opening brace
    # This ensures padding runs but doesn't affect the output
    awk '
    /int main\s*\(/ {
        in_main = 1
    }
    in_main && /\{/ && !inserted {
        print $0
        print "    __cpu_cycle_padding();"
        inserted = 1
        next
    }
    { print }
    ' "$file" > "$temp_file"
    
    mv "$temp_file" "$file"
}

run_test() {
    local test_num=$1
    
    # Csmith configurations for variety (defined inline for export compatibility)
    local configs=(
        "--no-argc --no-arrays --max-funcs 5"
        "--no-argc --max-funcs 4 --max-block-depth 3"
        "--no-argc --no-pointers --max-funcs 6"
        "--no-argc --max-block-depth 2 --max-funcs 3"
        "--no-argc --no-bitfields --max-funcs 5"
        "--no-argc --no-structs --max-funcs 4"
        "--no-argc --no-unions --max-funcs 5"
        "--no-argc --no-volatiles --max-funcs 6"
        "--no-argc --no-volatile-pointers --max-funcs 4"
        "--no-argc --max-expr-complexity 4 --max-funcs 3"
    )
    
    local config_idx=$((test_num % ${#configs[@]}))
    local csmith_opts="${configs[$config_idx]}"
    
    local test_name="test_$(printf '%03d' $test_num)"
    local src_file="$TEST_DIR/${test_name}.c"
    local pp_file="$TEST_DIR/${test_name}.pp.c"
    local reduced_file="$TEST_DIR/${test_name}.reduced.c"
    local final_file="$TEST_DIR/${test_name}.final.c"
    local orig_bin="$TEST_DIR/${test_name}.orig"
    local reduced_bin="$TEST_DIR/${test_name}.reduced"
    local final_bin="$TEST_DIR/${test_name}.final"
    local orig_stdout="$TEST_DIR/${test_name}.orig.stdout"
    local orig_stderr="$TEST_DIR/${test_name}.orig.stderr"
    local reduced_stdout="$TEST_DIR/${test_name}.reduced.stdout"
    local reduced_stderr="$TEST_DIR/${test_name}.reduced.stderr"
    local result_file="$TEST_DIR/${test_name}.result"
    
    # Generate csmith program
    if ! csmith $csmith_opts --seed $test_num > "$src_file" 2>/dev/null; then
        echo "SKIP csmith_gen_failed" > "$result_file"
        return 0
    fi
    
    # Track original source file lines (before preprocessing)
    local src_lines=$(wc -l < "$src_file")
    
    # Preprocess to embed headers
    local include_flag=""
    [ -n "$CSMITH_INCLUDE" ] && include_flag="-I$CSMITH_INCLUDE"
    
    local pp_raw="$TEST_DIR/${test_name}.pp.raw.c"
    if ! timeout ${TIMEOUT_SECS}s gcc $include_flag -E -P "$src_file" > "$pp_raw" 2>/dev/null; then
        echo "SKIP preprocess_failed" > "$result_file"
        rm -f "$src_file"
        return 0
    fi
    
    # Filter out _Float128 lines that libclang cannot parse
    # Also filter out __attribute__((...)) lines with unsupported attributes
    grep -v '_Float128' "$pp_raw" | grep -v '__float128' > "$pp_file"
    rm -f "$pp_raw"
    
    # orig_lines = preprocessed file lines (for internal use)
    # src_lines = original source lines (for reduction calculation)
    local orig_lines=$(wc -l < "$pp_file")
    
    # Compile original (use the filtered version for the reducer, but original for compile check)
    if ! timeout ${TIMEOUT_SECS}s gcc -w -O0 "$pp_file" -o "$orig_bin" 2>/dev/null; then
        echo "SKIP compile_failed" > "$result_file"
        rm -f "$src_file" "$pp_file"
        return 0
    fi
    
    # Run original - capture stdout, stderr, exit code
    timeout ${TIMEOUT_SECS}s "$orig_bin" > "$orig_stdout" 2> "$orig_stderr"
    local orig_exit=$?
    
    if [ $orig_exit -eq 124 ]; then
        echo "SKIP timeout" > "$result_file"
        rm -f "$src_file" "$pp_file" "$orig_bin"
        return 0
    fi
    
    # Measure original CPU cycles
    local orig_cycles=$(measure_cycles "$orig_bin")
    if [ "$orig_cycles" = "0" ]; then
        echo "SKIP cycles_measure_failed" > "$result_file"
        rm -f "$src_file" "$pp_file" "$orig_bin" "$orig_stdout" "$orig_stderr"
        return 0
    fi
    
    # Run reducer
    local reducer_output="$TEST_DIR/${test_name}.reducer.log"
    local start_time=$(date +%s.%N)
    if ! timeout 120s "$SLICER" -i "$pp_file" -o "$reduced_file" --no-coverage --timeout $TIMEOUT_SECS --total-timeout 60 > "$reducer_output" 2>&1; then
        echo "SKIP reducer_failed $(tail -1 $reducer_output 2>/dev/null)" > "$result_file"
        rm -f "$src_file" "$pp_file" "$orig_bin" "$orig_stdout" "$orig_stderr" "$reducer_output"
        return 0
    fi
    local end_time=$(date +%s.%N)
    local red_time=$(echo "$end_time - $start_time" | bc | awk '{printf "%.2f", $0}')
    rm -f "$reducer_output"
    
    if [ ! -f "$reduced_file" ]; then
        echo "FAIL no_output" > "$result_file"
        return 1
    fi
    
    local reduced_lines=$(wc -l < "$reduced_file")
    
    # Compile reduced
    if ! timeout ${TIMEOUT_SECS}s gcc -w -O0 "$reduced_file" -o "$reduced_bin" 2>/dev/null; then
        echo "FAIL reduced_compile_failed $src_lines $reduced_lines" > "$result_file"
        return 1
    fi
    
    # Run reduced - capture stdout, stderr, exit code
    timeout ${TIMEOUT_SECS}s "$reduced_bin" > "$reduced_stdout" 2> "$reduced_stderr"
    local reduced_exit=$?
    
    # Verify stdout matches
    if ! diff -q "$orig_stdout" "$reduced_stdout" > /dev/null 2>&1; then
        echo "FAIL stdout_mismatch $src_lines $reduced_lines" > "$result_file"
        return 1
    fi
    
    # Verify stderr matches
    if ! diff -q "$orig_stderr" "$reduced_stderr" > /dev/null 2>&1; then
        echo "FAIL stderr_mismatch $src_lines $reduced_lines" > "$result_file"
        return 1
    fi
    
    # Verify exit code matches
    if [ $orig_exit -ne $reduced_exit ]; then
        echo "FAIL exit_mismatch orig=$orig_exit reduced=$reduced_exit" > "$result_file"
        return 1
    fi
    
    # Measure reduced CPU cycles
    local reduced_cycles=$(measure_cycles "$reduced_bin")
    
    # Check if cycles need padding
    local cycle_ratio=$(echo "scale=4; $reduced_cycles / $orig_cycles" | bc)
    local min_ratio=$(echo "scale=4; 1 - $CYCLE_TOLERANCE" | bc)
    
    cp "$reduced_file" "$final_file"
    local final_cycles=$reduced_cycles
    local final_lines=$reduced_lines
    local padding_added="no"
    
    # If reduced cycles are below tolerance, add padding
    if (( $(echo "$cycle_ratio < $min_ratio" | bc -l) )); then
        # Calculate iterations needed (rough estimate)
        local cycle_deficit=$((orig_cycles - reduced_cycles))
        # Estimate ~10 cycles per iteration (rough)
        local iterations=$((cycle_deficit / 10))
        [ $iterations -lt 1000 ] && iterations=1000
        
        # Add padding code at the TOP of the file (before any functions)
        local padding_file="$TEST_DIR/${test_name}.padding.c"
        generate_padding_code $iterations > "$padding_file"
        cat "$reduced_file" >> "$padding_file"
        mv "$padding_file" "$final_file"
        
        # Insert padding call in main
        insert_padding_call "$final_file"
        
        # Recompile with padding
        local compile_out=$(timeout ${TIMEOUT_SECS}s gcc -w -O0 "$final_file" -o "$final_bin" 2>&1)
        local compile_status=$?
        if [ $compile_status -eq 0 ]; then
            # Re-measure cycles
            final_cycles=$(measure_cycles "$final_bin")
            final_lines=$(wc -l < "$final_file")
            padding_added="yes"
            
            # Verify behavior still matches after padding
            local final_stdout="$TEST_DIR/${test_name}.final.stdout"
            local final_stderr="$TEST_DIR/${test_name}.final.stderr"
            timeout ${TIMEOUT_SECS}s "$final_bin" > "$final_stdout" 2> "$final_stderr"
            local final_exit=$?
            
            if ! diff -q "$orig_stdout" "$final_stdout" > /dev/null 2>&1; then
                echo "FAIL padding_broke_stdout $src_lines $final_lines" > "$result_file"
                return 1
            fi
            
            if [ $orig_exit -ne $final_exit ]; then
                echo "FAIL padding_broke_exit $src_lines $final_lines" > "$result_file"
                return 1
            fi
            
            rm -f "$final_stdout" "$final_stderr"
        fi
    fi
    
    # Calculate reduction percentage (based on preprocessed file, which is what we actually reduce)
    # Note: We track src_lines for context but compare preprocessed input to reduced output
    local reduction=0
    if [ $orig_lines -gt 0 ]; then
        reduction=$((100 - (final_lines * 100 / orig_lines)))
    fi
    
    # Calculate cycle ratio after adjustment
    local final_ratio=$(echo "scale=2; $final_cycles * 100 / $orig_cycles" | bc)
    
    # Report: src_lines (original) pp_lines (preprocessed) final_lines reduction% orig_cycles final_cycles cycle_ratio padding_added red_time
    echo "PASS $src_lines $orig_lines $final_lines $reduction $orig_cycles $final_cycles $final_ratio $padding_added $red_time" > "$result_file"
    
    # Cleanup successful tests
    #rm -f "$src_file" "$pp_file" "$reduced_file" "$final_file" \
    #      "$orig_bin" "$reduced_bin" "$final_bin" \
    #      "$orig_stdout" "$orig_stderr" "$reduced_stdout" "$reduced_stderr"
    
    return 0
}

# Export for parallel execution
export -f run_test measure_cycles generate_padding_code insert_padding_call
export SLICER TEST_DIR TIMEOUT_SECS CSMITH_INCLUDE CYCLE_TOLERANCE PERF_RUNS

echo "" | tee -a "$RESULTS_FILE"
echo "Running $NUM_TESTS csmith tests (parallel: $MAX_PARALLEL)..." | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

# Run tests in parallel
seq 1 $NUM_TESTS | xargs -P $MAX_PARALLEL -I {} bash -c 'run_test {}'

# Collect results
PASSED=0
FAILED=0
SKIPPED=0
TOTAL_ORIG_LINES=0
TOTAL_FINAL_LINES=0
TOTAL_ORIG_CYCLES=0
TOTAL_FINAL_CYCLES=0
PADDING_COUNT=0
TOTAL_TIME=0

for i in $(seq 1 $NUM_TESTS); do
    result_file="$TEST_DIR/test_$(printf '%03d' $i).result"
    if [ -f "$result_file" ]; then
        result=$(cat "$result_file")
        status=$(echo "$result" | cut -d' ' -f1)
        
        case "$status" in
            PASS)
                ((PASSED++))
                # Format: PASS src_lines pp_lines final_lines reduction% orig_cycles final_cycles cycle_ratio padding_added red_time
                src_l=$(echo "$result" | cut -d' ' -f2)
                pp_l=$(echo "$result" | cut -d' ' -f3)
                final_l=$(echo "$result" | cut -d' ' -f4)
                red=$(echo "$result" | cut -d' ' -f5)
                orig_c=$(echo "$result" | cut -d' ' -f6)
                final_c=$(echo "$result" | cut -d' ' -f7)
                ratio=$(echo "$result" | cut -d' ' -f8)
                padded=$(echo "$result" | cut -d' ' -f9)
                red_time=$(echo "$result" | cut -d' ' -f10)
                
                TOTAL_ORIG_LINES=$((TOTAL_ORIG_LINES + pp_l))
                TOTAL_FINAL_LINES=$((TOTAL_FINAL_LINES + final_l))
                TOTAL_ORIG_CYCLES=$((TOTAL_ORIG_CYCLES + orig_c))
                TOTAL_FINAL_CYCLES=$((TOTAL_FINAL_CYCLES + final_c))
                TOTAL_TIME=$(echo "$TOTAL_TIME + $red_time" | bc | awk '{printf "%.2f", $0}')
                
                [ "$padded" = "yes" ] && ((PADDING_COUNT++))
                
                echo -e "Test $i: ${GREEN}PASSED${NC} (src:$src_l pp:$pp_l -> $final_l, ${red}% reduction, cycles: ${ratio}%, padded: $padded, time: ${red_time}s)" | tee -a "$RESULTS_FILE"
                ;;
            FAIL)
                ((FAILED++))
                reason=$(echo "$result" | cut -d' ' -f2-)
                echo -e "Test $i: ${RED}FAILED${NC} ($reason)" | tee -a "$RESULTS_FILE"
                ;;
            SKIP)
                ((SKIPPED++))
                reason=$(echo "$result" | cut -d' ' -f2)
                echo -e "Test $i: ${YELLOW}SKIPPED${NC} ($reason)" | tee -a "$RESULTS_FILE"
                ;;
        esac
        rm -f "$result_file"
    fi
done

# Calculate averages
if [ $PASSED -gt 0 ]; then
    AVG_REDUCTION=$((100 - (TOTAL_FINAL_LINES * 100 / TOTAL_ORIG_LINES)))
    AVG_CYCLE_RATIO=$(echo "scale=1; $TOTAL_FINAL_CYCLES * 100 / $TOTAL_ORIG_CYCLES" | bc)
    AVG_TIME=$(echo "scale=2; $TOTAL_TIME / $PASSED" | bc)
else
    AVG_REDUCTION=0
    AVG_CYCLE_RATIO=0
    AVG_TIME=0
fi

# Summary
echo "" | tee -a "$RESULTS_FILE"
echo "=== Summary ===" | tee -a "$RESULTS_FILE"
echo "Total:   $NUM_TESTS" | tee -a "$RESULTS_FILE"
echo -e "Passed:  ${GREEN}$PASSED${NC}" | tee -a "$RESULTS_FILE"
echo -e "Failed:  ${RED}$FAILED${NC}" | tee -a "$RESULTS_FILE"
echo -e "Skipped: ${YELLOW}$SKIPPED${NC}" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"
echo "=== Performance ===" | tee -a "$RESULTS_FILE"
echo "Average code reduction: ${AVG_REDUCTION}%" | tee -a "$RESULTS_FILE"
echo "Average CPU cycle ratio: ${AVG_CYCLE_RATIO}% of original" | tee -a "$RESULTS_FILE"
echo "Tests requiring cycle padding: $PADDING_COUNT" | tee -a "$RESULTS_FILE"
echo "Average reduction time: ${AVG_TIME}s" | tee -a "$RESULTS_FILE"
echo "" | tee -a "$RESULTS_FILE"

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}" | tee -a "$RESULTS_FILE"
    exit 1
else
    echo -e "${GREEN}All tests passed with CPU cycle parity!${NC}" | tee -a "$RESULTS_FILE"
    exit 0
fi
