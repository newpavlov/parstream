# parstream [![crates.io](https://img.shields.io/crates/v/parstream.svg)](https://crates.io/crates/parstream) [![Documentation](https://docs.rs/parstream/badge.svg)](https://docs.rs/parstream) [![Build Status](https://travis-ci.org/newpavlov/parstream.svg?branch=master)](https://travis-ci.org/newpavlov/parstream) [![dependency status](https://deps.rs/repo/github/newpavlov/parstream/status.svg)](https://deps.rs/repo/github/newpavlov/parstream)
Crate for computing function over iterator in a streaming fashion using
multi-threading while preserving order.

# Examples
```rust
let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
let mut ys = Vec::new();
let f = |x| x*x;
let res: Result<usize, ()> = parstream::run(
    xs, 4,
    // closure which is called for every x in xs
    |x| {
        std::thread::sleep(std::time::Duration::from_millis(*x));
        Ok(f(x))
    },
    // closure which is called for every result with preserved order
    |y| {
        ys.push(y);
        Ok(())
    },
);
assert_eq!(res, Ok(xs.len()));
assert_eq!(ys, xs.iter().map(f).collect::<Vec<_>>());
```
If one of callbacks will return error, no new tasks will be started and `run`
will end as soon as possible (after threads cleanup) to report this error to
caller.
```rust
#[derive(Eq, PartialEq, Debug)]
struct MyError(usize);
let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
let mut ys = Vec::new();
let f = |x| x*x;
let res = parstream::run(xs.iter().enumerate(), 4,
    |(i, x)| {
        std::thread::sleep(std::time::Duration::from_millis(*x));
        if *x == 0 { return Err(MyError(i)); }
        Ok(f(x))
    },
    |y| {
        ys.push(y);
        Ok(())
    },
);

assert_eq!(res, Err(MyError(5)));
assert_eq!(ys.len(), 0);
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
