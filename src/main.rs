extern crate graceful;
extern crate sysfs_class;

use graceful::SignalGuard;
use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use std::thread;
use std::time::Duration;

use fan::FanDaemon;
use std::process;

mod fan;

static STOP: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    let signal_guard = SignalGuard::new();

    if unsafe { libc::geteuid() } != 0 {
        eprintln!("must be run as root");
        process::exit(1);
    }

    let handle = thread::spawn(|| {
        let refresh_time = Duration::from_millis(100);
        let mut fan_daemon_res = FanDaemon::new();
        if let Err(ref err) = fan_daemon_res {
            eprintln!("fan daemon: {}", err);
        }
        while !STOP.load(Ordering::Acquire) {
            if let Ok(ref mut fan_daemon) = fan_daemon_res {
                if !fan_daemon.step() {
                    eprintln!("Failed to step.");
                    STOP.store(true, Ordering::Release);
                    break;
                }
            }
            thread::sleep(refresh_time);
        }

        fan_daemon_res.unwrap();
        println!("Bye.");
        process::exit(1);
    });
    signal_guard.at_exit(move |sig| {
        println!("Signal {} received.", sig);
        STOP.store(true, Ordering::Release);
        handle.join().unwrap();
    });
}
