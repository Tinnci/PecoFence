#!/usr/bin/env bash
set -euo pipefail

PECOFENCE_DIR="/mnt/c/Users/Administrator/PecoFence"
PROMPT_FILE="/home/enterp/.gemini/antigravity-cli/brain/3600ed40-52cd-4da4-b479-57faafd31f67/scratch/sol_prompt.txt"

echo "[$(date -Iseconds)] Invoking GPT-5.6 Sol via Codex CLI..."
codex exec --skip-git-repo-check -m gpt-5.6-sol --sandbox danger-full-access < "$PROMPT_FILE"
