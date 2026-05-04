#!/bin/sh
# Sourceable environment configuration for Toolbridge
# Usage: . scripts/env.sh  (from project root)
#    or: PROJECT_ROOT=/path/to/project . scripts/env.sh

# Project root: use override, or assume current directory
HYPOMNEMA_ROOT="${PROJECT_ROOT:-$(pwd)}"
export HYPOMNEMA_ROOT

export UV_PYTHON="python3.12"
export UV_PYTHON_PREFERENCE="only-system"
export UV_PROJECT_ENVIRONMENT="$HYPOMNEMA_ROOT/.direnv/.venv"
export UV_CACHE_DIR="$HYPOMNEMA_ROOT/.direnv/.cache/uv"

export PYTHONDONTWRITEBYTECODE=1
export PYTHONHASHSEED=0
