// 有窗口的程序：不要额外开控制台
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clackdx_lib::run()
}
