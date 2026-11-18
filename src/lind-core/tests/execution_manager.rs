use lind_core::LindCageManager;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

#[test]
fn execution_waits_for_the_last_cage() {
    let manager = Arc::new(LindCageManager::new(1));
    manager.increment();

    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let waiter_manager = Arc::clone(&manager);
    let waiter = thread::spawn(move || {
        started_tx.send(()).unwrap();
        waiter_manager.wait();
        done_tx.send(()).unwrap();
    });

    started_rx.recv().unwrap();
    manager.decrement();
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    manager.decrement();
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    waiter.join().unwrap();
}
