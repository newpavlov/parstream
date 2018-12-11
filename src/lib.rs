//! Crate for computing function over iterator in a streaming fashion
//! using multi-threading while preserving order.
//!
//! # Examples
//! ```
//! let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
//! let mut ys = Vec::new();
//! let f = |x| x*x;
//! let res: Result<usize, ()> = parstream::run(
//!     xs, 4,
//!     // closure which is called for every x in xs
//!     |x| {
//!         std::thread::sleep(std::time::Duration::from_millis(*x));
//!         Ok(f(x))
//!     },
//!     // closure which is called for every result with preserved order
//!     |y| {
//!         ys.push(y);
//!         Ok(())
//!     },
//! );
//!
//! assert_eq!(res, Ok(xs.len()));
//! assert_eq!(ys, xs.iter().map(f).collect::<Vec<_>>());
//! ```
//!
//! If one of callbacks will return error, no new tasks will be started and
//! `run` will end as soon as possible (after threads cleanup) to report this
//! error to caller.
//! ```
//! #[derive(Eq, PartialEq, Debug)]
//! struct MyError(usize);
//!
//! let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
//! let mut ys = Vec::new();
//! let f = |x| x*x;
//! let res = parstream::run(xs.iter().enumerate(), 4,
//!     |(i, x)| {
//!         std::thread::sleep(std::time::Duration::from_millis(*x));
//!         if *x == 0 { return Err(MyError(i)); }
//!         Ok(f(x))
//!     },
//!     |y| {
//!         ys.push(y);
//!         Ok(())
//!     },
//! );
//!
//! assert_eq!(res, Err(MyError(5)));
//! assert_eq!(ys.len(), 0);
//! ```
//!
//! # Warnings
//! The first closure in `run` should not panic as it will lead to a deadlock!
//! Also report thread will not recover after the second closure panic, it will
//! not result in deadlock, but the second closure will not be called anymore.
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;
use std::cmp;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering;

use crossbeam_channel as channel;
use crossbeam_utils::thread as cb_thread;

struct State<T> {
    pos: usize,
    payload: T,
}

impl<T> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

impl<T> Eq for State<T> { }

impl<T> Ord for State<T> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        other.pos.cmp(&self.pos)
    }
}

impl<T> PartialOrd for State<T> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

enum ReportMsg<T, E> {
    None,
    NewResult((usize, Result<T, E>)),
}

fn run_report<T, E>(
    rx: channel::Receiver<ReportMsg<T, E>>,
    mut f: impl FnMut(T) -> Result<(), E>,
    flag: &AtomicIsize,
) -> Result<(), E> {
    let mut buf: BinaryHeap<State<T>> = BinaryHeap::new();
    let mut n = 0;

    use self::ReportMsg::*;
    for val in rx.iter() {
        let target = flag.load(Ordering::Acquire);
        if target < 0 { break }

        match val {
            NewResult((i, payload)) => {
                let payload = payload.map_err(Into::into)?;
                if i != n {
                    buf.push(State { pos: i, payload: payload });
                    continue;
                }
                f(payload).map_err(Into::into)?;

                n += 1;
                while let Some(pm) = buf.peek_mut() {
                    assert!(pm.pos >= n);
                    if pm.pos != n { break }
                    f(PeekMut::pop(pm).payload).map_err(Into::into)?;
                    n += 1
                }
            },
            None => (),
        }

        if target as usize == n { break; }
    }
    Ok(())
}

const FLAG_INIT: isize = 0;
const FLAG_ERROR: isize = -1;
const FLAG_WORKER_PANIC: isize = -2;
const FLAG_REPORT_PANIC: isize = -3;

/// Compute `f(x)` for every `x` in `xs` using thread pool and call `report`
/// for every result and preserve order of elements.
///
/// Retutns either number of elements successfully processed or first
/// enocuntered error.
///
/// Number of threads in the workers pool will be equal to `threads`.
pub fn run<X: Send, Y: Send, E: Send>(
    xs: impl IntoIterator<Item=X>,
    threads: usize,
    f: impl Fn(X) -> Result<Y, E> + Sync,
    report: impl FnMut(Y) -> Result<(), E> + Send
) -> Result<usize, E> {
    let (tx, rx) = channel::bounded(2*threads);
    let (tx2, rx2) = channel::bounded(2*threads);
    // FLAG_INIT = 0 represents default value
    // FLAG_INIT > 0 represents number of elements in the non-empty iterator
    // FLAG_INIT < 0 rуpresents error or panics which have happened in threads
    let flag = &AtomicIsize::new(FLAG_INIT);
    let mut result = Ok(0);

    cb_thread::scope(|scope| {
        for _ in 0..threads {
            let rxc = rx.clone();
            let txc = tx2.clone();
            let fp = &f;
            scope.spawn(move |_| {
                for x in rxc.iter() {
                    if flag.load(Ordering::Acquire) < 0 { break }

                    match x {
                        Some((i, x)) => {
                            let res = (i, fp(x)) ;
                            let r = txc.send(ReportMsg::NewResult(res));
                            if r.is_err() { break; }
                        },
                        None => break,
                    }
                }
            });
        }

        let res = &mut result;
        scope.spawn(move |_| {
            if let Err(err) = run_report(rx2, report, flag) {
                flag.store(FLAG_ERROR, Ordering::Release);
                *res = Err(err);
            }
        });

        let mut n = 0;
        for val in xs.into_iter().enumerate() {
            if flag.load(Ordering::Acquire) < 0 { break }
            n += 1;
            tx.send(Some(val)).unwrap();
        }

        if flag.load(Ordering::Acquire) >= 0 {
            flag.store(n as isize, Ordering::Release);
            tx2.send(ReportMsg::None).unwrap();
        } else {
            // clear all messages in the channel if there is an error or panic
            while let Ok(_) = rx.try_recv() {}
        }

        for _ in 0..threads {
            tx.send(None).unwrap();
        }
    }).unwrap();

    match flag.load(Ordering::Acquire) {
        n if n >= 0 => { Ok(n as usize) },
        FLAG_ERROR => result,
        FLAG_WORKER_PANIC => panic!("worker thread has panicked"),
        FLAG_REPORT_PANIC => panic!("report thread has panicked"),
        _ => unreachable!(),
    }
}
