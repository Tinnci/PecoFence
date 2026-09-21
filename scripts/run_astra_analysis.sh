#!/usr/bin/env bash
set -euo pipefail

PROMPT_FILE="/home/enterp/.gemini/antigravity-cli/brain/3600ed40-52cd-4da4-b479-57faafd31f67/scratch/astra_architecture_analysis_prompt.txt"

echo "[$(date -Iseconds)] Invoking GPT-6 Astra via Codex CLI for architectural analysis and extensibility optimization..."
codex exec --skip-git-repo-check -m gpt-6-astra --sandbox danger-full-access < "$PROMPT_FILE"
