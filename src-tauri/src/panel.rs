//! macOS：把提醒窗口抬到浮动层之上，并允许它进入全屏空间。
//! `NSWindow.setLevel` 与 `NSWindowCollectionBehavior` 都是公开 API。
//! 独占全屏（尤其是游戏）仍然可能盖住它，这是系统限制。

use objc::sel_impl;
use tauri::WebviewWindow;

const POPUP_MENU_LEVEL: i64 = 101;
const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
const STATIONARY: u64 = 1 << 4;
const IGNORES_CYCLE: u64 = 1 << 6;
const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;

pub fn raise_above_fullscreen(window: &WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    if ptr.is_null() {
        return;
    }
    unsafe {
        let ns_window = ptr as *mut objc::runtime::Object;
        let _: () = objc::msg_send![ns_window, setLevel: POPUP_MENU_LEVEL];
        let current: u64 = objc::msg_send![ns_window, collectionBehavior];
        let behavior =
            current | CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
        let _: () = objc::msg_send![ns_window, setCollectionBehavior: behavior];
    }
}
