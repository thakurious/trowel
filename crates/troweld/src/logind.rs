//! F7 PrepareForSleep without zbus: `gdbus monitor` plus sysfs resume poll.
//! Tests inject overflow (SIGUSR1) and sleep toggle (SIGUSR2).

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static OVERFLOW_PTR: AtomicUsize = AtomicUsize::new(0);
static SLEEP_PTR: AtomicUsize = AtomicUsize::new(0);

extern "C" fn on_usr1(_: i32) {
    let p = OVERFLOW_PTR.load(Ordering::SeqCst);
    if p != 0 {
        unsafe { &*(p as *const AtomicBool) }.store(true, Ordering::SeqCst);
    }
}

extern "C" fn on_usr2(_: i32) {
    let p = SLEEP_PTR.load(Ordering::SeqCst);
    if p != 0 {
        let flag = unsafe { &*(p as *const AtomicBool) };
        let next = !flag.load(Ordering::SeqCst);
        flag.store(next, Ordering::SeqCst);
    }
}

pub fn bind_signals(overflow: &Arc<AtomicBool>, sleeping: &Arc<AtomicBool>) {
    OVERFLOW_PTR.store(Arc::as_ptr(overflow) as usize, Ordering::SeqCst);
    SLEEP_PTR.store(Arc::as_ptr(sleeping) as usize, Ordering::SeqCst);
    unsafe {
        libc::signal(libc::SIGUSR1, on_usr1 as *const () as usize);
        libc::signal(libc::SIGUSR2, on_usr2 as *const () as usize);
    }
}

/// `on_sleep(true)` before sleep, `on_sleep(false)` after resume.
pub fn start(on_sleep: impl Fn(bool) + Send + Sync + 'static) {
    let on_sleep = Arc::new(on_sleep);
    let gdbus = Arc::clone(&on_sleep);
    thread::spawn(move || {
        let Ok(mut child) = Command::new("gdbus")
            .args(["monitor", "--system", "--dest", "org.freedesktop.login1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if !line.contains("PrepareForSleep") {
                continue;
            }
            let going = line.contains("true") || line.contains("True");
            gdbus(going);
        }
    });
    let sys = Arc::clone(&on_sleep);
    thread::spawn(move || {
        let path = "/sys/power/suspend_stats/success";
        let mut last = std::fs::read_to_string(path).ok();
        loop {
            thread::sleep(Duration::from_secs(15));
            let now = std::fs::read_to_string(path).ok();
            if now != last && last.is_some() {
                sys(false);
            }
            last = now;
        }
    });
}
