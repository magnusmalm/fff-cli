#!/usr/bin/env bash
# fff-cli benchmarks vs ripgrep and fzf
# Usage: ./bench.sh [repo_path]
# Requires: hyperfine, rg, fzf, git
set -euo pipefail

FFF="$(cd "$(dirname "$0")" && pwd)/target/release/fff"
REPO="${1:-$HOME/devel/buildroot}"
REPO_NAME="$(basename "$REPO")"
FILE_COUNT="$(git -C "$REPO" ls-files 2>/dev/null | wc -l)"
FILELIST="/tmp/fff-bench-filelist.txt"
GREP_COMMON="${GREP_COMMON:-CONFIG_}"
GREP_RARE="${GREP_RARE:-wpa_supplicant}"

if [ ! -x "$FFF" ]; then
    echo "Release binary not found. Run: cargo build --release"
    exit 1
fi

echo "=== fff-cli benchmarks ==="
echo "Repo: $REPO ($FILE_COUNT files)"
echo "fff:  $($FFF --version)"
echo "rg:   $(rg --version | head -1)"
echo "fzf:  $(fzf --version 2>&1)"
echo ""

# ── Prep ────────────────────────────────────────────────────────────────
echo "--- Prep ---"
rm -rf "$REPO/.fff"
$FFF index -C "$REPO" --force 2>&1
# Pre-generate file list for fair fzf comparison (no git ls-files overhead)
git -C "$REPO" ls-files > "$FILELIST"
echo "File list cached to $FILELIST"
echo ""

# ════════════════════════════════════════════════════════════════════════
# SECTION 1: Standard benchmarks (include all real-world overhead)
# ════════════════════════════════════════════════════════════════════════
echo "========================================"
echo "  STANDARD (real-world invocation cost)"
echo "========================================"
echo ""

# ── 1a. Fuzzy file search ──────────────────────────────────────────────
echo "--- Fuzzy search: 'Makefile' ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-search.md \
    -n "fff search"              "$FFF search Makefile -C $REPO -n 20" \
    -n "fzf (git ls-files pipe)" "git -C $REPO ls-files | fzf --filter='Makefile' | head -20" \
    -n "find"                    "find $REPO -name '*Makefile*' -type f 2>/dev/null | head -20"
echo ""

# ── 1b. Fuzzy search with typo ────────────────────────────────────────
echo "--- Fuzzy search (typo): 'Makeifle' ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-typo.md \
    -n "fff search"              "$FFF search Makeifle -C $REPO -n 20" \
    -n "fzf (git ls-files pipe)" "git -C $REPO ls-files | fzf --filter='Makeifle' | head -20"
echo ""

# ── 1c. Grep: common pattern ──────────────────────────────────────────
echo "--- Grep (common): '$GREP_COMMON' ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-grep.md \
    -n "fff grep" "$FFF grep $GREP_COMMON -C $REPO -n 50" \
    -n "rg"       "rg --no-heading -m 50 $GREP_COMMON $REPO"
echo ""

# ── 1d. Grep: rare pattern ────────────────────────────────────────────
echo "--- Grep (rare): '$GREP_RARE' ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-grep-rare.md \
    -n "fff grep" "$FFF grep $GREP_RARE -C $REPO -n 50" \
    -n "rg"       "rg --no-heading -m 50 $GREP_RARE $REPO"
echo ""

# ════════════════════════════════════════════════════════════════════════
# SECTION 2: Apples-to-apples (remove pipeline / shell overhead)
# ════════════════════════════════════════════════════════════════════════
echo "========================================"
echo "  APPLES-TO-APPLES (no shell overhead)"
echo "========================================"
echo ""

# ── 2a. Fuzzy search: fff (shell=none) vs fzf reading from file ───────
echo "--- Fuzzy search (no shell overhead): 'Makefile' ---"
hyperfine \
    --warmup 3 -i -N \
    --export-markdown /tmp/fff-bench-search-fair.md \
    -n "fff search (no shell)" "$FFF search Makefile -C $REPO -n 20" \
    -n "fzf (from file)"       "fzf --filter='Makefile' < $FILELIST | head -20"
echo ""

# ── 2b. Fuzzy search with typo (no shell overhead) ────────────────────
echo "--- Fuzzy search typo (no shell overhead): 'Makeifle' ---"
hyperfine \
    --warmup 3 -i -N \
    --export-markdown /tmp/fff-bench-typo-fair.md \
    -n "fff search (no shell)" "$FFF search Makeifle -C $REPO -n 20" \
    -n "fzf (from file)"       "fzf --filter='Makeifle' < $FILELIST | head -20"
echo ""

# ── 2c. Grep (no shell overhead) ──────────────────────────────────────
echo "--- Grep (no shell overhead): '$GREP_COMMON' ---"
hyperfine \
    --warmup 3 -i -N \
    --export-markdown /tmp/fff-bench-grep-fair.md \
    -n "fff grep (no shell)" "$FFF grep $GREP_COMMON -C $REPO -n 50" \
    -n "rg (no shell)"       "rg --no-heading -m 50 $GREP_COMMON $REPO"
echo ""

# ════════════════════════════════════════════════════════════════════════
# SECTION 2b: Filter mode — direct fzf competitor (no index needed)
# ════════════════════════════════════════════════════════════════════════
echo "========================================"
echo "  FILTER MODE (fff --filter vs fzf)"
echo "========================================"
echo ""

echo "--- Filter: 'Makefile' (from cached file list, no shell) ---"
hyperfine \
    --warmup 3 -i -N \
    --export-markdown /tmp/fff-bench-filter.md \
    -n "fff --filter" "$FFF --filter Makefile -n 20 < $FILELIST" \
    -n "fzf --filter" "fzf --filter='Makefile' < $FILELIST | head -20"
echo ""

echo "--- Filter: 'Makeifle' (typo, from cached file list, no shell) ---"
hyperfine \
    --warmup 3 -i -N \
    --export-markdown /tmp/fff-bench-filter-typo.md \
    -n "fff --filter" "$FFF --filter Makeifle -n 20 < $FILELIST" \
    -n "fzf --filter" "fzf --filter='Makeifle' < $FILELIST | head -20"
echo ""

echo "--- Filter: full pipeline (git ls-files | filter) ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-filter-pipe.md \
    -n "fff --filter" "git -C $REPO ls-files | $FFF --filter Makefile -n 20" \
    -n "fzf --filter" "git -C $REPO ls-files | fzf --filter='Makefile' | head -20"
echo ""

# ════════════════════════════════════════════════════════════════════════
# SECTION 3: Hot cache (page cache fully warm, rapid repeated queries)
# ════════════════════════════════════════════════════════════════════════
echo "========================================"
echo "  HOT CACHE (repeated queries)"
echo "========================================"
echo ""

# ── 3a. 10 sequential searches (simulates interactive typing) ─────────
# Write helper scripts — hyperfine -N can't run shell loops directly.
cat > /tmp/fff-bench-burst-fff.sh << INNEREOF
#!/usr/bin/env bash
for q in M Ma Mak Make Makef Makefi Makefil Makefile Make.in Makefile.in; do
    $FFF search "\$q" -C $REPO -n 10 > /dev/null 2>&1
done
INNEREOF
cat > /tmp/fff-bench-burst-fzf.sh << INNEREOF
#!/usr/bin/env bash
for q in M Ma Mak Make Makef Makefi Makefil Makefile Make.in Makefile.in; do
    fzf --filter="\$q" < $FILELIST | head -10 > /dev/null 2>&1
done
INNEREOF
chmod +x /tmp/fff-bench-burst-fff.sh /tmp/fff-bench-burst-fzf.sh

echo "--- 10 sequential fuzzy searches ---"
hyperfine \
    --warmup 3 -i \
    --export-markdown /tmp/fff-bench-burst.md \
    -n "fff x10" "/tmp/fff-bench-burst-fff.sh" \
    -n "fzf x10" "/tmp/fff-bench-burst-fzf.sh"
echo ""

# ── 3b. Grep after warm ───────────────────────────────────────────────
# Pre-warm: read all files into page cache
echo "--- Grep (page cache warm): '$GREP_COMMON' ---"
echo "(pre-warming page cache...)"
rg --no-heading "$GREP_COMMON" "$REPO" > /dev/null 2>&1 || true
$FFF grep "$GREP_COMMON" -C "$REPO" -n 50 > /dev/null 2>&1 || true
hyperfine \
    --warmup 5 -i -N \
    --export-markdown /tmp/fff-bench-grep-warm.md \
    -n "fff grep (warm)" "$FFF grep $GREP_COMMON -C $REPO -n 50" \
    -n "rg (warm)"       "rg --no-heading -m 50 $GREP_COMMON $REPO"
echo ""

# ════════════════════════════════════════════════════════════════════════
# SECTION 4: Cold start costs
# ════════════════════════════════════════════════════════════════════════
echo "========================================"
echo "  COLD START"
echo "========================================"
echo ""

# ── 4a. Index build ───────────────────────────────────────────────────
echo "--- Index build (cold) ---"
hyperfine \
    --prepare "rm -rf $REPO/.fff; sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null 2>&1 || true" \
    --export-markdown /tmp/fff-bench-index.md \
    -n "fff index (cold)" "$FFF index -C $REPO" 2>&1
echo ""

# ── 4b. First search (auto-index + search) ───────────────────────────
echo "--- First search (auto-index): 'Makefile' ---"
hyperfine \
    --prepare "rm -rf $REPO/.fff" -i \
    --export-markdown /tmp/fff-bench-first.md \
    -n "fff first search" "$FFF search Makefile -C $REPO -n 20" \
    -n "rg (cold)"        "rg --no-heading -m 20 Makefile $REPO" \
    -n "fzf (cold)"       "git -C $REPO ls-files | fzf --filter='Makefile' | head -20"
echo ""

# ── Summary ────────────────────────────────────────────────────────────
echo "========================================"
echo "  RESULTS"
echo "========================================"
for f in /tmp/fff-bench-*.md; do
    echo ""
    echo "### $(basename "$f" .md | sed 's/-/ /g')"
    cat "$f"
done

# Cleanup
rm -rf "$REPO/.fff" "$FILELIST"
