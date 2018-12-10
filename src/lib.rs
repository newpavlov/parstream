//! Compute function over iterator in a streaming fashion using thread pool and
//! preserving order of elements.
//!
//! # Example
//! ```
//! let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
//! let mut ys = Vec::new();
//! let f = |x| x*x;
//! parstream::run(xs, 4,
//!     |x| {
//!         std::thread::sleep(std::time::Duration::from_millis(*x));
//!         f(x)
//!     },
//!     |y| ys.push(y),
//! );
//! assert_eq!(ys, xs.iter().map(f).collect::<Vec<_>>());
//! ```
//!
//! # Warning
//! First closure in `run` should not panic!
use std::collections::BinaryHeap;
use std::cmp::Ordering;

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
    fn cmp(&self, other: &Self) -> Ordering {
        other.pos.cmp(&self.pos)
    }
}

impl<T> PartialOrd for State<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

enum ReportMsg<T> {
    SetTarget(usize),
    NewResult((usize, T)),
}

fn run_report<T>(
    rx: channel::Receiver<ReportMsg<T>>,
    mut f: impl FnMut(T)
) {
    let mut buf: BinaryHeap<State<T>> = BinaryHeap::new();
    let mut n = 0;
    let mut target = None;

    use self::ReportMsg::*;
    for val in rx.iter() {
        match val {
            NewResult((i, payload)) => {
                if i != n {
                    buf.push(State { pos: i, payload: payload });
                    continue;
                }
                f(payload);
                n += 1;
                while let Some(&State { pos, .. }) = buf.peek() {
                    assert!(pos >= n);
                    if pos != n { break }
                    f(buf.pop().unwrap().payload);
                    n += 1
                }
                if let Some(t) = target {
                    if t + 1 == n { break; }
                }
            },
            SetTarget(t) => {
                target = Some(t);
                if n == t { break; }
            }
        }
    }
}

/// Compute `f(x)` for every `x` in `xs` using thread pool and call `report` for
/// every result and preserve order of elements.
///
/// Number of threads in the pool will be equal to `threads`.
pub fn run<X: Send, Y: Send>(
    xs: impl IntoIterator<Item=X>,
    threads: usize,
    f: impl Fn(X) -> Y + Sync,
    report: impl FnMut(Y) + Send
) {
    let (tx, rx) = channel::bounded(2*threads);
    let (tx2, rx2) = channel::unbounded();

    cb_thread::scope(|scope| {
        for _ in 0..threads {
            let rxc = rx.clone();
            let txc = tx2.clone();
            let fp = &f;
            scope.spawn(move |_| {
                for x in rxc.iter() {
                    match x {
                        Some((i, x)) => {
                            let res = (i, fp(x)) ;
                            txc.send(ReportMsg::NewResult(res)).unwrap()
                        },
                        None => break,
                    }
                }
            });
        }

        scope.spawn(move |_| run_report(rx2, report));

        let mut n = 0;
        for val in xs.into_iter().enumerate() {
            n = val.0;
            tx.send(Some(val)).unwrap();
        }

        tx2.send(ReportMsg::SetTarget(n)).unwrap();

        for _ in 0..threads {
            tx.send(None).unwrap();
        }
    }).unwrap();
}
