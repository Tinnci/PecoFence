#!/usr/bin/env bash
set -euo pipefail

PROMPT_FILE="/home/enterp/.gemini/antigravity-cli/brain/3600ed40-52cd-4da4-b479-57faafd31f67/scratch/sol_consideration_prompt.txt"

echo "[$(date -Iseconds)] Invoking GPT-5.6 Sol (medium effort) via Codex CLI..."
codex exec --skip-git-repo-check -m gpt-5.6-sol -c model_reasoning_effort="medium" --sandbox danger-full-access < "$PROMPT_FILE"
