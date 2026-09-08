// リリースで Windows の余分なコンソール窓を出さない (DO NOT REMOVE)。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // .env はアプリ入口で読む (lib は環境を書き換えない)。開発時はリポジトリ root の .env を拾う。
    dotenvy::dotenv().ok();
    apppromo_desktop_lib::run()
}
