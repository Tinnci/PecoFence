#!/usr/bin/env bash
# Run a prompt file through Codex from ~/dev: run-agent.sh [--dry-run] [--model M] [--effort E] <prompt-file>.
set -euo pipefail

dry_run=false
model=gpt-6-sol
effort=low

usage() {
  echo 'Usage: run-agent.sh [--dry-run] [--model M] [--effort E] <prompt-file>' >&2
  exit 2
}

while (($#)); do
  case "$1" in
    --dry-run) dry_run=true; shift ;;
    --model) (($# >= 2)) || usage; model=$2; shift 2 ;;
    --effort) (($# >= 2)) || usage; effort=$2; shift 2 ;;
    --) shift; break ;;
    -*) usage ;;
    *) break ;;
  esac
done

(($# == 1)) || usage
prompt_file=$1
[[ -r $prompt_file ]] || { echo "Cannot read prompt file: $prompt_file" >&2; exit 1; }
prompt_file=$(realpath -- "$prompt_file")
command=(codex exec -s workspace-write --skip-git-repo-check -m "$model" -c "model_reasoning_effort=\"$effort\"" -C "$HOME/dev" -)

if $dry_run; then
  printf '%q ' "${command[@]}"
  printf '< %q\n' "$prompt_file"
else
  "${command[@]}" < "$prompt_file"
fi
