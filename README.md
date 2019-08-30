# AdEx Explorer

**To get started:**
- Clone this repo: `git clone https://github.com/AdExNetwork/adex-explorer.git`

- If you don't have Rust and cargo-make installed, [Download it](https://www.rust-lang.org/tools/install), and run the following commands:

`rustup update`

`rustup target add wasm32-unknown-unknown`

`cargo install --force cargo-make`

`curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`

Run `cargo make all` or `cargo make watch` in a terminal to build the app, and `cargo make serve` to start a dev server
on `127.0.0.0:8000`.

## publish to github pages

```
git checkout gh-pages
git merge master
cargo make create_wasm_release
wasm-opt -Os -o pkg/package_bg.wasm pkg/package_bg.wasm
git commit -am 'new release'
git push
```
