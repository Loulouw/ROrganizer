use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG, WH_KEYBOARD_LL,
    WH_MOUSE_LL,
};

pub fn spawn() {
    std::thread::Builder::new()
        .name("rorg-hooks".into())
        .spawn(run)
        .expect("hooks thread");
}

fn run() {
    unsafe {
        let _kb = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(super::keyboard::kb_proc),
            None,
            0,
        )
        .expect("install kb hook");
        let _mse = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(super::mouse::mouse_proc),
            None,
            0,
        )
        .expect("install mouse hook");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
