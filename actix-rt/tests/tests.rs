use std::{
    sync::mpsc::channel,
    thread,
    time::{Duration, Instant},
};

use actix_rt::{System, Worker};
use tokio::sync::oneshot;

#[test]
fn await_for_timer() {
    let time = Duration::from_secs(1);
    let instant = Instant::now();
    System::new().block_on(async move {
        tokio::time::sleep(time).await;
    });
    assert!(
        instant.elapsed() >= time,
        "Block on should poll awaited future to completion"
    );
}

#[test]
fn join_another_worker() {
    let time = Duration::from_secs(1);
    let instant = Instant::now();
    System::new().block_on(async move {
        let worker = Worker::new();
        worker.spawn(Box::pin(async move {
            tokio::time::sleep(time).await;
            Worker::handle().stop();
        }));
        worker.join().unwrap();
    });
    assert!(
        instant.elapsed() >= time,
        "Join on another worker should complete only when it calls stop"
    );

    let instant = Instant::now();
    System::new().block_on(async move {
        let worker = Worker::new();
        worker.spawn_fn(move || {
            actix_rt::spawn(async move {
                tokio::time::sleep(time).await;
                Worker::handle().stop();
            });
        });
        worker.join().unwrap();
    });
    assert!(
        instant.elapsed() >= time,
        "Join on a worker that has used actix_rt::spawn should wait for said future"
    );

    let instant = Instant::now();
    System::new().block_on(async move {
        let worker = Worker::new();
        worker.spawn(Box::pin(async move {
            tokio::time::sleep(time).await;
            Worker::handle().stop();
        }));
        worker.stop();
        worker.join().unwrap();
    });
    assert!(
        instant.elapsed() < time,
        "Premature stop of worker should conclude regardless of it's current state"
    );
}

#[test]
fn non_static_block_on() {
    let string = String::from("test_str");
    let str = string.as_str();

    let sys = System::new();

    sys.block_on(async {
        actix_rt::time::sleep(Duration::from_millis(1)).await;
        assert_eq!("test_str", str);
    });

    let rt = actix_rt::Runtime::new().unwrap();

    rt.block_on(async {
        actix_rt::time::sleep(Duration::from_millis(1)).await;
        assert_eq!("test_str", str);
    });

    System::with_init(async {
        assert_eq!("test_str", str);
        System::current().stop();
    })
    .run()
    .unwrap();
}

#[test]
fn wait_for_spawns() {
    let rt = actix_rt::Runtime::new().unwrap();

    let handle = rt.spawn(async {
        println!("running on the runtime");
        // assertion panic is caught at task boundary
        assert_eq!(1, 2);
    });

    assert!(rt.block_on(handle).is_err());
}

#[test]
fn worker_spawn_fn_runs() {
    let _ = System::new();

    let (tx, rx) = channel::<u32>();

    let worker = Worker::new();
    worker.spawn_fn(move || tx.send(42).unwrap());

    let num = rx.recv().unwrap();
    assert_eq!(num, 42);

    worker.stop();
    worker.join().unwrap();
}

#[test]
fn worker_drop_no_panic_fn() {
    let _ = System::new();

    let worker = Worker::new();
    worker.spawn_fn(|| panic!("test"));

    worker.stop();
    worker.join().unwrap();
}

#[test]
fn worker_drop_no_panic_fut() {
    let _ = System::new();

    let worker = Worker::new();
    worker.spawn(async { panic!("test") });

    worker.stop();
    worker.join().unwrap();
}

#[test]
fn worker_item_storage() {
    let _ = System::new();

    let worker = Worker::new();

    assert!(!Worker::contains_item::<u32>());
    Worker::set_item(42u32);
    assert!(Worker::contains_item::<u32>());

    Worker::get_item(|&item: &u32| assert_eq!(item, 42));
    Worker::get_mut_item(|&mut item: &mut u32| assert_eq!(item, 42));

    let thread = thread::spawn(move || {
        Worker::get_item(|&_item: &u32| unreachable!("u32 not in this thread"));
    })
    .join();
    assert!(thread.is_err());

    let thread = thread::spawn(move || {
        Worker::get_mut_item(|&mut _item: &mut i8| unreachable!("i8 not in this thread"));
    })
    .join();
    assert!(thread.is_err());

    worker.stop();
    worker.join().unwrap();
}

#[test]
#[should_panic]
fn no_system_current_panic() {
    System::current();
}

#[test]
#[should_panic]
fn no_system_worker_new_panic() {
    Worker::new();
}

#[test]
fn system_worker_spawn() {
    let runner = System::new();

    let (tx, rx) = oneshot::channel();
    let sys = System::current();

    thread::spawn(|| {
        // this thread will have no worker in it's thread local so call will panic
        Worker::handle();
    })
    .join()
    .unwrap_err();

    let thread = thread::spawn(|| {
        // this thread will have no worker in it's thread local so use the system handle instead
        System::set_current(sys);
        let sys = System::current();

        let wrk = sys.worker();
        wrk.spawn(async move {
            tx.send(42u32).unwrap();
            System::current().stop();
        });
    });

    assert_eq!(runner.block_on(rx).unwrap(), 42);
    thread.join().unwrap();
}
