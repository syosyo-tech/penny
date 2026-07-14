# PENNY

LAN内で使うターミナルチャットです。

## Install

GitHub Releaseからビルド済みバイナリを取得するため、利用者側にRust環境は不要です。

```sh
./install.sh
```

デフォルトでは`$HOME/.local/bin/penny`にインストールします。別の場所に入れる場合は`INSTALL_DIR`を指定します。

```sh
INSTALL_DIR=/usr/local/bin ./install.sh
```

`$HOME/.local/bin`が`PATH`に入っていない場合は、シェル設定に追加してください。

```sh
export PATH="$HOME/.local/bin:$PATH"
```

## Uninstall

```sh
./uninstall.sh
```

## Development Install

開発中に手元でビルドしてインストールする場合だけ、Rust/Cargoが必要です。

```sh
SOURCE_INSTALL=1 ./install.sh
```

## Release

タグをpushするとGitHub ActionsがmacOS/Linux向けのリリースバイナリを作成します。
対象はmacOS arm64、macOS Intel、Linux x86_64、Linux arm64です。

```sh
git tag v0.1.0
git push origin v0.1.0
```
