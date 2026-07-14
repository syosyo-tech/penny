#!/bin/sh
set -eu

APP_NAME="penny"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
INSTALL_PATH="$INSTALL_DIR/$APP_NAME"

detect_repo() {
    if [ -n "${PENNY_REPO:-}" ]; then
        printf '%s\n' "$PENNY_REPO"
        return
    fi

    if ! command -v git >/dev/null 2>&1; then
        return 1
    fi

    remote_url=$(git -C "$PROJECT_DIR" remote get-url origin 2>/dev/null || true)

    case "$remote_url" in
        git@github.com:*)
            printf '%s\n' "$remote_url" | sed 's#git@github.com:##; s#\.git$##'
            ;;
        https://github.com/*)
            printf '%s\n' "$remote_url" | sed 's#https://github.com/##; s#\.git$##'
            ;;
        *)
            return 1
            ;;
    esac
}

detect_asset() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os:$arch" in
        Darwin:arm64 | Darwin:aarch64)
            asset_platform="macos-aarch64"
            ;;
        Darwin:x86_64 | Darwin:amd64)
            asset_platform="macos-x86_64"
            ;;
        Linux:x86_64 | Linux:amd64)
            asset_platform="linux-x86_64"
            ;;
        Linux:arm64 | Linux:aarch64)
            asset_platform="linux-aarch64"
            ;;
        *)
            echo "Unsupported platform: $os $arch" >&2
            exit 1
            ;;
    esac

    printf '%s-%s.tar.gz\n' "$APP_NAME" "$asset_platform"
}

download_file() {
    url=$1
    output=$2

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$output"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$output" "$url"
    else
        echo "curl または wget が必要です。" >&2
        exit 1
    fi
}

install_from_source() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo が見つかりません。Rust をインストールするか、GitHub Release版を使用してください。" >&2
        exit 1
    fi

    echo "Building $APP_NAME from source..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"

    mkdir -p "$INSTALL_DIR"
    cp "$PROJECT_DIR/target/release/$APP_NAME" "$INSTALL_PATH"
    chmod 755 "$INSTALL_PATH"
}

install_from_release() {
    repo=$(detect_repo) || {
        echo "GitHubリポジトリを特定できませんでした。" >&2
        echo "PENNY_REPO=owner/repo ./install.sh のように指定してください。" >&2
        exit 1
    }

    asset=$(detect_asset)

    if [ -n "${PENNY_VERSION:-}" ]; then
        download_url="https://github.com/$repo/releases/download/$PENNY_VERSION/$asset"
    else
        download_url="https://github.com/$repo/releases/latest/download/$asset"
    fi

    temp_dir=$(mktemp -d)
    trap 'rm -rf "$temp_dir"' EXIT INT TERM

    echo "Downloading $download_url..."
    download_file "$download_url" "$temp_dir/$asset"

    tar -xzf "$temp_dir/$asset" -C "$temp_dir"

    mkdir -p "$INSTALL_DIR"
    cp "$temp_dir/$APP_NAME" "$INSTALL_PATH"
    chmod 755 "$INSTALL_PATH"
}

if [ "${SOURCE_INSTALL:-0}" = "1" ]; then
    install_from_source
else
    install_from_release
fi

echo "Installed: $INSTALL_PATH"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "注意: $INSTALL_DIR は PATH に含まれていません。"
        echo "次をシェル設定に追加してください: export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac
