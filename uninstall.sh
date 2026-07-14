#!/bin/sh
set -eu

APP_NAME="penny"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
INSTALL_PATH="$INSTALL_DIR/$APP_NAME"

if [ -e "$INSTALL_PATH" ]; then
    rm -f "$INSTALL_PATH"
    echo "Removed: $INSTALL_PATH"
else
    echo "Not installed: $INSTALL_PATH"
fi
